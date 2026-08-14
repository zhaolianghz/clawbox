//! 统一配置快照层:ClawBox 每次改写 agent 配置文件前,对「该操作可能触碰
//! 的全部路径」拍快照,用户可浏览并一键恢复。
//!
//! 存储(见 docs/superpowers/specs/2026-08-14-config-snapshots-rollback-design.md):
//!
//! ```text
//! ~/.clawbox/snapshots/<agent_id>/<id>/manifest.json
//! ~/.clawbox/snapshots/<agent_id>/<id>/blobs/0        # 文件内容(编号 = entries 下标)
//! ~/.clawbox/snapshots/<agent_id>/<id>/blobs/2/...    # 目录树(entries[2] 为 dir)
//! ```
//!
//! `<id>` = `yyyyMMdd-HHmmss[-NNN]-<scope>`(同秒冲突追加 3 位序号,保字典
//! 序 = 时间序)。entry 四种:`file`(拷内容)、`missing`(快照时不存在,
//! 恢复时删除)、`symlink`(记目标,恢复重建)、`dir`(递归拷整树,恢复
//! 精确重建)。CLI 型 agent 无文件可拍(空清单)→ `restorable: false`,
//! 恢复走人工。
//!
//! 与旧 `backup_target` 的关系:本模块取代之;`~/.clawbox/backups/` 旧目录
//! 遗留不迁移不清理。home 参数化 + TempHome 测试,铁律同 `sync` 其余模块。

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// 每 agent 保留的快照份数(设计决策:固定常量,不做配置)。
pub const KEEP_PER_AGENT: usize = 20;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SnapshotEntry {
    /// home 相对路径(恢复时拒绝绝对路径与 `..` 组件)。
    pub rel_path: String,
    /// "file" | "missing" | "symlink" | "dir"
    pub kind: String,
    /// blobs 下的编号(file/dir 有,missing/symlink 无)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blob: Option<String>,
    /// symlink 的目标(照 `read_link` 原样记录,相对目标恢复时同样成立)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
}

/// 快照清单,逐字段落盘为 manifest.json。
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Manifest {
    pub id: String,
    pub agent_id: String,
    /// "provider" | "fallback" | "mcp" | "skills" | "memory"
    pub scope: String,
    pub summary: String,
    /// false = CLI 型下发,无本地文件,恢复需人工(openclaw/hermes)。
    pub restorable: bool,
    /// RFC3339 UTC。
    pub created_at: String,
    pub entries: Vec<SnapshotEntry>,
}

/// 前端列表条目(manifest 的摘要形态,不载 blobs)。
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SnapshotInfo {
    pub id: String,
    pub agent_id: String,
    pub scope: String,
    pub summary: String,
    pub restorable: bool,
    pub created_at: String,
    /// entry 数(文件 + missing + symlink + dir 总和)。
    pub files: usize,
}

/// 恢复结果:动了哪些文件、清了哪些记账(供 UI 汇报)。
#[derive(Serialize, Clone, Debug, Default)]
pub struct RestoreResult {
    pub restored: Vec<String>,
    pub cleared: Vec<String>,
}

pub fn snapshots_dir(home: &Path) -> PathBuf {
    home.join(".clawbox").join("snapshots")
}

fn agent_snapshots_dir(home: &Path, agent_id: &str) -> PathBuf {
    snapshots_dir(home).join(agent_id)
}

fn utc_stamp() -> String {
    let now = time::OffsetDateTime::now_utc();
    format!(
        "{:04}{:02}{:02}-{:02}{:02}{:02}",
        now.year(),
        u8::from(now.month()),
        now.day(),
        now.hour(),
        now.minute(),
        now.second()
    )
}

/// 同秒冲突追加 3 位序号(-001..-999),保证字典序 = 时间序。
fn next_snapshot_id(agent_dir: &Path, scope: &str) -> String {
    let stamp = utc_stamp();
    let base = format!("{}-{}", stamp, scope);
    if !agent_dir.join(&base).exists() {
        return base;
    }
    for n in 1..=999u32 {
        let id = format!("{}-{:03}-{}", stamp, n, scope);
        if !agent_dir.join(&id).exists() {
            return id;
        }
    }
    // 同秒 999 份:纳秒兜底(实际不可能达到)。
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    format!("{}-{}-{}", stamp, nanos, scope)
}

