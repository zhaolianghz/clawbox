//! 统一指令记忆同步。
//!
//! 真源 = `~/.agents/memory/MEMORY.md`(单 Markdown 文件;目录/文件不存在
//! 视为空库)。下发 = 在各 agent 指令文件中维护一个托管区块:
//!
//! ```text
//! <!-- CLAWBOX_START -->
//! <库文件内容原样>
//! <!-- CLAWBOX_END -->
//! ```
//!
//! 合并写铁律:区块外内容一字不动。文件不存在 → 创建;无区块 → 末尾追加
//! (前补一个空行);有区块 → 只替换区块内容;标记重复或残缺 → 该 agent
//! plan error,绝不动文件。
//!
//! 支持矩阵(指令文件):claude-code `~/.claude/CLAUDE.md`、codex
//! `~/.codex/AGENTS.md`、opencode `~/.config/opencode/AGENTS.md`、hermes
//! `~/.hermes/memories/MEMORY.md`;openclaw `~/.openclaw/workspace/MEMORY.md`
//! —— 查证:`openclaw memory status` 输出 `Workspace: ~/.openclaw/workspace`
//! (`agents.defaults.workspace` 未配置时的默认值),workspace 下的
//! MEMORY.md 即其记忆文件约定。其余 agent unsupported。

use super::{AgentPlan, ApplyResult, ChangeItem};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub const BLOCK_START: &str = "<!-- CLAWBOX_START -->";
pub const BLOCK_END: &str = "<!-- CLAWBOX_END -->";
/// memory_managed 里的标记:表示我们在该 agent 指令文件里管理过托管区块。
const MANAGED_MARK: &str = "block";
/// ChangeItem 统一条目名。
const ITEM_NAME: &str = "memory";

/// 支持记忆同步的 agent 及其指令文件(home 相对)。
const MEMORY_FILES: [(&str, &[&str]); 5] = [
    ("claude-code", &[".claude", "CLAUDE.md"]),
    ("codex", &[".codex", "AGENTS.md"]),
    ("openclaw", &[".openclaw", "workspace", "MEMORY.md"]),
    ("opencode", &[".config", "opencode", "AGENTS.md"]),
    ("hermes", &[".hermes", "memories", "MEMORY.md"]),
];

/// 注册表全集,与 providers 注册表同序。
const ALL_AGENTS: [&str; 9] = [
    "claude-code",
    "codex",
    "openclaw",
    "opencode",
    "codebuddy",
    "cursor-agent",
    "kimi",
    "qodercli",
    "hermes",
];

#[derive(Serialize, Clone, Debug)]
pub struct MemoryTarget {
    pub agent_id: String,
    pub path: String,
    pub exists: bool,
    pub has_block: bool,
    /// 托管区块之外的内容字符数(收编参考;无区块/残缺 = 全文)。
    pub outside_chars: usize,
}

pub fn library_path(home: &Path) -> PathBuf {
    home.join(".agents").join("memory").join("MEMORY.md")
}

pub fn agent_memory_path(home: &Path, agent_id: &str) -> Option<PathBuf> {
    MEMORY_FILES
        .iter()
        .find(|(id, _)| *id == agent_id)
        .map(|(_, rel)| rel.iter().fold(home.to_path_buf(), |p, s| p.join(s)))
}

/// 读库;文件不存在 = 空库。
pub fn read_library(home: &Path) -> Result<String, String> {
    let path = library_path(home);
    if !path.exists() {
        return Ok(String::new());
    }
    std::fs::read_to_string(&path).map_err(|e| format!("failed to read {}: {}", path.display(), e))
}

/// 写库;文件已存在先备份到 ~/.clawbox/backups/<stamp>/memory-lib/MEMORY.md。
/// 返回备份路径(有备份时)。
pub fn write_library(home: &Path, content: &str) -> Result<Option<String>, String> {
    let path = library_path(home);
    let mut backup = None;
    if path.is_file() {
        let dest_dir = home
            .join(".clawbox")
            .join("backups")
            .join(super::skills::backup_stamp())
            .join("memory-lib");
        std::fs::create_dir_all(&dest_dir)
            .map_err(|e| format!("failed to create {}: {}", dest_dir.display(), e))?;
        let dest = dest_dir.join("MEMORY.md");
        std::fs::copy(&path, &dest)
            .map_err(|e| format!("failed to back up {}: {}", path.display(), e))?;
        backup = Some(dest.to_string_lossy().to_string());
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("failed to create {}: {}", dir.display(), e))?;
    }
    std::fs::write(&path, content).map_err(|e| format!("failed to write {}: {}", path.display(), e))?;
    Ok(backup)
}

