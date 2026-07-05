use std::process::Command;

use regex::Regex;

use super::{Backend, CronJob, GatewayStatus, NewCron};

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

    fn gateway_start(&self) -> Result<String, String> {
        run_hermes(&["gateway", "start"])
    }

    fn gateway_stop(&self) -> Result<String, String> {
        run_hermes(&["gateway", "stop"])
    }
    fn cron_list(&self) -> Result<Vec<CronJob>, String> {
        let output = Command::new("hermes")
            .args(["cron", "list"])
            .output()
            .map_err(|e| format!("Failed to run hermes: {}", e))?;
        if !output.status.success() {
            return Err(format!("hermes cron list failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()));
        }
        let text = String::from_utf8_lossy(&output.stdout);
        Ok(parse_hermes_cron_text(&text))
    }
    fn cron_create(&self, params: NewCron) -> Result<String, String> {
        let args = hermes_create_args(&params);
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        run_hermes(&refs)
    }

    fn cron_remove(&self, id: &str) -> Result<String, String> {
        run_hermes(&["cron", "remove", id])
    }

    fn cron_set_enabled(&self, id: &str, enabled: bool) -> Result<String, String> {
        let action = hermes_set_enabled_action(enabled);
        run_hermes(&["cron", action, id])
    }

    fn cron_run(&self, id: &str) -> Result<String, String> {
        run_hermes(&["cron", "run", id])
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

fn hermes_create_args(params: &NewCron) -> Vec<String> {
    let mut args = vec!["cron".into(), "create".into(), params.schedule.clone()];
    if let Some(m) = &params.message {
        args.push(m.clone());
    }
    args.push("--name".into());
    args.push(params.name.clone());
    args
}

fn hermes_set_enabled_action(enabled: bool) -> &'static str {
    if enabled { "resume" } else { "pause" }
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

/// Parse `hermes cron list` output. Real output is an ASCII table with one
/// job block per entry. Captured fixture (verbatim from Hermes v0.11.0):
///
/// ```text
/// ┌──────────────────────────────────────┐
/// │          Scheduled Jobs              │
/// └──────────────────────────────────────┘
///
///   f523f090a165 [active]
///     Name:      first_test
///     Schedule:  once in 30m
///     Repeat:    0/1
///     Next run:  2026-07-04T11:43:47Z
///     Deliver:   local
/// ```
fn parse_hermes_cron_text(text: &str) -> Vec<CronJob> {
    let trimmed = text.trim();
    if trimmed.is_empty()
        || trimmed.contains("No scheduled jobs")
        || trimmed.starts_with("Create one with")
    {
        return vec![];
    }

    let mut jobs: Vec<std::collections::BTreeMap<String, String>> = Vec::new();
    let mut current: Option<std::collections::BTreeMap<String, String>> = None;

    for raw_line in text.lines() {
        let line = raw_line.trim_end();
        let stripped = line.trim();

        // Skip blanks and table borders.
        if stripped.is_empty()
            || stripped.chars().all(|c| matches!(c, '┌' | '┐' | '└' | '┘' | '─' | '│' | ' '))
        {
            continue;
        }

        if let Some(caps) = HEADER_RE.captures(stripped) {
            if let Some(map) = current.take() { jobs.push(map); }
            let mut m = std::collections::BTreeMap::new();
            m.insert("id".into(), caps[1].to_string());
            m.insert("state".into(), caps[2].to_string());
            current = Some(m);
            continue;
        }

        if let Some(caps) = FIELD_RE.captures(stripped) {
            if let Some(map) = current.as_mut() {
                map.insert(caps[1].to_string(), caps[2].to_string());
            }
        }
    }
    if let Some(map) = current.take() { jobs.push(map); }
    jobs.into_iter().map(map_to_job).collect()
}

static HEADER_RE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"^([0-9a-f]+)\s+\[([a-z]+)\]\s*$").unwrap()
});
static FIELD_RE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"^([A-Za-z][A-Za-z _]*):\s*(.*)$").unwrap()
});