/// 递归拷贝目录树;树内 symlink 原样重建(unix;windows 树内 symlink 报错)。
fn copy_tree(src: &Path, dst: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dst)
        .map_err(|e| format!("failed to create {}: {}", dst.display(), e))?;
    let rd = std::fs::read_dir(src).map_err(|e| format!("failed to read {}: {}", src.display(), e))?;
    for entry in rd {
        let entry = entry.map_err(|e| format!("failed to iterate {}: {}", src.display(), e))?;
        let ft = entry.file_type().map_err(|e| e.to_string())?;
        let s = entry.path();
        let d = dst.join(entry.file_name());
        if ft.is_symlink() {
            let target = std::fs::read_link(&s).map_err(|e| e.to_string())?;
            #[cfg(unix)]
            std::os::unix::fs::symlink(&target, &d)
                .map_err(|e| format!("failed to symlink {} -> {}: {}", d.display(), target.display(), e))?;
            #[cfg(windows)]
            {
                let _ = target;
                return Err(format!(
                    "symlink inside directory snapshot is not supported on this platform: {}",
                    s.display()
                ));
            }
        } else if ft.is_dir() {
            copy_tree(&s, &d)?;
        } else {
            std::fs::copy(&s, &d)
                .map_err(|e| format!("failed to copy {} -> {}: {}", s.display(), d.display(), e))?;
        }
    }
    Ok(())
}

/// 对 `paths` 逐项记录快照。空 paths(CLI 型 agent)→ 空清单 +
/// `restorable: false`。capture 后按 agent 修剪到最近 KEEP_PER_AGENT 份
/// (`keep` 里的 id 不会被修剪 —— restore 先拍安全快照再还原，需保护
/// 正在恢复的源快照不被 prune 掉)。
pub fn capture(
    home: &Path,
    agent_id: &str,
    scope: &str,
    summary: &str,
    paths: &[PathBuf],
) -> Result<SnapshotInfo, String> {
    capture_inner(home, agent_id, scope, summary, paths, &[])
}

fn capture_inner(
    home: &Path,
    agent_id: &str,
    scope: &str,
    summary: &str,
    paths: &[PathBuf],
    keep: &[String],
) -> Result<SnapshotInfo, String> {
    let agent_dir = agent_snapshots_dir(home, agent_id);
    std::fs::create_dir_all(&agent_dir)
        .map_err(|e| format!("failed to create {}: {}", agent_dir.display(), e))?;
    let id = next_snapshot_id(&agent_dir, scope);
    let dir = agent_dir.join(&id);
    let blobs = dir.join("blobs");
    std::fs::create_dir_all(&blobs)
        .map_err(|e| format!("failed to create {}: {}", blobs.display(), e))?;

    let mut entries = Vec::new();
    for p in paths {
        if p.as_os_str().is_empty() {
            continue; // CLI 型适配器的空 config_path
        }
        let rel = p
            .strip_prefix(home)
            .map_err(|_| format!("snapshot path escapes home: {}", p.display()))?
            .to_string_lossy()
            .to_string();
        let idx = entries.len().to_string();
        match std::fs::symlink_metadata(p) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                entries.push(SnapshotEntry { rel_path: rel, kind: "missing".into(), blob: None, target: None, size: None });
            }
            Err(e) => return Err(format!("failed to stat {}: {}", p.display(), e)),
            Ok(m) => {
                if m.file_type().is_symlink() {
                    let target = std::fs::read_link(p).map_err(|e| format!("failed to read link {}: {}", p.display(), e))?;
                    entries.push(SnapshotEntry {
                        rel_path: rel,
                        kind: "symlink".into(),
                        blob: None,
                        target: Some(target.to_string_lossy().to_string()),
                        size: None,
                    });
                } else if m.is_dir() {
                    copy_tree(p, &blobs.join(&idx))?;
                    entries.push(SnapshotEntry { rel_path: rel, kind: "dir".into(), blob: Some(idx), target: None, size: None });
                } else {
                    std::fs::copy(p, blobs.join(&idx))
                        .map_err(|e| format!("failed to copy {} -> blobs/{}: {}", p.display(), idx, e))?;
                    entries.push(SnapshotEntry { rel_path: rel, kind: "file".into(), blob: Some(idx), target: None, size: Some(m.len()) });
                }
            }
        }
    }

    let manifest = Manifest {
        id: id.clone(),
        agent_id: agent_id.to_string(),
        scope: scope.to_string(),
        summary: summary.to_string(),
        restorable: !entries.is_empty(),
        created_at: time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_default(),
        entries,
    };
    let text = serde_json::to_string_pretty(&manifest)
        .map_err(|e| format!("failed to serialize manifest: {}", e))?;
    std::fs::write(dir.join("manifest.json"), text)
        .map_err(|e| format!("failed to write manifest.json: {}", e))?;

    prune(home, agent_id, keep);
    Ok(manifest.info())
}