// ---- 托管区块纯函数 ----------------------------------------------------------

/// 找唯一托管区块:Ok(None)=没有;Ok(Some((full_range, inner)))=恰好一个;
/// 标记重复、残缺(有 START 无 END / 顺序颠倒)→ Err,调用方绝不动文件。
fn scan_block(text: &str) -> Result<Option<(std::ops::Range<usize>, String)>, String> {
    let starts: Vec<usize> = text.match_indices(BLOCK_START).map(|(i, _)| i).collect();
    let ends: Vec<usize> = text.match_indices(BLOCK_END).map(|(i, _)| i).collect();
    match (starts.len(), ends.len()) {
        (0, 0) => Ok(None),
        (1, 1) if starts[0] < ends[0] => {
            let full = starts[0]..ends[0] + BLOCK_END.len();
            let inner_start = starts[0] + BLOCK_START.len();
            // 标记行后的换行属于结构,不属于 inner
            let inner_start = if text[inner_start..].starts_with('\n') { inner_start + 1 } else { inner_start };
            let inner = text[inner_start..ends[0]].to_string();
            Ok(Some((full, inner)))
        }
        _ => Err("Managed block markers are broken or duplicated (CLAWBOX_START/END); refusing to modify this file".to_string()),
    }
}

/// 库内容规范化:非空则保证尾换行(区块结构需要)。
fn normalized(lib: &str) -> String {
    if lib.is_empty() || lib.ends_with('\n') {
        lib.to_string()
    } else {
        format!("{}\n", lib)
    }
}

/// 期望的区块全文(含标记)。
fn render_block(lib: &str) -> String {
    format!("{}\n{}{}", BLOCK_START, normalized(lib), BLOCK_END)
}

/// 注入/更新区块:无区块 → 末尾追加(原内容与区块之间空一行);有区块 →
/// 只替换区块;区块外一字不动。
fn upsert_block(existing: &str, lib: &str) -> Result<String, String> {
    let block = render_block(lib);
    match scan_block(existing)? {
        Some((range, _)) => {
            Ok(format!("{}{}{}", &existing[..range.start], block, &existing[range.end..]))
        }
        None => {
            if existing.is_empty() {
                return Ok(format!("{}\n", block));
            }
            let head = existing.strip_suffix('\n').unwrap_or(existing);
            Ok(format!("{}\n\n{}\n", head, block))
        }
    }
}

/// 整块删除(含标记):吞掉我们追加时补的分隔空行与块尾换行,尽量还原
/// 注入前的原文。没有区块 → Ok(None)(无事可做)。
fn remove_block(existing: &str) -> Result<Option<String>, String> {
    let Some((range, _)) = scan_block(existing)? else {
        return Ok(None);
    };
    let mut start = range.start;
    let mut end = range.end;
    // 块尾换行
    if existing[end..].starts_with('\n') {
        end += 1;
    }
    // 块前分隔空行("\n\n" → 留一个 \n)
    if existing[..start].ends_with("\n\n") {
        start -= 1;
    }
    Ok(Some(format!("{}{}", &existing[..start], &existing[end..])))
}

// ---- plan / apply -----------------------------------------------------------

fn read_target(path: &Path) -> Result<Option<String>, String> {
    if !path.exists() {
        return Ok(None);
    }
    std::fs::read_to_string(path)
        .map(Some)
        .map_err(|e| format!("failed to read {}: {}", path.display(), e))
}

/// 单 agent 计划。库空:曾管理且区块仍在 → remove,否则 skip。
fn plan_one(path: &Path, lib: &str, managed: &[String]) -> Result<Vec<ChangeItem>, String> {
    let existing = read_target(path)?;
    let item = |action: &str, detail: String| ChangeItem {
        name: ITEM_NAME.into(),
        action: action.into(),
        detail,
    };
    let change = if lib.is_empty() {
        let has_block = match &existing {
            Some(text) => scan_block(text)?.is_some(),
            None => false,
        };
        if managed.iter().any(|m| m == MANAGED_MARK) && has_block {
            item("remove", "no longer managed by ClawBox".into())
        } else {
            item("skip", "Memory library is empty".into())
        }
    } else {
        match &existing {
            None => item("add", "Create file with managed block".into()),
            Some(text) => match scan_block(text)? {
                None => item("add", "Append managed block at end of file".into()),
                Some((_, inner)) if inner == normalized(lib) => item("unchanged", String::new()),
                Some(_) => item("update", "Managed block content is outdated".into()),
            },
        }
    };
    Ok(vec![change])
}

