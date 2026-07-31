//! Unified agent registry — the single authority for every agent CLI ClawBox
//! can detect/install. Spec: docs/superpowers/specs/2026-07-16-agent-management-center-design.md

pub mod install;

use serde::Serialize;
use std::process::{Command, Stdio};
use std::time::Duration;

#[derive(Serialize, Clone, Copy, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AgentKind {
    NativeCli,
    Runtime,
    Gateway,
}

pub enum InstallMethod {
    Npm { package: &'static str, force: bool },
    Script {
        /// Unix (macOS/Linux) installer URL, run via `curl -fsSL {url} | bash`.
        unix: &'static str,
        /// Windows installer URL, run via PowerShell `irm {url} | iex`.
        windows: &'static str,
    },
    /// Platform package manager (node: brew on macOS / winget on Windows).
    PlatformPkg,
    /// No auto-install; detection + docs link only.
    DetectOnly,
}

pub struct AgentDef {
    pub id: &'static str,
    pub label: &'static str,
    pub binary: &'static str,
    pub kind: AgentKind,
    pub install: InstallMethod,
    pub check_probe: &'static [&'static str],
    pub depends_on: &'static [&'static str],
    pub docs_url: Option<&'static str>,
}

static AGENTS: &[AgentDef] = &[
    AgentDef {
        id: "node", label: "Node.js", binary: "node",
        kind: AgentKind::Runtime, install: InstallMethod::PlatformPkg,
        check_probe: &["--version"], depends_on: &[],
        docs_url: Some("https://nodejs.org"),
    },
    AgentDef {
        id: "claude-code", label: "Claude Code", binary: "claude",
        kind: AgentKind::NativeCli,
        install: InstallMethod::Npm { package: "@anthropic-ai/claude-code", force: false },
        check_probe: &["--version"], depends_on: &[],
        docs_url: Some("https://docs.anthropic.com/en/docs/claude-code"),
    },
    AgentDef {
        id: "codex", label: "Codex", binary: "codex",
        kind: AgentKind::NativeCli,
        install: InstallMethod::Npm { package: "@openai/codex", force: false },
        check_probe: &["--version"], depends_on: &[],
        docs_url: Some("https://developers.openai.com/codex/cli"),
    },
    AgentDef {
        id: "openclaw", label: "OpenClaw", binary: "openclaw",
        kind: AgentKind::Gateway,
        install: InstallMethod::Npm { package: "openclaw", force: false },
        check_probe: &["--version"], depends_on: &[],
        docs_url: Some("https://www.npmjs.com/package/openclaw"),
    },
    AgentDef {
        id: "opencode", label: "OpenCode", binary: "opencode",
        kind: AgentKind::NativeCli,
        install: InstallMethod::Npm { package: "opencode-ai", force: false },
        check_probe: &["--version"], depends_on: &[],
        docs_url: Some("https://opencode.ai"),
    },
    AgentDef {
        id: "codebuddy", label: "CodeBuddy", binary: "codebuddy",
        kind: AgentKind::NativeCli,
        install: InstallMethod::Npm { package: "@tencent-ai/codebuddy-code", force: false },
        check_probe: &["--version"], depends_on: &[],
        docs_url: Some("https://www.codebuddy.ai"),
    },
    AgentDef {
        id: "cursor-agent", label: "Cursor", binary: "cursor-agent",
        kind: AgentKind::NativeCli,
        install: InstallMethod::Script { unix: "https://cursor.com/install", windows: "https://cursor.com/install" },
        check_probe: &["--version"], depends_on: &[],
        docs_url: Some("https://cursor.com/cli"),
    },
    AgentDef {
        id: "kimi", label: "Kimi", binary: "kimi",
        kind: AgentKind::NativeCli,
        install: InstallMethod::Script { unix: "https://code.kimi.com/kimi-code/install.sh", windows: "https://code.kimi.com/kimi-code/install.sh" },
        check_probe: &["--version"], depends_on: &[],
        docs_url: Some("https://code.kimi.com"),
    },
    AgentDef {
        id: "qodercli", label: "Qoder", binary: "qodercli",
        kind: AgentKind::NativeCli,
        install: InstallMethod::Script { unix: "https://qoder.com/install", windows: "https://qoder.com/install" },
        check_probe: &["--version"], depends_on: &[],
        docs_url: Some("https://qoder.com"),
    },
    AgentDef {
        id: "hermes", label: "Hermes", binary: "hermes",
        kind: AgentKind::Gateway,
        // Hermes (NousResearch/hermes-agent) 分发跨平台安装脚本:
        //   unix:    curl -fsSL https://hermes-agent.nousresearch.com/install.sh | bash
        //   windows: irm https://hermes-agent.nousresearch.com/install.ps1 | iex
        install: InstallMethod::Script {
            unix: "https://hermes-agent.nousresearch.com/install.sh",
            windows: "https://hermes-agent.nousresearch.com/install.ps1",
        },
        check_probe: &["--version"], depends_on: &[],
        docs_url: Some("https://github.com/NousResearch/hermes-agent"),
    },
    AgentDef {
        id: "gemini", label: "Gemini CLI", binary: "gemini",
        kind: AgentKind::NativeCli,
        install: InstallMethod::Npm { package: "@google/gemini-cli", force: false },
        check_probe: &["--version"], depends_on: &[],
        docs_url: Some("https://github.com/google-gemini/gemini-cli"),
    },
    AgentDef {
        id: "cline", label: "Cline", binary: "cline",
        kind: AgentKind::NativeCli,
        install: InstallMethod::Npm { package: "cline", force: false },
        check_probe: &["--version"], depends_on: &[],
        docs_url: Some("https://docs.cline.bot/cline-cli/installation"),
    },
    AgentDef {
        // badlogic/pi:npm 包已迁移为 @earendil-works/pi-coding-agent
        // (旧名 @mariozechner/pi-coding-agent)。设计上不支持 MCP。
        id: "pi", label: "Pi", binary: "pi",
        kind: AgentKind::NativeCli,
        install: InstallMethod::Npm { package: "@earendil-works/pi-coding-agent", force: false },
        check_probe: &["--version"], depends_on: &[],
        docs_url: Some("https://github.com/badlogic/pi-mono"),
    },
    AgentDef {
        id: "qwen-code", label: "Qwen Code", binary: "qwen",
        kind: AgentKind::NativeCli,
        install: InstallMethod::Npm { package: "@qwen-code/qwen-code", force: false },
        check_probe: &["--version"], depends_on: &[],
        docs_url: Some("https://qwenlm.github.io/qwen-code-docs/"),
    },
    AgentDef {
        id: "copilot-cli", label: "Copilot CLI", binary: "copilot",
        kind: AgentKind::NativeCli,
        install: InstallMethod::Npm { package: "@github/copilot", force: false },
        check_probe: &["--version"], depends_on: &[],
        docs_url: Some("https://docs.github.com/en/copilot/how-tos/copilot-cli"),
    },
    AgentDef {
        // bytedance/trae-agent:不发 npm/PyPI,仅源码安装(clone + uv sync),
        // 故 DetectOnly。注意 Trae IDE 无 CLI,这里接的是开源 trae-agent。
        id: "trae-agent", label: "Trae Agent", binary: "trae-cli",
        kind: AgentKind::NativeCli,
        install: InstallMethod::DetectOnly,
        check_probe: &["--version"], depends_on: &[],
        docs_url: Some("https://github.com/bytedance/trae-agent"),
    },
];

