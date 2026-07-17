//! CLI-backed adapters: openclaw and hermes.
//!
//! These gateways own their MCP config; ClawBox never writes their files
//! directly. Instead the adapter reuses the existing `McpCapability`
//! implementations (`crate::backends`) — plan diffs names against
//! `mcp_list`, apply loops `mcp_add` / `mcp_remove`. When the CLI is not
//! installed, plan returns an error string instead of panicking.

use super::{spec_summary, ChangeItem, ConfigAdapter};
use crate::commands::config::McpServerSpec;
use serde_json::json;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub struct CliMcpAdapter {
    pub id: &'static str,
}

impl CliMcpAdapter {
    fn entry(&self) -> Result<&'static crate::backends::BackendEntry, String> {
        let entry = crate::backends::find_entry(self.id)
            .ok_or_else(|| format!("{}: backend not registered", self.id))?;
        if !entry.backend.is_installed() {
            return Err(format!("{} is not installed", self.id));
        }
        Ok(entry)
    }

    fn mcp(&self) -> Result<&'static dyn crate::backends::capabilities::McpCapability, String> {
        self.entry()?
            .mcp
            .ok_or_else(|| format!("{} has no MCP capability", self.id))
    }

    /// Server names currently registered in the gateway.
    fn current_names(&self) -> Result<Vec<String>, String> {
        Ok(self.mcp()?.mcp_list()?.into_iter().map(|s| s.name).collect())
    }

    /// Native config JSON passed to `mcp add`/`mcp set`.
    fn config_json(spec: &McpServerSpec) -> Result<String, String> {
        match spec.kind.as_str() {
            "stdio" => {
                let cmd = spec
                    .command
                    .as_deref()
                    .filter(|c| !c.is_empty())
                    .ok_or_else(|| "stdio server has no command".to_string())?;
                Ok(json!({"command": cmd, "args": spec.args, "env": spec.env}).to_string())
            }
            "http" => {
                let url = spec
                    .url
                    .as_deref()
                    .filter(|u| !u.is_empty())
                    .ok_or_else(|| "http server has no url".to_string())?;
                Ok(json!({"type": "http", "url": url, "headers": spec.headers}).to_string())
            }
            other => Err(format!("unsupported server kind: {}", other)),
        }
    }
}

impl ConfigAdapter for CliMcpAdapter {
    fn agent_id(&self) -> &'static str {
        self.id
    }

    /// No file managed directly — deployment goes through the CLI.
    fn config_path(&self, _home: &Path) -> PathBuf {
        PathBuf::new()
    }

    fn plan_mcp(
        &self,
        _home: &Path,
        desired: &BTreeMap<String, McpServerSpec>,
        managed: &[String],
    ) -> Result<Vec<ChangeItem>, String> {
        let current = self.current_names()?;
        let mut changes = Vec::new();
        for (name, spec) in desired.iter().filter(|(_, s)| s.enabled) {
            if let Err(reason) = Self::config_json(spec) {
                changes.push(ChangeItem {
                    name: name.clone(),
                    action: "skip".into(),
                    detail: reason,
                });
                continue;
            }
            // CLI list only exposes names, not full configs: existing names
            // plan as update (re-add is an idempotent upsert), not unchanged.
            let action = if current.contains(name) { "update" } else { "add" };
            changes.push(ChangeItem {
                name: name.clone(),
                action: action.into(),
                detail: spec_summary(spec),
            });
        }
        for name in managed {
            let still_deployed = desired.get(name).map(|s| s.enabled).unwrap_or(false);
            if !still_deployed && current.contains(name) {
                changes.push(ChangeItem {
                    name: name.clone(),
                    action: "remove".into(),
                    detail: "no longer managed by ClawBox".into(),
                });
            }
        }
        Ok(changes)
    }

    fn apply_mcp(
        &self,
        _home: &Path,
        desired: &BTreeMap<String, McpServerSpec>,
        managed: &[String],
    ) -> Result<usize, String> {
        let mcp = self.mcp()?;
        let current = self.current_names()?;
        let mut applied = 0;
        for (name, spec) in desired.iter().filter(|(_, s)| s.enabled) {
            let Ok(cfg) = Self::config_json(spec) else { continue };
            mcp.mcp_add(name, &cfg)
                .map_err(|e| format!("{}: mcp add {} failed: {}", self.id, name, e))?;
            applied += 1;
        }
        for name in managed {
            let still_deployed = desired.get(name).map(|s| s.enabled).unwrap_or(false);
            if !still_deployed && current.contains(name) {
                mcp.mcp_remove(name)
                    .map_err(|e| format!("{}: mcp remove {} failed: {}", self.id, name, e))?;
                applied += 1;
            }
        }
        Ok(applied)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::test_util::*;

    #[test]
    fn config_json_shapes() {
        let mut spec = stdio_spec("codegraph", &["serve"]);
        spec.env.insert("K".into(), "V".into());
        let cfg: serde_json::Value =
            serde_json::from_str(&CliMcpAdapter::config_json(&spec).unwrap()).unwrap();
        assert_eq!(cfg, json!({"command": "codegraph", "args": ["serve"], "env": {"K": "V"}}));

        let cfg: serde_json::Value =
            serde_json::from_str(&CliMcpAdapter::config_json(&http_spec("https://x/mcp")).unwrap())
                .unwrap();
        assert_eq!(cfg, json!({"type": "http", "url": "https://x/mcp", "headers": {}}));
    }

    // Not-installed graceful failure. We can't force-uninstall the real CLIs,
    // so when a CLI happens to be installed on this host the case is skipped
    // (matching the smoke-test convention) — unit tests stay hermetic and
    // never invoke a real gateway.
    #[test]
    fn plan_errors_gracefully_when_cli_missing() {
        for id in ["openclaw", "hermes"] {
            let adapter = CliMcpAdapter { id };
            let entry = crate::backends::find_entry(id).unwrap();
            if entry.backend.is_installed() {
                eprintln!("{} installed — skipping not-installed case", id);
                continue;
            }
            let home = TempHome::new();
            let err = adapter
                .plan_mcp(home.path(), &BTreeMap::new(), &[])
                .unwrap_err();
            assert!(err.contains("not installed"), "{}: {}", id, err);
        }
    }

    #[test]
    fn apply_errors_gracefully_when_cli_missing() {
        for id in ["openclaw", "hermes"] {
            let adapter = CliMcpAdapter { id };
            let entry = crate::backends::find_entry(id).unwrap();
            if entry.backend.is_installed() {
                continue; // never mutate a real gateway from unit tests
            }
            let home = TempHome::new();
            let err = adapter
                .apply_mcp(home.path(), &BTreeMap::new(), &[])
                .unwrap_err();
            assert!(err.contains("not installed"), "{}: {}", id, err);
        }
    }
}