impl Manifest {
    fn info(&self) -> SnapshotInfo {
        SnapshotInfo {
            id: self.id.clone(),
            agent_id: self.agent_id.clone(),
            scope: self.scope.clone(),
            summary: self.summary.clone(),
            restorable: self.restorable,
            created_at: self.created_at.clone(),
            files: self.entries.len(),
        }
    }
}

fn read_manifest(dir: &Path) -> Option<Manifest> {
    let text = std::fs::read_to_string(dir.join("manifest.json")).ok()?;
    match serde_json::from_str(&text) {
        Ok(m) => Some(m),
        Err(e) => {
            eprintln!("[clawbox] snapshots: skipping corrupt manifest {}: {}", dir.display(), e);
            None
        }
    }
}

/// 列快照(只读 manifest)。agent_id=None 列全部 agent;按 id 倒序
/// (id 前缀时间戳保证字典序 = 时间序)。损坏的 manifest 跳过并记日志。
pub fn list(home: &Path, agent_id: Option<&str>) -> Vec<SnapshotInfo> {
    let roots: Vec<PathBuf> = match agent_id {
        Some(id) => vec![agent_snapshots_dir(home, id)],
        None => match std::fs::read_dir(snapshots_dir(home)) {
            Ok(rd) => rd
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect(),
            Err(_) => vec![],
        },
    };
    let mut out = Vec::new();
    for root in roots {
        let Ok(rd) = std::fs::read_dir(&root) else { continue };
        for snap_dir in rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()) {
            if let Some(m) = read_manifest(&snap_dir) {
                out.push(m.info());
            }
        }
    }
    out.sort_by(|a, b| b.id.cmp(&a.id));
    out
}

// ---- restore --------------------------------------------------------------

/// rel_path 安全校验:必须 home 相对、拒绝绝对路径与 `..` 组件(防手改
/// manifest 后的路径逃逸)。
fn safe_rel(rel: &str) -> Result<PathBuf, String> {
    let p = Path::new(rel);
    if rel.starts_with('/') || rel.starts_with('\\') || p.is_absolute() {
        return Err(format!("snapshot entry escapes home: {}", rel));
    }
    if p.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return Err(format!("snapshot entry escapes home: {}", rel));
    }
    Ok(p.to_path_buf())
}

/// 删掉现存对象(文件/软链/目录任一);不存在则不动。
fn remove_any(path: &Path) -> Result<(), String> {
    match std::fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("failed to stat {}: {}", path.display(), e)),
        Ok(m) => {
            if m.is_dir() && !m.file_type().is_symlink() {
                std::fs::remove_dir_all(path)
                    .map_err(|e| format!("failed to remove {}: {}", path.display(), e))
            } else {
                std::fs::remove_file(path)
                    .map_err(|e| format!("failed to remove {}: {}", path.display(), e))
            }
        }
    }
}

/// 原子写文件:同目录 `.clawbox-swap` 临时文件 + rename。
fn atomic_write(path: &Path, content: &[u8]) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("failed to create {}: {}", dir.display(), e))?;
    }
    let tmp = path.with_extension("clawbox-swap");
    std::fs::write(&tmp, content).map_err(|e| format!("failed to write {}: {}", tmp.display(), e))?;
    std::fs::rename(&tmp, path)
        .map_err(|e| format!("failed to rename {} -> {}: {}", tmp.display(), path.display(), e))
}

fn load_manifest(home: &Path, agent_id: &str, snapshot_id: &str) -> Result<Manifest, String> {
    let dir = agent_snapshots_dir(home, agent_id).join(snapshot_id);
    read_manifest(&dir).ok_or_else(|| format!("snapshot not found: {}", snapshot_id))
}

