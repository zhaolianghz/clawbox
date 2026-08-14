//! MCP config unified deployment ("MCP 配置统一下发").
//!
//! ClawBox holds the canonical MCP server registry
//! (`Config::mcp_servers`); per-agent adapters translate it into each
//! agent's native config file with a merge-write that never touches keys
//! ClawBox does not manage. `Config::mcp_managed` records what each agent
//! received on the last sync, so removals are scoped to names we wrote.
//!
//! Every path is resolved against an explicit `home: &Path` (production
//! passes `dirs::home_dir()`); tests run against a tempdir and never touch
//! the real user config files.

pub mod cli;
pub mod codex;
pub mod json_file;
pub mod providers;
pub mod memory;
pub mod skills;
pub mod snapshots;

use crate::commands::config::McpServerSpec;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Serialize, Clone, Debug)]
pub struct ChangeItem {
    pub name: String,
    /// "add" | "update" | "remove" | "unchanged" | "skip"
    pub action: String,
    pub detail: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct AgentPlan {
    pub agent_id: String,
    pub supported: bool,
    pub config_path: String,
    pub changes: Vec<ChangeItem>,
    pub error: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct ApplyResult {
    pub agent_id: String,
    pub ok: bool,
    pub backup_path: Option<String>,
    pub applied: usize,
    pub error: Option<String>,
}

pub trait ConfigAdapter: Send + Sync {
    fn agent_id(&self) -> &'static str;
    /// False for placeholder adapters (registered so the UI lists them, but
    /// plan/apply are no-ops).
    fn supported(&self) -> bool {
        true
    }
    /// Native config file this adapter writes. CLI-backed adapters return an
    /// empty path (they deploy through the agent's own CLI, not a file).
    fn config_path(&self, home: &Path) -> PathBuf;
    fn plan_mcp(
        &self,
        home: &Path,
        desired: &BTreeMap<String, McpServerSpec>,
        managed: &[String],
    ) -> Result<Vec<ChangeItem>, String>;
    fn apply_mcp(
        &self,
        home: &Path,
        desired: &BTreeMap<String, McpServerSpec>,
        managed: &[String],
    ) -> Result<usize, String>;
    /// Names this adapter would actually deploy (enabled minus adapter-level
    /// skips) — becomes the agent's new `mcp_managed` entry after apply.
    fn deployed_names(&self, desired: &BTreeMap<String, McpServerSpec>) -> Vec<String> {
        desired
            .iter()
            .filter(|(_, s)| s.enabled)
            .map(|(n, _)| n.clone())
            .collect()
    }
}

/// Placeholder for agents whose MCP config format we don't manage yet.
struct UnsupportedAdapter {
    id: &'static str,
}

impl ConfigAdapter for UnsupportedAdapter {
    fn agent_id(&self) -> &'static str {
        self.id
    }
    fn supported(&self) -> bool {
        false
    }
    fn config_path(&self, _home: &Path) -> PathBuf {
        PathBuf::new()
    }
    fn plan_mcp(
        &self,
        _home: &Path,
        _desired: &BTreeMap<String, McpServerSpec>,
        _managed: &[String],
    ) -> Result<Vec<ChangeItem>, String> {
        Ok(vec![])
    }
    fn apply_mcp(
        &self,
        _home: &Path,
        _desired: &BTreeMap<String, McpServerSpec>,
        _managed: &[String],
    ) -> Result<usize, String> {
        Err(format!("{} MCP sync is not supported yet", self.id))
    }
    fn deployed_names(&self, _desired: &BTreeMap<String, McpServerSpec>) -> Vec<String> {
        vec![]
    }
}

/// All registered adapters. ids must match the agent registry
/// (`crate::agents`); node is a runtime, not an agent, and is absent.
pub fn adapters() -> &'static [Box<dyn ConfigAdapter>] {
    static INSTANCES: std::sync::OnceLock<Vec<Box<dyn ConfigAdapter>>> = std::sync::OnceLock::new();
    INSTANCES
        .get_or_init(|| {
            vec![
                Box::new(json_file::claude_code()),
                Box::new(codex::CodexAdapter),
                Box::new(cli::CliMcpAdapter { id: "openclaw" }),
                Box::new(json_file::opencode()),
                Box::new(json_file::codebuddy()),
                Box::new(json_file::cursor_agent()),
                Box::new(UnsupportedAdapter { id: "kimi" }),
                Box::new(UnsupportedAdapter { id: "qodercli" }),
                Box::new(cli::CliMcpAdapter { id: "hermes" }),
                Box::new(json_file::gemini()),
                Box::new(json_file::cline()),
                Box::new(json_file::qwen_code()),
                Box::new(json_file::copilot_cli()),
                // pi 设计上不支持 MCP(官方:build CLI tools with READMEs)。
                Box::new(UnsupportedAdapter { id: "pi" }),
                Box::new(UnsupportedAdapter { id: "trae-agent" }),
            ]
        })
        .as_slice()
}