/// 为注册表全部 agent 生成记忆同步计划;库读取失败落在各支持 agent 的
/// error 上,不炸整体。
pub fn plan_all(home: &Path, managed: &HashMap<String, Vec<String>>) -> Vec<AgentPlan> {
    let library = read_library(home);
    ALL_AGENTS
        .iter()
        .map(|id| {
            let agent_id = id.to_string();
            let Some(path) = agent_memory_path(home, id) else {
                return AgentPlan {
                    agent_id,
                    supported: false,
                    config_path: String::new(),
                    changes: vec![],
                    error: None,
                };
            };
            let config_path = path.to_string_lossy().to_string();
            let empty = vec![];
            let m = managed.get(*id).unwrap_or(&empty);
            let planned = library
                .as_ref()
                .map_err(|e| e.clone())
                .and_then(|lib| plan_one(&path, lib, m));
            match planned {
                Ok(changes) => AgentPlan { agent_id, supported: true, config_path, changes, error: None },
                Err(e) => AgentPlan {
                    agent_id,
                    supported: true,
                    config_path,
                    changes: vec![],
                    error: Some(e),
                },
            }
        })
        .collect()
}

/// 对单个 agent 应用:注入/更新/删除托管区块。返回变更数(0 或 1)。
pub fn apply_agent(home: &Path, agent_id: &str, managed: &[String]) -> Result<usize, String> {
    let path = agent_memory_path(home, agent_id)
        .ok_or_else(|| format!("{} memory sync is not supported", agent_id))?;
    let lib = read_library(home)?;
    let existing = read_target(&path)?;
    let new_content = if lib.is_empty() {
        // 只有曾管理过才清理;残缺标记同样拒动(remove_block 内部 Err)。
        if !managed.iter().any(|m| m == MANAGED_MARK) {
            return Ok(0);
        }
        match &existing {
            None => return Ok(0),
            Some(text) => match remove_block(text)? {
                None => return Ok(0),
                Some(new_text) => new_text,
            },
        }
    } else {
        let current = existing.clone().unwrap_or_default();
        let new_text = upsert_block(&current, &lib)?;
        if existing.is_some() && new_text == current {
            return Ok(0);
        }
        new_text
    };
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("failed to create {}: {}", dir.display(), e))?;
    }
    std::fs::write(&path, new_content)
        .map_err(|e| format!("failed to write {}: {}", path.display(), e))?;
    Ok(1)
}

/// apply 成功后应记入 memory_managed 的标记:目标文件现有我们的区块 →
/// ["block"],否则空。
pub fn deployed_names(home: &Path, agent_id: &str) -> Vec<String> {
    let Some(path) = agent_memory_path(home, agent_id) else {
        return vec![];
    };
    let has_block = read_target(&path)
        .ok()
        .flatten()
        .and_then(|t| scan_block(&t).ok().flatten())
        .is_some();
    if has_block { vec![MANAGED_MARK.to_string()] } else { vec![] }
}

/// 应用到一个 agent:快照、应用、汇报。调用方成功后更新
/// memory_managed。
pub fn apply_one(home: &Path, agent_id: &str, managed: &[String]) -> ApplyResult {
    let id = agent_id.to_string();
    let Some(path) = agent_memory_path(home, agent_id) else {
        return ApplyResult {
            agent_id: id,
            ok: false,
            snapshot_id: None,
            applied: 0,
            error: Some("agent not supported for memory sync".to_string()),
        };
    };
    let snapshot_id = match super::snapshots::capture(home, agent_id, "memory", "memory sync", &[path]) {
        Ok(s) => Some(s.id),
        Err(e) => {
            return ApplyResult { agent_id: id, ok: false, snapshot_id: None, applied: 0, error: Some(e) }
        }
    };
    match apply_agent(home, agent_id, managed) {
        Ok(applied) => ApplyResult { agent_id: id, ok: true, snapshot_id, applied, error: None },
        Err(e) => ApplyResult { agent_id: id, ok: false, snapshot_id, applied: 0, error: Some(e) },
    }
}