/// 恢复单个 entry 到 home 下的原始位置。返回恢复的 rel_path。
fn restore_entry(home: &Path, snap_dir: &Path, e: &SnapshotEntry) -> Result<String, String> {
    let rel = safe_rel(&e.rel_path)?;
    let target = home.join(&rel);
    match e.kind.as_str() {
        "file" => {
            let blob = snap_dir.join("blobs").join(e.blob.as_deref().unwrap_or_default());
            let content = std::fs::read(&blob)
                .map_err(|err| format!("failed to read blob {}: {}", blob.display(), err))?;
            atomic_write(&target, &content)?;
        }
        "missing" => {
            remove_any(&target)?;
        }
        "symlink" => {
            let dst = e.target.as_deref().ok_or_else(|| format!("symlink entry without target: {}", e.rel_path))?;
            remove_any(&target)?;
            if let Some(dir) = target.parent() {
                std::fs::create_dir_all(dir).map_err(|err| format!("failed to create {}: {}", dir.display(), err))?;
            }
            #[cfg(unix)]
            std::os::unix::fs::symlink(dst, &target).map_err(|err| {
                format!("failed to symlink {} -> {}: {}", target.display(), dst, err)
            })?;
            #[cfg(windows)]
            return Err(format!(
                "symlink restore is not supported on this platform: {}",
                e.rel_path
            ));
        }
        "dir" => {
            let blob = snap_dir.join("blobs").join(e.blob.as_deref().unwrap_or_default());
            if !blob.is_dir() {
                return Err(format!("missing dir blob for {}", e.rel_path));
            }
            remove_any(&target)?;
            copy_tree(&blob, &target)?;
        }
        other => return Err(format!("unknown snapshot entry kind: {}", other)),
    }
    Ok(e.rel_path.clone())
}

/// 恢复指定 entries(全部或 rel_paths 子集)。CLI 型(restorable=false)
/// 拒绝恢复。
fn restore_entries(
    home: &Path,
    agent_id: &str,
    snapshot_id: &str,
    rel_paths: &[String],
) -> Result<Vec<String>, String> {
    let manifest = load_manifest(home, agent_id, snapshot_id)?;
    if !manifest.restorable {
        return Err(format!(
            "snapshot {} was taken for a CLI-backed change; restore it manually via the {} CLI",
            snapshot_id, agent_id
        ));
    }
    let snap_dir = agent_snapshots_dir(home, agent_id).join(snapshot_id);
    let mut restored = Vec::new();
    for e in &manifest.entries {
        if !rel_paths.is_empty() && !rel_paths.contains(&e.rel_path) {
            continue;
        }
        restored.push(restore_entry(home, &snap_dir, e)?);
    }
    Ok(restored)
}

/// 按快照 manifest 的 entries 恢复指定 rel_paths(供 apply 后校验失败的
/// 事务性回滚;不拍安全快照、不动记账)。
pub fn restore_paths(
    home: &Path,
    agent_id: &str,
    snapshot_id: &str,
    rel_paths: &[String],
) -> Result<Vec<String>, String> {
    restore_entries(home, agent_id, snapshot_id, rel_paths)
}

/// scope → 恢复后要清的记账字段(见 spec 映射表)。只报实际清掉的
/// 字段名(供 UI 汇报);未变则不落盘。
fn clear_bookkeeping(home: &Path, agent_id: &str, scope: &str) -> Result<Vec<String>, String> {
    use crate::commands::config::{load_config, save_config};
    let mut config = load_config(home)?;
    let agent = agent_id.to_string();
    let mut cleared = Vec::new();
    let mut had = |map: &mut std::collections::HashMap<String, Vec<String>>| map.remove(&agent).is_some();
    match scope {
        "provider" => {
            if config.agent_providers.remove(&agent).is_some() {
                cleared.push("agent_providers".into());
            }
            if had(&mut config.providers_managed) {
                cleared.push("providers_managed".into());
            }
        }
        "fallback" => {
            if config.agent_fallbacks.remove(&agent).is_some() {
                cleared.push("agent_fallbacks".into());
            }
            if had(&mut config.providers_fallback_managed) {
                cleared.push("providers_fallback_managed".into());
            }
        }
        "mcp" => {
            if had(&mut config.mcp_managed) {
                cleared.push("mcp_managed".into());
            }
        }
        "skills" => {
            if had(&mut config.skills_managed) {
                cleared.push("skills_managed".into());
            }
        }
        "memory" => {
            if had(&mut config.memory_managed) {
                cleared.push("memory_managed".into());
            }
        }
        other => return Err(format!("unknown snapshot scope: {}", other)),
    }
    if !cleared.is_empty() {
        save_config(home, &config)?;
    }
    Ok(cleared)
}