pub fn agents() -> &'static [AgentDef] {
    AGENTS
}

pub fn find_agent(id: &str) -> Option<&'static AgentDef> {
    AGENTS.iter().find(|a| a.id == id)
}

impl AgentDef {
    /// Probe the binary for its version string.
    /// Bounded by a 10s watchdog: a hung `--version` (interactive prompt,
    /// self-update check) must not stall list_agent_status' rayon collect
    /// and freeze the frontend spinner forever.
    pub fn version(&self) -> Option<String> {
        let child = Command::new(self.binary)
            .args(self.check_probe)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let out = crate::path_env::wait_with_timeout(child, Duration::from_secs(10))?;
        if !out.status.success() {
            return None;
        }
        let s = String::from_utf8_lossy(&out.stdout).trim().lines().next()?.to_string();
        if s.is_empty() { None } else { Some(s) }
    }

    pub fn is_installed(&self) -> bool {
        self.version().is_some()
    }
}

/// Explicit depends_on plus the implicit node dependency for Npm installs.
pub fn effective_deps(def: &AgentDef) -> Vec<&'static str> {
    let mut deps: Vec<&'static str> = def.depends_on.to_vec();
    if matches!(def.install, InstallMethod::Npm { .. }) && !deps.contains(&"node") {
        deps.push("node");
    }
    deps
}

