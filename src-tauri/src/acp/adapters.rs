//! Adapter registry — ACP-compatible agent bridges installable as CLIs.

use serde::Serialize;
use std::process::Command;

pub struct AcpAdapter {
    pub id: &'static str,
    pub label: &'static str,
    pub binary: &'static str,
    pub install_hint: &'static str,
    pub check_probe: &'static [&'static str],
}

impl AcpAdapter {
    pub fn is_installed(&self) -> bool {
        self.version().is_some()
    }

    pub fn version(&self) -> Option<String> {
        let out = Command::new(self.binary).args(self.check_probe).output().ok()?;
        if !out.status.success() {
            return None;
        }
        let s = String::from_utf8_lossy(&out.stdout).trim().lines().next()?.to_string();
        if s.is_empty() { None } else { Some(s) }
    }
}

static ADAPTERS: &[AcpAdapter] = &[
    AcpAdapter {
        id: "claude-agent-acp",
        label: "ClaudeCode",
        binary: "claude-agent-acp",
        install_hint: "npm install -g @agentclientprotocol/claude-agent-acp",
        check_probe: &["--version"],
    },
    AcpAdapter {
        id: "codex-acp",
        label: "Codex",
        binary: "codex-acp",
        install_hint: "npm install -g --force @agentclientprotocol/codex-acp",
        check_probe: &["--version"],
    },
];

pub fn adapters() -> &'static [AcpAdapter] {
    ADAPTERS
}

pub fn find_adapter(id: &str) -> Option<&'static AcpAdapter> {
    ADAPTERS.iter().find(|a| a.id == id)
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
    ADAPTERS
        .iter()
        .map(|a| {
            let version = a.version();
            AdapterInfo {
                id: a.id.to_string(),
                label: a.label.to_string(),
                installed: version.is_some(),
                version,
                install_hint: a.install_hint.to_string(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_claude_and_codex() {
        let ids: Vec<_> = adapters().iter().map(|a| a.id).collect();
        assert!(ids.contains(&"claude-agent-acp"));
        assert!(ids.contains(&"codex-acp"));
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
}