/// 恢复到指定快照时刻:
/// 1. 先对当前状态拍 pre-restore 安全快照(撤销也可撤销)
/// 2. 逐 entry 还原(file 写回 / missing 删现存 / symlink 重建 / dir 精确重建)
/// 3. 清掉该 agent 对应维度的 managed 记账与绑定 —— 否则启动对账会把
///    回滚当漂移"安全自愈",悄悄撤销用户的撤销
///
/// CLI 型快照(restorable=false)返回明确错误引导人工。
pub fn restore(home: &Path, agent_id: &str, snapshot_id: &str) -> Result<RestoreResult, String> {
    let manifest = load_manifest(home, agent_id, snapshot_id)?;
    if !manifest.restorable {
        return Err(format!(
            "snapshot {} was taken for a CLI-backed change; restore it manually via the {} CLI",
            snapshot_id, agent_id
        ));
    }
    // 安全快照:同 paths、同 scope;keep 保护源快照不被 prune
    let paths: Vec<PathBuf> = manifest
        .entries
        .iter()
        .map(|e| safe_rel(&e.rel_path).map(|rel| home.join(rel)))
        .collect::<Result<_, _>>()?;
    capture_inner(
        home,
        agent_id,
        &manifest.scope,
        "pre-restore safety",
        &paths,
        &[snapshot_id.to_string()],
    )?;

    let restored = restore_entries(home, agent_id, snapshot_id, &[])?;
    let cleared = clear_bookkeeping(home, agent_id, &manifest.scope)?;
    Ok(RestoreResult { restored, cleared })
}

