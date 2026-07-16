//! Adapter registry — ACP bridges, now sourced from the unified agent
//! registry (crate::agents) so bridges are registered exactly once.

use crate::agents::{self, install_command_display, AgentKind};
use serde::Serialize;
use std::sync::OnceLock;

pub struct AcpAdapter {
    pub id: String,
    pub label: String,
    pub binary: String,
    pub install_hint: String,
    pub check_probe: Vec<String>,
}

impl AcpAdapter {
    fn def(&self) -> &'static agents::AgentDef {
        // Invariant: every AcpAdapter is built from a registry entry.
        agents::find_agent(&self.id).expect("adapter id present in agent registry")
    }

    pub fn is_installed(&self) -> bool {
        self.def().is_installed()
    }

    pub fn version(&self) -> Option<String> {
        self.def().version()
    }
}

static ADAPTERS: OnceLock<Vec<AcpAdapter>> = OnceLock::new();

pub fn adapters() -> &'static [AcpAdapter] {
    ADAPTERS.get_or_init(|| {
        agents::agents()
            .iter()
            .filter(|a| a.kind == AgentKind::AcpBridge)
            .map(|a| AcpAdapter {
                id: a.id.to_string(),
                label: a.label.to_string(),
                binary: a.binary.to_string(),
                install_hint: install_command_display(a)
                    .expect("bridges are npm-installable"),
                check_probe: a.check_probe.iter().map(|s| s.to_string()).collect(),
            })
            .collect()
    })
}

pub fn find_adapter(id: &str) -> Option<&'static AcpAdapter> {
    adapters().iter().find(|a| a.id == id)
}

#[derive(Serialize)]
pub struct AdapterInfo {
    pub id: String,
    pub label: String,
    pub installed: bool,
    pub version: Option<String>,
    pub install_hint: String,
}

pub fn list_adapter_info() -> Vec<AdapterInfo> {
    adapters()
        .iter()
        .map(|a| {
            let version = a.version();
            AdapterInfo {
                id: a.id.clone(),
                label: a.label.clone(),
                installed: version.is_some(),
                version,
                install_hint: a.install_hint.clone(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_claude_and_codex() {
        let ids: Vec<_> = adapters().iter().map(|a| a.id.as_str()).collect();
        assert!(ids.contains(&"claude-agent-acp"));
        assert!(ids.contains(&"codex-acp"));
        assert_eq!(ids.len(), 2, "exactly the bridge entries from the registry");
    }

    #[test]
    fn find_adapter_works() {
        assert!(find_adapter("claude-agent-acp").is_some());
        assert!(find_adapter("nonexistent").is_none());
    }

    #[test]
    fn list_info_covers_all_adapters() {
        assert_eq!(list_adapter_info().len(), adapters().len());
    }

    #[test]
    fn install_hint_comes_from_registry() {
        assert_eq!(
            find_adapter("codex-acp").unwrap().install_hint,
            "npm install -g --force @agentclientprotocol/codex-acp"
        );
    }
}