/// 各支持 agent 的指令文件概况(收编参考)。
pub fn targets(home: &Path) -> Result<Vec<MemoryTarget>, String> {
    MEMORY_FILES
        .iter()
        .map(|(agent_id, _)| {
            let path = agent_memory_path(home, agent_id).unwrap();
            let text = read_target(&path)?;
            let (exists, has_block, outside_chars) = match &text {
                None => (false, false, 0),
                Some(t) => match scan_block(t) {
                    Ok(Some((range, _))) => {
                        (true, true, t.chars().count() - t[range.clone()].chars().count())
                    }
                    // 残缺/重复标记:按"无有效区块"报告,全文算区块外
                    _ => (true, false, t.chars().count()),
                },
            };
            Ok(MemoryTarget {
                agent_id: agent_id.to_string(),
                path: path.to_string_lossy().to_string(),
                exists,
                has_block,
                outside_chars,
            })
        })
        .collect()
}

/// 该 agent 指令文件全文(收编查看);不存在返回空串。
pub fn target_content(home: &Path, agent_id: &str) -> Result<String, String> {
    let path = agent_memory_path(home, agent_id)
        .ok_or_else(|| format!("{} memory sync is not supported", agent_id))?;
    Ok(read_target(&path)?.unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::test_util::*;

    fn set_library(home: &TempHome, content: &str) {
        write_file(
            home.path(),
            &PathBuf::from(".agents").join("memory").join("MEMORY.md"),
            content,
        );
    }

    fn claude_md(home: &TempHome) -> PathBuf {
        home.path().join(".claude").join("CLAUDE.md")
    }

    fn plan_of<'a>(plans: &'a [AgentPlan], id: &str) -> &'a AgentPlan {
        plans.iter().find(|p| p.agent_id == id).unwrap()
    }

    // ---- 区块纯函数 ----

    #[test]
    fn upsert_creates_appends_and_replaces_preserving_outside_bytes() {
        // 空文件(不存在)→ 只有区块
        let created = upsert_block("", "memo\n").unwrap();
        assert_eq!(created, "<!-- CLAWBOX_START -->\nmemo\n<!-- CLAWBOX_END -->\n");
        // 无区块 → 末尾追加,原内容一字不动 + 空行分隔
        let orig = "# My rules\n\ndo X\n";
        let appended = upsert_block(orig, "memo").unwrap(); // 库尾无 \n 自动规范
        assert_eq!(
            appended,
            "# My rules\n\ndo X\n\n<!-- CLAWBOX_START -->\nmemo\n<!-- CLAWBOX_END -->\n"
        );
        assert!(appended.starts_with(orig));
        // 有区块 → 只换内部;前后文字节不动
        let updated = upsert_block(&appended, "memo v2\n").unwrap();
        assert_eq!(
            updated,
            "# My rules\n\ndo X\n\n<!-- CLAWBOX_START -->\nmemo v2\n<!-- CLAWBOX_END -->\n"
        );
        // 区块夹在中间也只动区块
        let middle = format!("head\n\n{}\ntail text\n", render_block("old\n"));
        let updated = upsert_block(&middle, "new\n").unwrap();
        assert_eq!(updated, format!("head\n\n{}\ntail text\n", render_block("new\n")));
        // 幂等:同内容再 upsert 输出不变
        assert_eq!(upsert_block(&updated, "new\n").unwrap(), updated);
    }

    #[test]
    fn remove_block_restores_pre_injection_text() {
        let orig = "# My rules\ndo X\n";
        let injected = upsert_block(orig, "memo\n").unwrap();
        let removed = remove_block(&injected).unwrap().unwrap();
        assert_eq!(removed, orig, "remove must restore pre-injection text");
        // 整文件只有区块 → 空串
        let only = upsert_block("", "memo\n").unwrap();
        assert_eq!(remove_block(&only).unwrap().unwrap(), "");
        // 无区块 → None
        assert!(remove_block("plain\n").unwrap().is_none());
    }

    #[test]
    fn broken_or_duplicate_markers_error() {
        for text in [
            "<!-- CLAWBOX_START -->\nno end\n",                                  // 残缺
            "<!-- CLAWBOX_END -->\n<!-- CLAWBOX_START -->\n",                    // 顺序颠倒
            &format!("{}\n{}", render_block("a\n"), render_block("b\n")),        // 重复
        ] {
            assert!(scan_block(text).is_err(), "{:?}", text);
            assert!(upsert_block(text, "x\n").is_err());
            assert!(remove_block(text).is_err());
        }
    }

    // ---- plan / apply ----

    #[test]
    fn plan_apply_add_update_unchanged_lifecycle() {
        let home = TempHome::new();
        set_library(&home, "shared memo\n");
        let managed = HashMap::new();
        let plans = plan_all(home.path(), &managed);
        assert_eq!(plans.len(), 9);
        let cc = plan_of(&plans, "claude-code");
        assert!(cc.supported);
        assert!(cc.config_path.ends_with("CLAUDE.md"));
        assert_eq!((cc.changes[0].name.as_str(), cc.changes[0].action.as_str()), ("memory", "add"));
        assert!(!plan_of(&plans, "codebuddy").supported);
        // openclaw 支持,路径 = 默认 workspace 下 MEMORY.md
        let oc = plan_of(&plans, "openclaw");
        assert!(oc.supported && oc.config_path.ends_with("workspace/MEMORY.md"), "{}", oc.config_path);

        // apply:文件不存在 → 创建
        assert_eq!(apply_agent(home.path(), "claude-code", &[]).unwrap(), 1);
        let text = std::fs::read_to_string(claude_md(&home)).unwrap();
        assert!(text.contains("shared memo"));
        assert_eq!(deployed_names(home.path(), "claude-code"), vec!["block"]);
        // 幂等
        let plans = plan_all(home.path(), &managed);
        assert_eq!(plan_of(&plans, "claude-code").changes[0].action, "unchanged");
        assert_eq!(apply_agent(home.path(), "claude-code", &[]).unwrap(), 0);

        // 库变 → update,区块外内容不动
        let mut user_file = std::fs::read_to_string(claude_md(&home)).unwrap();
        user_file = format!("# user header\n\n{}", user_file);
        std::fs::write(claude_md(&home), &user_file).unwrap();
        set_library(&home, "shared memo v2\n");
        let plans = plan_all(home.path(), &managed);
        assert_eq!(plan_of(&plans, "claude-code").changes[0].action, "update");
        assert_eq!(apply_agent(home.path(), "claude-code", &[]).unwrap(), 1);
        let text = std::fs::read_to_string(claude_md(&home)).unwrap();
        assert!(text.starts_with("# user header\n\n"));
        assert!(text.contains("shared memo v2"));
        assert!(!text.contains("shared memo\n<!--"), "old content must be replaced");
    }

    #[test]
    fn append_to_existing_file_preserves_user_content() {
        let home = TempHome::new();
        set_library(&home, "memo\n");
        write_file(
            home.path(),
            &PathBuf::from(".codex").join("AGENTS.md"),
            "# codex rules\nalways test\n",
        );
        let plans = plan_all(home.path(), &HashMap::new());
        assert_eq!(plan_of(&plans, "codex").changes[0].action, "add");
        apply_agent(home.path(), "codex", &[]).unwrap();
        let text = std::fs::read_to_string(home.path().join(".codex").join("AGENTS.md")).unwrap();
        assert_eq!(
            text,
            "# codex rules\nalways test\n\n<!-- CLAWBOX_START -->\nmemo\n<!-- CLAWBOX_END -->\n"
        );
    }

    #[test]
    fn empty_library_remove_or_skip_semantics() {
        let home = TempHome::new();
        // 先正常下发
        set_library(&home, "memo\n");
        apply_agent(home.path(), "hermes", &[]).unwrap();
        let path = home.path().join(".hermes").join("memories").join("MEMORY.md");
        assert!(std::fs::read_to_string(&path).unwrap().contains("CLAWBOX_START"));

        // 库清空 + managed 有标记 → remove 整块
        set_library(&home, "");
        let managed_map: HashMap<String, Vec<String>> =
            [("hermes".to_string(), vec!["block".to_string()])].into();
        let plans = plan_all(home.path(), &managed_map);
        assert_eq!(plan_of(&plans, "hermes").changes[0].action, "remove");
        assert_eq!(apply_agent(home.path(), "hermes", &["block".to_string()]).unwrap(), 1);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains("CLAWBOX"), "{}", text);

        // 库空、无 managed → skip,apply 不动
        let plans = plan_all(home.path(), &HashMap::new());
        let h = plan_of(&plans, "hermes");
        assert_eq!(h.changes[0].action, "skip");
        assert!(h.changes[0].detail.contains("Memory library is empty"));
        assert_eq!(apply_agent(home.path(), "hermes", &[]).unwrap(), 0);
        // 未曾管理时即使文件里有区块也不删(managed 空)
        std::fs::write(&path, upsert_block("", "someone else\n").unwrap()).unwrap();
        assert_eq!(apply_agent(home.path(), "hermes", &[]).unwrap(), 0);
        assert!(std::fs::read_to_string(&path).unwrap().contains("CLAWBOX_START"));
    }

    #[test]
    fn broken_markers_plan_error_and_apply_refuses() {
        let home = TempHome::new();
        set_library(&home, "memo\n");
        let broken = "user text\n<!-- CLAWBOX_START -->\nno end marker\n";
        write_file(home.path(), &PathBuf::from(".claude").join("CLAUDE.md"), broken);
        let plans = plan_all(home.path(), &HashMap::new());
        let cc = plan_of(&plans, "claude-code");
        assert!(cc.error.as_deref().unwrap_or_default().contains("broken or duplicated"), "{:?}", cc.error);
        assert!(cc.changes.is_empty());
        // apply 拒动,文件逐字节原样
        assert!(apply_agent(home.path(), "claude-code", &[]).is_err());
        assert_eq!(std::fs::read_to_string(claude_md(&home)).unwrap(), broken);
        // 其它 agent 不受影响
        assert!(plan_of(&plans, "codex").error.is_none());
    }

    #[test]
    fn write_library_backs_up_existing_and_read_roundtrips() {
        let home = TempHome::new();
        assert_eq!(read_library(home.path()).unwrap(), ""); // 不存在 = 空
        assert!(write_library(home.path(), "v1\n").unwrap().is_none()); // 首写无备份
        assert_eq!(read_library(home.path()).unwrap(), "v1\n");
        let backup = write_library(home.path(), "v2\n").unwrap().expect("backed up");
        assert!(backup.contains("memory-lib"), "{}", backup);
        assert_eq!(std::fs::read_to_string(&backup).unwrap(), "v1\n");
        assert_eq!(read_library(home.path()).unwrap(), "v2\n");
    }

    #[test]
    fn targets_and_target_content_report_block_state() {
        let home = TempHome::new();
        set_library(&home, "memo\n");
        // claude-code:用户内容 + 区块;codex:不存在;opencode:残缺标记
        write_file(
            home.path(),
            &PathBuf::from(".claude").join("CLAUDE.md"),
            &upsert_block("12345\n", "memo\n").unwrap(),
        );
        write_file(
            home.path(),
            &PathBuf::from(".config").join("opencode").join("AGENTS.md"),
            "<!-- CLAWBOX_START -->\nbroken\n",
        );
        let ts = targets(home.path()).unwrap();
        assert_eq!(ts.len(), 5);
        let by_id = |id: &str| ts.iter().find(|t| t.agent_id == id).unwrap();
        let cc = by_id("claude-code");
        assert!(cc.exists && cc.has_block);
        assert_eq!(cc.outside_chars, "12345\n\n\n".chars().count()); // 用户文本 + 分隔与块尾换行
        assert!(cc.path.ends_with("CLAUDE.md"));
        let codex = by_id("codex");
        assert!(!codex.exists && !codex.has_block && codex.outside_chars == 0);
        let oc = by_id("opencode");
        assert!(oc.exists && !oc.has_block); // 残缺 → 按无有效区块报告
        assert!(oc.outside_chars > 0);

        // target_content:全文原样;不存在 → 空串;不支持 → Err
        assert!(target_content(home.path(), "claude-code").unwrap().starts_with("12345\n"));
        assert_eq!(target_content(home.path(), "codex").unwrap(), "");
        assert!(target_content(home.path(), "codebuddy").is_err());
    }

    #[test]
    fn apply_one_backs_up_existing_target_file() {
        let home = TempHome::new();
        set_library(&home, "memo\n");
        write_file(home.path(), &PathBuf::from(".claude").join("CLAUDE.md"), "orig\n");
        let r = apply_one(home.path(), "claude-code", &[]);
        assert!(r.ok, "{:?}", r.error);
        assert_eq!(r.applied, 1);
        let id = r.snapshot_id.expect("existing file snapshotted");
        let snaps = crate::sync::snapshots::list(home.path(), Some("claude-code"));
        assert_eq!(snaps.len(), 1);
        assert_eq!(snaps[0].id, id);
        assert_eq!(snaps[0].scope, "memory");
        let blob = home
            .path()
            .join(".clawbox")
            .join("snapshots")
            .join("claude-code")
            .join(&id)
            .join("blobs")
            .join("0");
        assert_eq!(std::fs::read_to_string(&blob).unwrap(), "orig\n");
        // 不支持的 agent
        let r = apply_one(home.path(), "kimi", &[]);
        assert!(!r.ok && r.error.unwrap().contains("not supported"));
    }
}