/// Human-readable install command (shown in UI before the user confirms).
/// None for DetectOnly.
pub fn install_command_display(def: &AgentDef) -> Option<String> {
    match def.install {
        InstallMethod::Npm { package, force } => Some(if force {
            format!("npm install -g --force {}", package)
        } else {
            format!("npm install -g {}", package)
        }),
        InstallMethod::Script { unix, windows } => Some(if cfg!(windows) {
            format!("irm {} | iex", windows)
        } else {
            format!("curl -fsSL {} | bash", unix)
        }),
        InstallMethod::PlatformPkg => Some(match std::env::consts::OS {
            "windows" => "winget install --id OpenJS.NodeJS.LTS --silent".to_string(),
            _ => "brew install node".to_string(),
        }),
        InstallMethod::DetectOnly => None,
    }
}

#[derive(Serialize)]
pub struct AgentStatus {
    pub id: String,
    pub label: String,
    pub kind: AgentKind,
    pub installed: bool,
    pub version: Option<String>,
    pub deps_satisfied: bool,
    pub missing_deps: Vec<String>,
    pub install_command: Option<String>,
    pub docs_url: Option<String>,
}

/// Probe every agent in parallel (each probe shells out, ~0.1-1s serial cost).
pub fn list_agent_status() -> Vec<AgentStatus> {
    use rayon::prelude::*;
    // Probe installed-state for all 10 first so dep checks reuse results.
    let installed: std::collections::HashMap<&str, Option<String>> = AGENTS
        .par_iter()
        .map(|a| (a.id, a.version()))
        .collect();
    AGENTS
        .iter()
        .map(|a| {
            let version = installed.get(a.id).cloned().flatten();
            let missing: Vec<String> = effective_deps(a)
                .into_iter()
                .filter(|dep| installed.get(*dep).map(|v| v.is_none()).unwrap_or(true))
                .map(|s| s.to_string())
                .collect();
            AgentStatus {
                id: a.id.to_string(),
                label: a.label.to_string(),
                kind: a.kind,
                installed: version.is_some(),
                version,
                deps_satisfied: missing.is_empty(),
                missing_deps: missing,
                install_command: install_command_display(a),
                docs_url: a.docs_url.map(|s| s.to_string()),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_exactly_16_unique_entries() {
        let ids: Vec<_> = agents().iter().map(|a| a.id).collect();
        assert_eq!(ids.len(), 16);
        let mut dedup = ids.clone();
        dedup.sort();
        dedup.dedup();
        assert_eq!(dedup.len(), 16, "duplicate agent ids");
    }

    #[test]
    fn registry_covers_spec_table() {
        for id in [
            "node", "claude-code", "codex", "openclaw", "opencode", "codebuddy",
            "cursor-agent", "kimi", "qodercli", "hermes",
            "gemini", "cline", "pi", "qwen-code", "copilot-cli", "trae-agent",
        ] {
            assert!(find_agent(id).is_some(), "missing agent: {}", id);
        }
    }

    #[test]
    fn depends_on_references_are_valid_ids() {
        for a in agents() {
            for dep in a.depends_on {
                assert!(find_agent(dep).is_some(), "{} depends on unknown {}", a.id, dep);
            }
        }
    }

    #[test]
    fn hermes_uses_cross_platform_script_install() {
        // Hermes 由脚本安装 (unix install.sh / windows install.ps1),不再 DetectOnly。
        let h = find_agent("hermes").unwrap();
        assert!(matches!(h.install, InstallMethod::Script { .. }));
        assert!(install_command_display(h).is_some());
    }

    #[test]
    fn install_command_display_formats_each_method() {
        assert_eq!(
            install_command_display(find_agent("claude-code").unwrap()).as_deref(),
            Some("npm install -g @anthropic-ai/claude-code")
        );
        assert_eq!(
            install_command_display(find_agent("cursor-agent").unwrap()).as_deref(),
            Some("curl -fsSL https://cursor.com/install | bash")
        );
        // node = platform package manager
        let node_cmd = install_command_display(find_agent("node").unwrap()).unwrap();
        assert!(node_cmd.contains("brew") || node_cmd.contains("winget"));
    }

    #[test]
    fn npm_agents_implicitly_require_node_in_status() {
        // Registry-level check: effective_deps injects "node" for Npm installs.
        let deps = effective_deps(find_agent("claude-code").unwrap());
        assert!(deps.contains(&"node"));
        // ...but node itself must not depend on node.
        assert!(effective_deps(find_agent("node").unwrap()).is_empty());
    }
}
