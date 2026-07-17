use std::process::Command;

use super::{Backend, GatewayStatus};

pub struct HermesBackend;

impl Backend for HermesBackend {
    fn id(&self) -> &'static str { "hermes" }
    fn display_name(&self) -> &'static str { "Hermes" }
    fn version(&self) -> String {
        Command::new("hermes").arg("--version").output().ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).lines().next().unwrap_or("").trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "unknown".into())
    }

    fn is_installed(&self) -> bool {
        Command::new("hermes").arg("--version").output()
            .map(|o| o.status.success()).unwrap_or(false)
    }
    fn gateway_status(&self) -> Result<GatewayStatus, String> {
        let text = run_hermes(&["gateway", "status"]).unwrap_or_default();
        let pid = extract_pid(&text);
        // `running` is derived from a parsable PID. Some hermes plist dumps
        // contain a `PID` line that is not parseable (e.g. transitioning state)
        // — those must not be reported as running.
        let status = if pid.is_some() { "running" } else { "stopped" };
        Ok(GatewayStatus {
            status: status.into(),
            version: self.version(),
            pid,
        })
    }
}

impl super::capabilities::SkillsCapability for HermesBackend {
    fn skills_list(&self) -> Result<Vec<super::capabilities::Skill>, String> {
        let output = std::process::Command::new("hermes")
            .args(["skills", "list"])
            .output()
            .map_err(|e| format!("Failed to run hermes: {}", e))?;
        if !output.status.success() {
            return Err(format!("hermes skills list failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()));
        }
        Ok(parse_hermes_skills_text(&String::from_utf8_lossy(&output.stdout)))
    }
    fn skills_install(&self, id: &str) -> Result<String, String> {
        run_hermes(&["skills", "install", id])
    }
    fn skills_uninstall(&self, id: &str) -> Result<String, String> {
        run_hermes(&["skills", "uninstall", id])
    }
    fn skills_set_enabled(&self, _id: &str, _enabled: bool) -> Result<String, String> {
        Err("hermes does not support skills set_enabled".to_string())
    }
}

impl super::capabilities::McpCapability for HermesBackend {
    fn mcp_list(&self) -> Result<Vec<super::capabilities::McpServer>, String> {
        let output = std::process::Command::new("hermes")
            .args(["mcp", "list"])
            .output()
            .map_err(|e| format!("Failed to run hermes: {}", e))?;
        if !output.status.success() {
            return Err(format!("hermes mcp list failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()));
        }
        Ok(parse_hermes_mcp_text(&String::from_utf8_lossy(&output.stdout)))
    }
    fn mcp_add(&self, name: &str, config_json: &str) -> Result<String, String> {
        run_hermes(&["mcp", "add", name, "--config", config_json])
    }
    fn mcp_remove(&self, name: &str) -> Result<String, String> {
        run_hermes(&["mcp", "remove", name])
    }
}

impl super::capabilities::PluginsCapability for HermesBackend {
    fn plugins_list(&self) -> Result<Vec<super::capabilities::Plugin>, String> {
        let output = std::process::Command::new("hermes").args(["plugins", "list"]).output()
            .map_err(|e| format!("Failed to run hermes: {}", e))?;
        if !output.status.success() {
            return Err(format!("hermes plugins list failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()));
        }
        Ok(parse_hermes_plugins_text(&String::from_utf8_lossy(&output.stdout)))
    }
    fn plugins_install(&self, source: &str) -> Result<String, String> {
        run_hermes(&["plugins", "install", source])
    }
    fn plugins_remove(&self, id: &str) -> Result<String, String> {
        run_hermes(&["plugins", "remove", id])
    }
    fn plugins_set_enabled(&self, id: &str, enabled: bool) -> Result<String, String> {
        let action = if enabled { "enable" } else { "disable" };
        run_hermes(&["plugins", action, id])
    }
}

fn parse_hermes_plugins_text(text: &str) -> Vec<super::capabilities::Plugin> {
    if text.trim().is_empty() || text.contains("No plugins") { return vec![]; }
    let mut plugins = Vec::new();
    let mut current: Option<super::capabilities::Plugin> = None;
    let mut in_data = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with('│') {
            // Header row uses ┃ instead of │ — detect when we see the header to start data.
            if !in_data && trimmed.contains('┃') && trimmed.contains("Name") && trimmed.contains("Status") {
                in_data = true;
            }
            continue;
        }
        if !in_data { continue; }
        let cells: Vec<&str> = trimmed.split('│').map(str::trim).collect();
        // Format: ["", name, status, version, desc..., source, ""]
        if cells.len() < 4 { continue; }
        if cells[1].is_empty() || cells[1].contains('━') || cells[1].contains('╇')
            || cells[1].contains('╞') { continue; }
        // Detect new data row: status cell is "enabled" or "not enabled".
        let status = cells[2];
        let is_data_row = status.contains("enabled");
        if is_data_row {
            if let Some(p) = current.take() { plugins.push(p); }
            let name = cells[1];
            let version = cells[3];
            let enabled = status == "enabled";
            current = Some(super::capabilities::Plugin {
                id: name.to_string(),
                name: name.to_string(),
                version: version.to_string(),
                enabled,
                raw: serde_json::json!({}),
            });
        } else if let Some(ref mut p) = current {
            // Continuation row — usually just cells[4] (description) is non-empty.
            if !cells[1].is_empty() {
                p.name.push(' ');
                p.name.push_str(cells[1]);
            }
            if !cells[4].is_empty() {
                let existing = p.raw["description"].as_str().unwrap_or("").to_string();
                let new = if existing.is_empty() {
                    cells[4].to_string()
                } else {
                    format!("{} {}", existing, cells[4])
                };
                p.raw["description"] = serde_json::Value::String(new);
            }
        }
    }
    if let Some(p) = current.take() { plugins.push(p); }
    plugins
}

impl super::capabilities::HooksCapability for HermesBackend {
    fn hooks_list(&self) -> Result<Vec<super::capabilities::Hook>, String> {
        let output = std::process::Command::new("hermes").args(["hooks", "list"]).output()
            .map_err(|e| format!("Failed to run hermes: {}", e))?;
        if !output.status.success() {
            return Err(format!("hermes hooks list failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()));
        }
        Ok(parse_hermes_hooks_text(&String::from_utf8_lossy(&output.stdout)))
    }
    fn hooks_set_enabled(&self, _id: &str, _enabled: bool) -> Result<String, String> {
        // hermes hooks CLI surface is list/test/revoke/doctor only — no enable/disable subcommand.
        // Hooks are declared in ~/.hermes/config.yaml; toggle the `enabled` flag there and re-run
        // `hermes hooks list`. See `hermes hooks --help` and website/docs/user-guide/features/hooks.md.
        Err("hermes hooks are config-managed (edit ~/.hermes/config.yaml); no enable/disable CLI subcommand".to_string())
    }
}

/// Parse `hermes hooks list` output.
///
/// Best-effort defensive parser: the real hermes `hooks list` output format is
/// undocumented (the live hermes on this machine has no hooks configured, so we
/// have no real fixture to capture). We handle the empty case explicitly and
/// fall back to a line-based best-guess for non-empty output.
///
/// When hermes hooks are configured on a host, capture output via
/// `hermes hooks list > /tmp/hermes_hooks.txt` and improve the parser.
fn parse_hermes_hooks_text(text: &str) -> Vec<super::capabilities::Hook> {
    if text.trim().is_empty() || text.contains("No shell hooks configured") {
        return vec![];
    }
    let mut hooks: Vec<super::capabilities::Hook> = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() { continue; }
        // Skip separator rows (────────────) and obvious header rows.
        if trimmed.chars().all(|c| c == '─' || c == ' ') { continue; }
        if trimmed.starts_with("Hook Name") || trimmed.starts_with("Match") { continue; }
        // Skip trailing help/instructions lines if they slip through.
        if trimmed.starts_with("See `") || trimmed.starts_with("for the config") { continue; }
        // Best-effort: first whitespace-delimited token is the hook id/name.
        let id = match trimmed.split_whitespace().next() {
            Some(tok) if !tok.is_empty() => tok.to_string(),
            _ => continue,
        };
        let enabled = trimmed.contains("allowed") || trimmed.contains("✓");
        hooks.push(super::capabilities::Hook {
            id,
            name: trimmed.to_string(),
            event: "shell".to_string(),
            enabled,
            raw: serde_json::json!({"raw_line": line}),
        });
    }
    hooks
}

impl super::capabilities::ToolsCapability for HermesBackend {
    fn tools_list(&self) -> Result<Vec<super::capabilities::Tool>, String> {
        let output = std::process::Command::new("hermes").args(["tools", "list"]).output()
            .map_err(|e| format!("Failed to run hermes: {}", e))?;
        if !output.status.success() {
            return Err(format!("hermes tools list failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()));
        }
        Ok(parse_hermes_tools_text(&String::from_utf8_lossy(&output.stdout)))
    }
    fn tools_set_enabled(&self, id: &str, enabled: bool) -> Result<String, String> {
        let action = if enabled { "enable" } else { "disable" };
        run_hermes(&["tools", action, id])
    }
}

fn parse_hermes_tools_text(text: &str) -> Vec<super::capabilities::Tool> {
    let mut tools = Vec::new();
    let mut in_section = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with("Built-in toolsets") { in_section = true; continue; }
        if t.starts_with("MCP servers") { in_section = false; continue; }
        if !in_section { continue; }
        if !t.starts_with('✓') && !t.starts_with('✗') { continue; }
        let enabled = t.starts_with('✓');
        let parts: Vec<&str> = t.split_whitespace().collect();
        // Format: [marker, "enabled"/"disabled", <id>, <emoji>, ...]
        if parts.len() < 3 { continue; }
        let id = parts[2].to_string();
        tools.push(super::capabilities::Tool { id, enabled, raw: serde_json::json!({}) });
    }
    tools
}

impl super::capabilities::MemoryCapability for HermesBackend {
    fn memory_status(&self) -> Result<super::capabilities::MemoryStatus, String> {
        let output = std::process::Command::new("hermes").args(["memory", "status"]).output()
            .map_err(|e| format!("Failed to run hermes: {}", e))?;
        if !output.status.success() {
            return Err(format!("hermes memory status failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()));
        }
        Ok(parse_hermes_memory_text(&String::from_utf8_lossy(&output.stdout)))
    }
    fn memory_index(&self) -> Result<String, String> {
        Err("hermes does not support memory index".to_string())
    }
    fn memory_reset(&self) -> Result<String, String> {
        run_hermes(&["memory", "reset"])
    }
}

fn parse_hermes_memory_text(text: &str) -> super::capabilities::MemoryStatus {
    let mut provider = String::new();
    let mut builtin_active = false;
    for line in text.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("Provider:") {
            provider = rest.trim().to_string();
        } else if let Some(rest) = t.strip_prefix("Built-in:") {
            builtin_active = rest.trim().contains("active");
        }
    }
    super::capabilities::MemoryStatus { provider, builtin_active, raw: serde_json::json!({}) }
}

fn run_hermes(args: &[&str]) -> Result<String, String> {
    let output = Command::new("hermes").args(args).output()
        .map_err(|e| format!("Failed to run hermes: {}", e))?;
    if !output.status.success() {
        return Err(format!("hermes {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn extract_pid(text: &str) -> Option<i32> {
    for line in text.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("PID") {
            let val = rest.trim().trim_start_matches('=').trim().trim_end_matches(';');
            if let Ok(n) = val.parse::<i32>() { return Some(n); }
        }
    }
    None
}

fn parse_hermes_skills_text(text: &str) -> Vec<super::capabilities::Skill> {
    if text.trim().is_empty() || text.contains("No skills installed") {
        return vec![];
    }
    let mut skills = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with('│') { continue; }
        let cells: Vec<&str> = trimmed.split('│').map(str::trim).collect();
        // Format: ["", name, category, source, trust, status, ""]
        if cells.len() < 6 { continue; }
        let name = cells[1];
        if name.is_empty() { continue; }
        // Skip header / separator rows (contain box-drawing chars).
        if name.contains('━') || name.contains('╇') || name.contains('╞')
            || name.eq_ignore_ascii_case("name") {
            continue;
        }
        let status = cells[5];
        let enabled = status.contains("enabled") && !status.contains("disabled");
        skills.push(super::capabilities::Skill {
            id: name.to_string(),
            name: name.to_string(),
            version: String::new(),
            description: String::new(),
            enabled,
            raw: serde_json::json!({"raw_line": line}),
        });
    }
    skills
}

/// Parse `hermes mcp list` output. Real output is an ASCII table with columns
/// `Name | Transport | Tools | Status`. Captured fixture (verbatim from
/// Hermes v0.11.0):
///
/// ```text
///   MCP Servers:
///
///   Name             Transport                      Tools        Status
///   ──────────────── ────────────────────────────── ──────────── ─────────
///   mx_data          uvx mxAi/mcp-mxdata            all          ✓ enabled
///   codegraph        codegraph serve --mcp          all          ✓ enabled
///   paused_one       foo                            all          ✗ disabled
/// ```
fn parse_hermes_mcp_text(text: &str) -> Vec<super::capabilities::McpServer> {
    if text.trim().is_empty() {
        return vec![];
    }
    let mut servers = Vec::new();
    let mut in_data = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if !in_data {
            if trimmed.starts_with("Name ") {
                in_data = true;
            }
            continue;
        }
        // In data section now.
        if trimmed.is_empty() { continue; }
        // Skip the ─── separator line.
        if trimmed.starts_with('─') { continue; }
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() < 3 { continue; }
        let name = parts[0].to_string();
        let status_token = parts[parts.len() - 1];
        let transport = parts[1..parts.len() - 2].join(" ");
        let status = if status_token == "enabled" || status_token == "disabled" {
            status_token.to_string()
        } else {
            // Status line is `✓ enabled` / `✗ disabled` — joined tokens.
            let joined = parts[parts.len() - 2..].join(" ");
            if joined.contains("enabled") && !joined.contains("disabled") {
                "enabled".to_string()
            } else if joined.contains("disabled") {
                "disabled".to_string()
            } else {
                "unknown".to_string()
            }
        };
        servers.push(super::capabilities::McpServer {
            name,
            transport,
            status,
            raw: serde_json::json!({"raw_line": line}),
        });
    }
    servers
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_pid_parses_plist_line() {
        let text = "Label = \"ai.hermes.gateway\";\nPID = 26128;\n";
        assert_eq!(extract_pid(text), Some(26128));
    }

    #[test]
    fn extract_pid_returns_none_when_absent() {
        let text = "Label = \"ai.hermes.gateway\";\n";
        assert_eq!(extract_pid(text), None);
    }

    // Real fixture captured verbatim from `hermes skills list` (v0.11.0).
    const HERMES_SKILLS_FIXTURE: &str = "\
                                Installed Skills
┏━━━━━━━━━━━━━━━━━━━━━━━━━┳━━━━━━━━━━━━━━━━━━━━━━┳━━━━━━━━━┳━━━━━━━━━┳━━━━━━━━━┓
┃ Name                    ┃ Category             ┃ Source  ┃ Trust   ┃ Status  ┃
┡━━━━━━━━━━━━━━━━━━━━━━━━━╇━━━━━━━━━━━━━━━━━━━━━━╇━━━━━━━━━╇━━━━━━━━━╇━━━━━━━━━┩
│ dogfood                 │                      │ builtin │ builtin │ enabled │
│ webnovel-write          │                      │ local   │ local   │ enabled │
│ paused-skill            │                      │ local   │ local   │ disabled │
└─────────────────────────┴──────────────────────┴─────────┴─────────┴─────────┘
";

    #[test]
    fn parses_hermes_skills_table() {
        let skills = parse_hermes_skills_text(HERMES_SKILLS_FIXTURE);
        assert_eq!(skills.len(), 3);
        assert_eq!(skills[0].id, "dogfood");
        assert_eq!(skills[0].name, "dogfood");
        assert!(skills[0].enabled);
        assert_eq!(skills[2].id, "paused-skill");
        assert!(!skills[2].enabled);
    }

    #[test]
    fn parses_hermes_skills_empty() {
        let text = "No skills installed.\n";
        let skills = parse_hermes_skills_text(text);
        assert!(skills.is_empty());
    }

    // Real fixture captured verbatim from `hermes mcp list` (v0.11.0).
    const HERMES_MCP_FIXTURE: &str = "\
  MCP Servers:

  Name             Transport                      Tools        Status
  ──────────────── ────────────────────────────── ──────────── ─────────
  mx_data          uvx mxAi/mcp-mxdata            all          ✓ enabled
  codegraph        codegraph serve --mcp          all          ✓ enabled
  paused_one       foo                            all          ✗ disabled
";

    #[test]
    fn parses_hermes_mcp_table() {
        let servers = parse_hermes_mcp_text(HERMES_MCP_FIXTURE);
        assert_eq!(servers.len(), 3);
        assert_eq!(servers[0].name, "mx_data");
        assert!(servers[0].status.contains("enabled"));
        assert_eq!(servers[2].name, "paused_one");
        assert!(servers[2].status.contains("disabled"));
    }

    // Real fixture captured verbatim from `hermes memory status` (v0.11.0).
    const HERMES_MEMORY_FIXTURE: &str = "\
Memory status
────────────────────────────────────────
  Built-in:  always active
  Provider:  hindsight

  Plugin:    installed ✓
  Status:    available ✓

  Installed plugins:
";

    #[test]
    fn parses_hermes_memory_status() {
        let s = parse_hermes_memory_text(HERMES_MEMORY_FIXTURE);
        assert_eq!(s.provider, "hindsight");
        assert!(s.builtin_active);
    }

    // Real fixture captured verbatim from `hermes plugins list` (v0.11.0).
    const HERMES_PLUGINS_FIXTURE: &str = "\
                                    Plugins
┏━━━━━━━━━━━━━━┳━━━━━━━━━━━━━┳━━━━━━━━━┳━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┳━━━━━━━━━┓
┃ Name         ┃ Status      ┃ Version ┃ Description                 ┃ Source  ┃
┡━━━━━━━━━━━━━━╇━━━━━━━━━━━━━╇━━━━━━━━━╇━━━━━━━━━━━━━━━━━━━━━━━━━━━━━╇━━━━━━━━━┡
│ disk-cleanup │ not enabled │ 2.0.0   │ Auto-track and clean up     │ bundled │
│ google_meet  │ not enabled │ 0.2.0   │ Join a Google Meet call...   │ bundled │
└──────────────┴─────────────┴────────━┴─────────────────────────────┴─────────┘
";

    #[test]
    fn parses_hermes_plugins_table() {
        let plugins = parse_hermes_plugins_text(HERMES_PLUGINS_FIXTURE);
        assert_eq!(plugins.len(), 2);
        assert_eq!(plugins[0].id, "disk-cleanup");
        assert!(!plugins[0].enabled);  // "not enabled" → disabled
        assert_eq!(plugins[0].version, "2.0.0");
    }

    // Real fixture captured verbatim from `hermes tools list` (v0.11.0).
    const HERMES_TOOLS_FIXTURE: &str = "\
Built-in toolsets (cli):
  ✓ enabled  web  🔍 Web Search & Scraping
  ✗ disabled  moa  🧠 Mixture of Agents
  ✓ enabled  memory  💾 Memory
";

    #[test]
    fn parses_hermes_tools_section() {
        let tools = parse_hermes_tools_text(HERMES_TOOLS_FIXTURE);
        assert_eq!(tools.len(), 3);
        assert_eq!(tools[0].id, "web");
        assert!(tools[0].enabled);
        assert_eq!(tools[1].id, "moa");
        assert!(!tools[1].enabled);
    }

    // Real fixture captured verbatim from `hermes hooks list` (v0.11.0).
    const HERMES_HOOKS_EMPTY_FIXTURE: &str = "\
No shell hooks configured in ~/.hermes/config.yaml.
See `hermes hooks --help` or
    website/docs/user-guide/features/hooks.md
for the config schema and worked examples.
";

    #[test]
    fn parses_hermes_hooks_empty() {
        let hooks = parse_hermes_hooks_text(HERMES_HOOKS_EMPTY_FIXTURE);
        assert!(hooks.is_empty());
    }

    // Hypothetical fixture (per hermes hooks docs suggestion — actual format
    // unverified on this host since live hermes has no hooks configured).
    const HERMES_HOOKS_NONEMPTY_FIXTURE: &str = "\
Hook Name         Match                  Timeout  Consent
─────────────     ──────────────────     ───────  ───────
notify-desktop    command=notify-send    30s      ✓ allowed
log-event         match=*                60s      ✗ pending
";

    #[test]
    fn parses_hermes_hooks_nonempty() {
        let hooks = parse_hermes_hooks_text(HERMES_HOOKS_NONEMPTY_FIXTURE);
        assert_eq!(hooks.len(), 2);
        assert_eq!(hooks[0].id, "notify-desktop");
        assert!(hooks[0].enabled);  // "allowed" → enabled
        assert_eq!(hooks[1].id, "log-event");
        assert!(!hooks[1].enabled);  // "pending" → disabled
    }
}