/// 每 agent 修剪到最近 KEEP_PER_AGENT 份(删整目录,尽力而为);
/// `keep` 中的 id 免删。
fn prune(home: &Path, agent_id: &str, keep: &[String]) {
    let root = agent_snapshots_dir(home, agent_id);
    let Ok(rd) = std::fs::read_dir(&root) else { return };
    let mut ids: Vec<String> = rd
        .flatten()
        .filter(|e| e.path().is_dir() && e.path().join("manifest.json").is_file())
        .filter_map(|e| e.file_name().to_str().map(String::from))
        .filter(|id| !keep.contains(id))
        .collect();
    if ids.len() <= KEEP_PER_AGENT {
        return;
    }
    ids.sort(); // 字典序 = 时间序
    let excess = ids.len() - KEEP_PER_AGENT;
    for id in ids.into_iter().take(excess) {
        if let Err(e) = std::fs::remove_dir_all(root.join(&id)) {
            eprintln!("[clawbox] snapshots: failed to prune {}: {}", id, e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::test_util::TempHome;
    use std::fs;

    fn entry_of<'a>(m: &'a Manifest, rel: &str) -> &'a SnapshotEntry {
        m.entries
            .iter()
            .find(|e| e.rel_path == rel)
            .unwrap_or_else(|| panic!("no entry for {}", rel))
    }

    fn manifest_of(home: &Path, agent_id: &str, id: &str) -> Manifest {
        read_manifest(&agent_snapshots_dir(home, agent_id).join(id)).expect("manifest readable")
    }

    #[test]
    fn capture_records_file_missing_symlink_and_dir() {
        let home = TempHome::new();
        let h = home.path();
        // file / symlink / dir(含嵌套)/ missing 四种对象
        fs::create_dir_all(h.join(".codex")).unwrap();
        fs::write(h.join(".codex").join("config.toml"), "key = 1\n").unwrap();
        let lib = h.join(".clawbox").join("skills").join("library").join("foo");
        fs::create_dir_all(&lib).unwrap();
        fs::write(lib.join("SKILL.md"), "skill").unwrap();
        #[cfg(unix)]
        {
            fs::create_dir_all(h.join(".opencode").join("skills")).unwrap();
            std::os::unix::fs::symlink(&lib, h.join(".opencode").join("skills").join("foo")).unwrap();
        }
        fs::create_dir_all(h.join(".opencode").join("skills").join("bar")).unwrap();
        fs::write(h.join(".opencode").join("skills").join("bar").join("x.md"), "x").unwrap();

        let paths = vec![
            h.join(".codex").join("config.toml"),
            h.join(".codex").join("auth.json"), // 不存在
            h.join(".opencode").join("skills").join("foo"),
            h.join(".opencode").join("skills").join("bar"),
        ];
        let info = capture(h, "codex", "provider", "test", &paths).unwrap();
        assert!(info.restorable);
        assert_eq!(info.files, 4);

        let m = manifest_of(h, "codex", &info.id);
        let f = entry_of(&m, ".codex/config.toml");
        assert_eq!((f.kind.as_str(), f.blob.as_deref()), ("file", Some("0")));
        assert_eq!(f.size, Some(8));
        assert_eq!(
            fs::read_to_string(agent_snapshots_dir(h, "codex").join(&info.id).join("blobs").join("0")).unwrap(),
            "key = 1\n"
        );

        let miss = entry_of(&m, ".codex/auth.json");
        assert_eq!((miss.kind.as_str(), miss.blob.is_none()), ("missing", true));

        #[cfg(unix)]
        {
            let link = entry_of(&m, ".opencode/skills/foo");
            assert_eq!(link.kind, "symlink");
            assert_eq!(
                link.target.as_deref(),
                Some(h.join(".clawbox").join("skills").join("library").join("foo").to_string_lossy().as_ref())
            );
        }

        let d = entry_of(&m, ".opencode/skills/bar");
        assert_eq!((d.kind.as_str(), d.blob.as_deref()), ("dir", Some("3")));
        assert_eq!(
            fs::read_to_string(agent_snapshots_dir(h, "codex").join(&info.id).join("blobs").join("3").join("x.md")).unwrap(),
            "x"
        );
    }

    #[test]
    fn capture_empty_or_blank_paths_marks_unrestorable() {
        let home = TempHome::new();
        let h = home.path();
        // CLI 型:空列表 / 空路径占位
        let a = capture(h, "openclaw", "mcp", "cli", &[]).unwrap();
        assert!(!a.restorable);
        assert_eq!(a.files, 0);
        let b = capture(h, "hermes", "provider", "cli", &[PathBuf::new()]).unwrap();
        assert!(!b.restorable);
        assert_eq!(b.files, 0);
    }

    #[test]
    fn capture_rejects_paths_outside_home() {
        let home = TempHome::new();
        let err = capture(home.path(), "codex", "mcp", "x", &[PathBuf::from("/etc/passwd")]).unwrap_err();
        assert!(err.contains("escapes home"), "{}", err);
    }

    #[test]
    fn list_descends_and_prune_keeps_recent() {
        let home = TempHome::new();
        let h = home.path();
        // 同秒连拍 25 份 → -001..-024 序号;只应留最近 20
        for i in 0..25 {
            let p = h.join(".m").join(format!("cfg-{}.json", i));
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(&p, format!("{}", i)).unwrap();
            capture(h, "codex", "mcp", &format!("round {}", i), &[p]).unwrap();
        }
        let ids = list(h, Some("codex"));
        assert_eq!(ids.len(), KEEP_PER_AGENT);
        // 倒序:最新的在前(id 越大越新,含序号)
        assert!(ids.windows(2).all(|w| w[0].id > w[1].id), "{:?}", ids.iter().map(|i| &i.id).collect::<Vec<_>>());
        // 磁盘上也只剩 20 份
        let on_disk = fs::read_dir(agent_snapshots_dir(h, "codex"))
            .unwrap()
            .flatten()
            .filter(|e| e.path().is_dir())
            .count();
        assert_eq!(on_disk, KEEP_PER_AGENT);
        // 全 agent 列表(None)也能带出来
        assert_eq!(list(h, None).len(), KEEP_PER_AGENT);
    }

    #[test]
    fn corrupt_manifest_is_skipped_not_fatal() {
        let home = TempHome::new();
        let h = home.path();
        capture(h, "codex", "mcp", "ok", &[{
            let p = h.join(".m").join("cfg.json");
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(&p, "1").unwrap();
            p
        }])
        .unwrap();
        // 再拍一份然后弄坏它的 manifest
        let info2 = capture(h, "codex", "mcp", "bad", &[h.join(".m").join("cfg.json")]).unwrap();
        fs::write(
            agent_snapshots_dir(h, "codex").join(&info2.id).join("manifest.json"),
            "{ TRUNCATED",
        )
        .unwrap();
        let ids = list(h, Some("codex"));
        assert_eq!(ids.len(), 1);
        assert_eq!(ids[0].summary, "ok");
    }

    // ---- restore ----

    fn cfg_file(h: &Path) -> PathBuf {
        let p = h.join(".codex").join("config.toml");
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        p
    }

    #[test]
    fn restore_roundtrips_file_content_and_takes_safety_snapshot() {
        let home = TempHome::new();
        let h = home.path();
        let p = cfg_file(h);
        fs::write(&p, "original").unwrap();
        let snap = capture(h, "codex", "provider", "bind", &[p.clone()]).unwrap();

        // apply 改坏了 + 恢复
        fs::write(&p, "broken").unwrap();
        let r = restore(h, "codex", &snap.id).unwrap();
        assert_eq!(r.restored, vec![".codex/config.toml".to_string()]);
        assert_eq!(fs::read_to_string(&p).unwrap(), "original");

        // pre-restore 安全快照存在,内容是恢复前的 broken 状态
        let ids = list(h, Some("codex"));
        let safety = ids.iter().find(|i| i.summary == "pre-restore safety").expect("safety snapshot");
        let m = manifest_of(h, "codex", &safety.id);
        let blob = agent_snapshots_dir(h, "codex").join(&safety.id).join("blobs").join("0");
        assert_eq!(fs::read_to_string(blob).unwrap(), "broken");
        assert_eq!(m.entries.len(), 1);
    }

    #[test]
    fn restore_deletes_files_that_did_not_exist_and_recreates_missing() {
        let home = TempHome::new();
        let h = home.path();
        let existed = cfg_file(h);
        fs::write(&existed, "keep").unwrap();
        let created = h.join(".codex").join("auth.json"); // 快照时不存在
        let snap = capture(h, "codex", "provider", "bind", &[existed.clone(), created.clone()]).unwrap();

        // apply 后:新文件出现、旧文件被删
        fs::write(&created, "{}").unwrap();
        fs::remove_file(&existed).unwrap();

        restore(h, "codex", &snap.id).unwrap();
        assert!(!created.exists(), "apply 产物应被删除");
        assert_eq!(fs::read_to_string(&existed).unwrap(), "keep");
    }

    #[cfg(unix)]
    #[test]
    fn restore_recreates_symlinks_exactly() {
        let home = TempHome::new();
        let h = home.path();
        let lib = h.join(".clawbox").join("skills").join("library").join("foo");
        fs::create_dir_all(&lib).unwrap();
        let link = h.join(".opencode").join("skills").join("foo");
        fs::create_dir_all(link.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&lib, &link).unwrap();

        let snap = capture(h, "opencode", "skills", "link", &[link.clone()]).unwrap();
        // apply 换成了真目录
        fs::remove_file(&link).unwrap();
        fs::create_dir_all(&link).unwrap();
        fs::write(link.join("junk"), "junk").unwrap();

        restore(h, "opencode", &snap.id).unwrap();
        assert!(link.is_symlink(), "应恢复为软链而非目录");
        assert_eq!(std::fs::read_link(&link).unwrap(), lib);
    }

    #[test]
    fn restore_dir_is_exact_time_machine() {
        let home = TempHome::new();
        let h = home.path();
        let dir = h.join(".opencode").join("skills");
        fs::create_dir_all(dir.join("bar")).unwrap();
        fs::write(dir.join("bar").join("x.md"), "x").unwrap();

        let snap = capture(h, "opencode", "skills", "dir", &[dir.clone()]).unwrap();
        // 之后:x 被删、y 新增
        fs::remove_file(dir.join("bar").join("x.md")).unwrap();
        fs::write(dir.join("bar").join("y.md"), "y").unwrap();

        restore(h, "opencode", &snap.id).unwrap();
        assert!(dir.join("bar").join("x.md").is_file(), "快照时存在的应回来");
        assert!(!dir.join("bar").join("y.md").exists(), "快照后新增的应被删(精确恢复)");
    }

    #[test]
    fn restore_rejects_unrestorable_and_escapes() {
        let home = TempHome::new();
        let h = home.path();
        // CLI 型快照拒绝恢复
        let cli = capture(h, "openclaw", "mcp", "cli", &[]).unwrap();
        let err = restore(h, "openclaw", &cli.id).unwrap_err();
        assert!(err.contains("manually"), "{}", err);

        // 手改 manifest 逃逸:绝对路径 / ..
        let p = cfg_file(h);
        fs::write(&p, "1").unwrap();
        let snap = capture(h, "codex", "provider", "x", &[p.clone()]).unwrap();
        for evil in ["/etc/passwd", "../../evil"] {
            let mdir = agent_snapshots_dir(h, "codex").join(&snap.id);
            let m = read_manifest(&mdir).unwrap();
            let mut bad = m.clone();
            bad.entries[0].rel_path = evil.to_string();
            fs::write(
                mdir.join("manifest.json"),
                serde_json::to_string_pretty(&bad).unwrap(),
            )
            .unwrap();
            let err = restore(h, "codex", &snap.id).unwrap_err();
            assert!(err.contains("escapes home"), "{}", err);
        }
    }

    #[test]
    fn restore_clears_bookkeeping_for_its_scope_only() {
        let home = TempHome::new();
        let h = home.path();
        use crate::commands::config::{load_config, save_config};

        let mut config = crate::commands::config::Config::default();
        config.agent_providers.insert("codex".into(), "p1".into());
        config.providers_managed.insert("codex".into(), vec!["env".into()]);
        config.mcp_managed.insert("codex".into(), vec!["srv".into()]);
        config.agent_providers.insert("claude-code".into(), "p2".into());
        save_config(h, &config).unwrap();

        let p = cfg_file(h);
        fs::write(&p, "1").unwrap();
        let snap = capture(h, "codex", "provider", "bind", &[p]).unwrap();

        let r = restore(h, "codex", &snap.id).unwrap();
        assert!(r.cleared.contains(&"agent_providers".to_string()));
        assert!(r.cleared.contains(&"providers_managed".to_string()));

        let after = load_config(h).unwrap();
        assert!(!after.agent_providers.contains_key("codex"));
        assert!(!after.providers_managed.contains_key("codex"));
        // 其它维度 / 其它 agent 不受影响
        assert_eq!(after.mcp_managed.get("codex").map(|v| v.len()), Some(1));
        assert_eq!(after.agent_providers.get("claude-code").map(String::as_str), Some("p2"));

        // 无记账可清时 cleared 为空且不落盘
        let snap2 = capture(h, "codex", "memory", "mem", &[{
            let q = h.join(".codex").join("MEMORY.md");
            fs::write(&q, "m").unwrap();
            q
        }])
        .unwrap();
        let r2 = restore(h, "codex", &snap2.id).unwrap();
        assert!(r2.cleared.is_empty());
    }

    #[test]
    fn restore_paths_restores_subset_without_safety_or_bookkeeping() {
        let home = TempHome::new();
        let h = home.path();
        use crate::commands::config::{load_config, save_config};

        let a = cfg_file(h);
        fs::write(&a, "A").unwrap();
        let b = h.join(".codex").join("auth.json");
        fs::write(&b, "B").unwrap();
        let snap = capture(h, "codex", "provider", "x", &[a.clone(), b.clone()]).unwrap();

        let mut config = crate::commands::config::Config::default();
        config.agent_providers.insert("codex".into(), "p1".into());
        save_config(h, &config).unwrap();

        fs::write(&a, "broken").unwrap();
        fs::write(&b, "broken").unwrap();
        let restored = restore_paths(h, "codex", &snap.id, &[".codex/config.toml".to_string()]).unwrap();
        assert_eq!(restored, vec![".codex/config.toml".to_string()]);
        assert_eq!(fs::read_to_string(&a).unwrap(), "A");
        assert_eq!(fs::read_to_string(&b).unwrap(), "broken", "未选中的 entry 不动");
        // 不拍安全快照、不清记账
        assert!(!list(h, Some("codex")).iter().any(|i| i.summary == "pre-restore safety"));
        assert!(load_config(h).unwrap().agent_providers.contains_key("codex"));
    }
}