pub fn find_adapter(id: &str) -> Option<&'static dyn ConfigAdapter> {
    adapters().iter().find(|a| a.agent_id() == id).map(|a| a.as_ref())
}

/// Build the plan for every registered adapter. Parse/CLI failures land in
/// `AgentPlan::error` — one broken agent never aborts the whole plan.
pub fn plan_all(
    home: &Path,
    desired: &BTreeMap<String, McpServerSpec>,
    managed: &std::collections::HashMap<String, Vec<String>>,
) -> Vec<AgentPlan> {
    adapters()
        .iter()
        .map(|a| {
            let agent_id = a.agent_id().to_string();
            let config_path = a.config_path(home).to_string_lossy().to_string();
            if !a.supported() {
                return AgentPlan {
                    agent_id,
                    supported: false,
                    config_path,
                    changes: vec![],
                    error: None,
                };
            }
            let empty = vec![];
            let m = managed.get(a.agent_id()).unwrap_or(&empty);
            match a.plan_mcp(home, desired, m) {
                Ok(changes) => AgentPlan {
                    agent_id,
                    supported: true,
                    config_path,
                    changes,
                    error: None,
                },
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

/// Copy the target file (if any) to
/// `<home>/.clawbox/backups/<UTC yyyyMMdd-HHmmss>/<agent_id>__<filename>`.
/// Returns None when there is nothing to back up.
pub fn backup_target(home: &Path, agent_id: &str, target: &Path) -> Result<Option<String>, String> {
    if target.as_os_str().is_empty() || !target.is_file() {
        return Ok(None);
    }
    let now = time::OffsetDateTime::now_utc();
    let stamp = format!(
        "{:04}{:02}{:02}-{:02}{:02}{:02}",
        now.year(),
        u8::from(now.month()),
        now.day(),
        now.hour(),
        now.minute(),
        now.second()
    );
    let dir = home.join(".clawbox").join("backups").join(stamp);
    std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create backup dir: {}", e))?;
    let file_name = target
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "config".to_string());
    let dest = dir.join(format!("{}__{}", agent_id, file_name));
    std::fs::copy(target, &dest).map_err(|e| format!("Failed to back up {}: {}", target.display(), e))?;
    Ok(Some(dest.to_string_lossy().to_string()))
}

/// Apply the desired config to one agent: backup, write, report. The caller
/// (the `sync_mcp_apply` command) updates `mcp_managed` on success.
pub fn apply_one(
    home: &Path,
    adapter: &dyn ConfigAdapter,
    desired: &BTreeMap<String, McpServerSpec>,
    managed: &[String],
) -> ApplyResult {
    let agent_id = adapter.agent_id().to_string();
    if !adapter.supported() {
        return ApplyResult {
            agent_id,
            ok: false,
            backup_path: None,
            applied: 0,
            error: Some("agent not supported for MCP sync".to_string()),
        };
    }
    let backup_path = match backup_target(home, adapter.agent_id(), &adapter.config_path(home)) {
        Ok(p) => p,
        Err(e) => {
            return ApplyResult {
                agent_id,
                ok: false,
                backup_path: None,
                applied: 0,
                error: Some(e),
            }
        }
    };
    match adapter.apply_mcp(home, desired, managed) {
        Ok(applied) => ApplyResult {
            agent_id,
            ok: true,
            backup_path,
            applied,
            error: None,
        },
        Err(e) => ApplyResult {
            agent_id,
            ok: false,
            backup_path,
            applied: 0,
            error: Some(e),
        },
    }
}

// ---- shared plan helpers -------------------------------------------------

/// One-line human summary of a spec, used in ChangeItem.detail.
pub(crate) fn spec_summary(spec: &McpServerSpec) -> String {
    match spec.kind.as_str() {
        "stdio" => format!(
            "stdio: {} {}",
            spec.command.as_deref().unwrap_or("?"),
            spec.args.join(" ")
        )
        .trim_end()
        .to_string(),
        "http" => format!("http: {}", spec.url.as_deref().unwrap_or("?")),
        other => format!("unknown kind: {}", other),
    }
}

/// Diff a mapped desired set against the entries currently present in the
/// target, producing plan items shared by every file adapter. `mapped` holds
/// enabled servers only: Ok(rendered native value) or Err(skip reason).
/// `present` returns the target's current native value for a name (None when
/// absent). Values compare as serde_json for semantic (order-insensitive)
/// equality.
pub(crate) fn diff_changes(
    mapped: &BTreeMap<String, Result<serde_json::Value, String>>,
    managed: &[String],
    present: impl Fn(&str) -> Option<serde_json::Value>,
) -> Vec<ChangeItem> {
    let mut changes = Vec::new();
    for (name, rendered) in mapped {
        match rendered {
            Err(reason) => changes.push(ChangeItem {
                name: name.clone(),
                action: "skip".into(),
                detail: reason.clone(),
            }),
            Ok(value) => match present(name) {
                None => changes.push(ChangeItem {
                    name: name.clone(),
                    action: "add".into(),
                    detail: String::new(),
                }),
                Some(existing) if &existing == value => changes.push(ChangeItem {
                    name: name.clone(),
                    action: "unchanged".into(),
                    detail: String::new(),
                }),
                Some(_) => changes.push(ChangeItem {
                    name: name.clone(),
                    action: "update".into(),
                    detail: "content differs".into(),
                }),
            },
        }
    }
    // Removals: previously deployed, no longer desired (or disabled/skipped),
    // and still present in the target file.
    for name in managed {
        let still_deployed = matches!(mapped.get(name), Some(Ok(_)));
        if !still_deployed && present(name).is_some() {
            changes.push(ChangeItem {
                name: name.clone(),
                action: "remove".into(),
                detail: "no longer managed by ClawBox".into(),
            });
        }
    }
    changes
}

#[cfg(test)]
pub(crate) mod test_util {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    /// Self-created, self-cleaned tempdir acting as an isolated $HOME. Tests
    /// must never touch the real user config files.
    pub struct TempHome(pub PathBuf);

    impl TempHome {
        pub fn new() -> Self {
            let n = COUNTER.fetch_add(1, Ordering::SeqCst);
            let dir = std::env::temp_dir().join(format!(
                "clawbox-sync-test-{}-{}",
                std::process::id(),
                n
            ));
            std::fs::create_dir_all(&dir).unwrap();
            TempHome(dir)
        }
        pub fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempHome {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    pub fn stdio_spec(cmd: &str, args: &[&str]) -> McpServerSpec {
        McpServerSpec {
            kind: "stdio".into(),
            command: Some(cmd.into()),
            args: args.iter().map(|s| s.to_string()).collect(),
            env: Default::default(),
            url: None,
            headers: Default::default(),
            enabled: true,
        }
    }

    pub fn http_spec(url: &str) -> McpServerSpec {
        McpServerSpec {
            kind: "http".into(),
            command: None,
            args: vec![],
            env: Default::default(),
            url: Some(url.into()),
            headers: Default::default(),
            enabled: true,
        }
    }

    pub fn write_file(home: &Path, rel: &Path, content: &str) {
        let p = home.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
    }

    pub fn action_of<'a>(changes: &'a [ChangeItem], name: &str) -> &'a str {
        &changes
            .iter()
            .find(|c| c.name == name)
            .unwrap_or_else(|| panic!("no change item for {}", name))
            .action
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapter_registry_matches_agent_registry_ids() {
        // Every adapter id must exist in the agent registry; node (a runtime)
        // must not be registered.
        for a in adapters() {
            assert!(
                crate::agents::find_agent(a.agent_id()).is_some(),
                "adapter {} not in agent registry",
                a.agent_id()
            );
        }
        assert!(find_adapter("node").is_none());
        // 15 = 16 registry entries minus node.
        assert_eq!(adapters().len(), 15);
    }

    #[test]
    fn kimi_and_qodercli_are_unsupported_placeholders() {
        for id in ["kimi", "qodercli"] {
            let a = find_adapter(id).unwrap();
            assert!(!a.supported());
            let plans = plan_all(
                &std::path::PathBuf::from("/nonexistent"),
                &BTreeMap::new(),
                &std::collections::HashMap::new(),
            );
            let p = plans.iter().find(|p| p.agent_id == id).unwrap();
            assert!(!p.supported);
            assert!(p.changes.is_empty());
            assert!(p.error.is_none());
        }
    }

    #[test]
    fn backup_copies_into_timestamped_dir() {
        let home = test_util::TempHome::new();
        let target = home.path().join(".claude.json");
        std::fs::write(&target, "{\"x\":1}").unwrap();
        let backup = backup_target(home.path(), "claude-code", &target)
            .unwrap()
            .expect("backup created");
        let backup_path = PathBuf::from(&backup);
        assert!(backup_path.starts_with(home.path().join(".clawbox").join("backups")));
        assert!(backup.ends_with("claude-code__.claude.json"));
        assert_eq!(std::fs::read_to_string(&backup_path).unwrap(), "{\"x\":1}");
    }

    #[test]
    fn backup_of_missing_file_is_none() {
        let home = test_util::TempHome::new();
        let target = home.path().join("nope.json");
        assert!(backup_target(home.path(), "x", &target).unwrap().is_none());
    }
}
