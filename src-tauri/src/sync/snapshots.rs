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
/// `restorable: false`。capture 后按 agent 修剪到最近 KEEP_PER_AGENT 份。
pub fn capture(
    home: &Path,
    agent_id: &str,
    scope: &str,
    summary: &str,
    paths: &[PathBuf],
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

    prune(home, agent_id);
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

/// 每 agent 修剪到最近 KEEP_PER_AGENT 份(删整目录,尽力而为)。
fn prune(home: &Path, agent_id: &str) {
    let root = agent_snapshots_dir(home, agent_id);
    let Ok(rd) = std::fs::read_dir(&root) else { return };
    let mut ids: Vec<String> = rd
        .flatten()
        .filter(|e| e.path().is_dir() && e.path().join("manifest.json").is_file())
        .filter_map(|e| e.file_name().to_str().map(String::from))
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
}