fn map_to_job(map: std::collections::BTreeMap<String, String>) -> CronJob {
    let id = map.get("id").cloned().unwrap_or_default();
    let name = map.get("Name").cloned().unwrap_or_else(|| id.clone());
    let schedule = map.get("Schedule").cloned().unwrap_or_default();
    let enabled = map.get("state").map(|s| s == "active").unwrap_or(true);
    let next_run = map.get("Next run").cloned();
    let message = map.get("Prompt").cloned();
    let raw = serde_json::to_value(&map).unwrap_or(serde_json::Value::Null);
    CronJob {
        id, name, schedule, enabled,
        last_run: None, next_run,
        agent: None, message, raw,
    }
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
    fn parses_empty_state() {
        let text = "No scheduled jobs.\nCreate one with 'hermes cron create ...' or the /cron command in chat.\n";
        let jobs = parse_hermes_cron_text(text);
        assert!(jobs.is_empty());
    }

    // Real fixtures captured verbatim from `hermes cron list` (v0.11.0).
    const REAL_SINGLE: &str = "\
┌─────────────────────────────────────────────────────────────────────────┐
│                         Scheduled Jobs                                  │
└─────────────────────────────────────────────────────────────────────────┘

  f523f090a165 [active]
    Name:      first_test
    Schedule:  once in 30m
    Repeat:    0/1
    Next run:  2026-07-04T11:43:47.424517+08:00
    Deliver:   local
";

    const REAL_MULTI: &str = "\
┌─────────────────────────────────────────────────────────────────────────┐
│                         Scheduled Jobs                                  │
└─────────────────────────────────────────────────────────────────────────┘

  f523f090a165 [active]
    Name:      first_test
    Schedule:  once in 30m
    Next run:  2026-07-04T11:43:47Z

  a1a472ca98a9 [paused]
    Name:      second_test
    Schedule:  every 120m
    Next run:  -
";

    #[test]
    fn parses_real_single_job() {
        let jobs = parse_hermes_cron_text(REAL_SINGLE);
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].id, "f523f090a165");
        assert_eq!(jobs[0].name, "first_test");
        assert_eq!(jobs[0].schedule, "once in 30m");
        assert!(jobs[0].enabled); // [active]
        assert_eq!(jobs[0].next_run.as_deref(),
                   Some("2026-07-04T11:43:47.424517+08:00"));
    }

    #[test]
    fn parses_real_multiple_jobs_with_paused_state() {
        let jobs = parse_hermes_cron_text(REAL_MULTI);
        assert_eq!(jobs.len(), 2);
        assert_eq!(jobs[0].id, "f523f090a165");
        assert!(jobs[0].enabled);
        assert_eq!(jobs[1].id, "a1a472ca98a9");
        assert!(!jobs[1].enabled); // [paused]
    }

    #[test]
    fn hermes_create_args_with_prompt() {
        let args = hermes_create_args(&NewCron {
            name: "nightly".into(),
            schedule: "0 2 * * *".into(),
            message: Some("do thing".into()),
            agent: None,
        });
        assert_eq!(args, vec![
            "cron".to_string(),
            "create".to_string(),
            "0 2 * * *".to_string(),
            "do thing".to_string(),
            "--name".to_string(),
            "nightly".to_string(),
        ]);
    }

    #[test]
    fn hermes_create_args_without_prompt() {
        let args = hermes_create_args(&NewCron {
            name: "tick".into(), schedule: "30m".into(), message: None, agent: None,
        });
        assert_eq!(args, vec![
            "cron".to_string(), "create".to_string(),
            "30m".to_string(), "--name".to_string(), "tick".to_string(),
        ]);
    }

    #[test]
    fn hermes_enable_maps_to_resume() {
        assert_eq!(hermes_set_enabled_action(true), "resume");
        assert_eq!(hermes_set_enabled_action(false), "pause");
    }

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
}
