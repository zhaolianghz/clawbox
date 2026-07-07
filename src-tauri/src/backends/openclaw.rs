use std::process::Command;
use serde_json::Value;

const GATEWAY_PORT: u16 = 18789;

/// Run an `openclaw` subcommand and parse its stdout as JSON.
pub fn openclaw_json(args: &[&str]) -> Result<Value, String> {
    let output = Command::new("openclaw")
        .args(args)
        .output()
        .map_err(|e| format!("Failed to run openclaw: {}", e))?;
    if !output.status.success() {
        return Err(format!("openclaw {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let trimmed = stdout.trim();
    if trimmed.is_empty() { return Ok(Value::Null); }
    serde_json::from_str(trimmed).map_err(|e| format!("Failed to parse openclaw output: {}", e))
}

/// Run an `openclaw` subcommand for its side effect, returning stdout.
pub fn openclaw_run(args: &[&str]) -> Result<String, String> {
    let output = Command::new("openclaw")
        .args(args)
        .output()
        .map_err(|e| format!("Failed to run openclaw: {}", e))?;
    if !output.status.success() {
        return Err(format!("openclaw {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

use super::{Backend, CronJob, GatewayStatus, NewCron};

/// Trust boundary: `gateway_pid()` discovers any PID listening on
/// `GATEWAY_PORT` and trusts that it is openclaw's gateway. Before claiming or
/// killing it, we re-check the process's comm (via `ps -o comm=`) and reject
/// anything that is not openclaw itself or the Node.js interpreter running
/// openclaw's bundled JS. This guards against a stale dev server or stray
/// `nc -l 18789` causing `gateway_start` to skip starting or `gateway_stop`
/// to kill an unrelated PID.
fn gateway_pid() -> Option<i32> {
    let pid = Command::new("lsof")
        .args(["-t", "-i", &format!(":{}", GATEWAY_PORT), "-sTCP:LISTEN"])
        .output().ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8_lossy(&o.stdout).lines().next()
            .and_then(|l| l.trim().parse::<i32>().ok()))?;

    let comm = Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "comm="])
        .output().ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    if is_openclaw_process(&comm) { Some(pid) } else { None }
}

/// Pure helper: returns true when `comm` (the trimmed output of
/// `ps -o comm=`) looks like the openclaw gateway process. Accepts the
/// `openclaw` binary itself and any `node` process (openclaw ships a
/// Node.js entrypoint).
fn is_openclaw_process(comm: &str) -> bool {
    let lower = comm.trim().to_lowercase();
    lower.contains("openclaw") || lower.starts_with("node")
}

pub struct OpenClawBackend;

impl Backend for OpenClawBackend {
    fn id(&self) -> &'static str { "openclaw" }
    fn display_name(&self) -> &'static str { "OpenClaw" }
    fn version(&self) -> String {
        Command::new("openclaw").arg("--version").output().ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "unknown".into())
    }

    fn is_installed(&self) -> bool {
        Command::new("openclaw").arg("--version").output()
            .map(|o| o.status.success()).unwrap_or(false)
    }
    fn gateway_status(&self) -> Result<GatewayStatus, String> {
        let pid = gateway_pid();
        let status = if pid.is_some() { "running" } else { "stopped" }.to_string();
        Ok(GatewayStatus { status, version: self.version(), pid })
    }

    fn gateway_start(&self) -> Result<String, String> {
        if gateway_pid().is_some() {
            return Ok("Gateway already running".into());
        }
        use std::process::Stdio;
        Command::new("openclaw")
            .args(["gateway", "run", "--port", &GATEWAY_PORT.to_string()])
            .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Failed to start gateway: {}", e))?;
        Ok("Gateway starting".into())
    }

    fn gateway_stop(&self) -> Result<String, String> {
        match gateway_pid() {
            Some(pid) => {
                Command::new("kill").arg(pid.to_string()).output()
                    .map_err(|e| format!("Failed to stop gateway: {}", e))?;
                Ok("Gateway stopping".into())
            }
            None => Ok("Gateway not running".into()),
        }
    }
    fn cron_list(&self) -> Result<Vec<CronJob>, String> {
        let raw = openclaw_json(&["cron", "list", "--json"])?;
        let arr = match raw {
            serde_json::Value::Object(ref m) if m.contains_key("jobs") => m["jobs"].clone(),
            other => other,
        };
        let arr = arr.as_array().cloned().unwrap_or_default();
        Ok(arr.into_iter().map(normalise_openclaw_job).collect())
    }
    fn cron_create(&self, params: NewCron) -> Result<String, String> {
        let args = openclaw_create_args(&params);
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        openclaw_run(&refs)
    }

    fn cron_remove(&self, id: &str) -> Result<String, String> {
        openclaw_run(&["cron", "rm", id])
    }

    fn cron_set_enabled(&self, id: &str, enabled: bool) -> Result<String, String> {
        let action = if enabled { "enable" } else { "disable" };
        openclaw_run(&["cron", action, id])
    }

    fn cron_run(&self, id: &str) -> Result<String, String> {
        openclaw_run(&["cron", "run", id])
    }
}

impl super::capabilities::SkillsCapability for OpenClawBackend {
    fn skills_list(&self) -> Result<Vec<super::capabilities::Skill>, String> {
        let raw = openclaw_json(&["skills", "list", "--json"])?;
        Ok(parse_openclaw_skills(raw))
    }
    fn skills_install(&self, id: &str) -> Result<String, String> {
        openclaw_run(&["skills", "install", id])
    }
    fn skills_uninstall(&self, id: &str) -> Result<String, String> {
        openclaw_run(&["skills", "uninstall", id])
    }
    fn skills_set_enabled(&self, id: &str, enabled: bool) -> Result<String, String> {
        let action = if enabled { "enable" } else { "disable" };
        openclaw_run(&["skills", action, id])
    }
}

impl super::capabilities::McpCapability for OpenClawBackend {
    fn mcp_list(&self) -> Result<Vec<super::capabilities::McpServer>, String> {
        let raw = openclaw_json(&["mcp", "list", "--json"])?;
        Ok(parse_openclaw_mcp(raw))
    }
    fn mcp_add(&self, name: &str, config_json: &str) -> Result<String, String> {
        openclaw_run(&["mcp", "set", name, config_json])
    }
    fn mcp_remove(&self, name: &str) -> Result<String, String> {
        openclaw_run(&["mcp", "unset", name])
    }
}

impl super::capabilities::MemoryCapability for OpenClawBackend {
    fn memory_status(&self) -> Result<super::capabilities::MemoryStatus, String> {
        // openclaw has no "memory status" — best effort: try index --json, fall back to defaults
        let raw = openclaw_json(&["memory", "index", "--json"]).unwrap_or(serde_json::json!({}));
        Ok(parse_openclaw_memory_status(raw))
    }
    fn memory_index(&self) -> Result<String, String> {
        openclaw_run(&["memory", "index"])
    }
    fn memory_reset(&self) -> Result<String, String> {
        Err("openclaw does not support memory reset".to_string())
    }
}

fn parse_openclaw_memory_status(raw: serde_json::Value) -> super::capabilities::MemoryStatus {
    let provider = raw.get("provider").and_then(|v| v.as_str()).unwrap_or("builtin").to_string();
    let builtin_active = provider == "builtin" || raw.get("builtin").and_then(|v| v.as_bool()).unwrap_or(false);
    super::capabilities::MemoryStatus { provider, builtin_active, raw }
}

impl super::capabilities::HooksCapability for OpenClawBackend {
    fn hooks_list(&self) -> Result<Vec<super::capabilities::Hook>, String> {
        let output = std::process::Command::new("openclaw")
            .args(["hooks", "list"])
            .output()
            .map_err(|e| format!("Failed to run openclaw: {}", e))?;
        if !output.status.success() {
            return Err(format!("openclaw hooks list failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()));
        }
        Ok(parse_openclaw_hooks_text(&String::from_utf8_lossy(&output.stdout)))
    }
    fn hooks_set_enabled(&self, id: &str, enabled: bool) -> Result<String, String> {
        let action = if enabled { "enable" } else { "disable" };
        openclaw_run(&["hooks", action, id])
    }
}

fn parse_openclaw_hooks_text(text: &str) -> Vec<super::capabilities::Hook> {
    if text.trim().is_empty() { return vec![]; }
    let mut hooks: Vec<super::capabilities::Hook> = Vec::new();
    let mut current: Option<super::capabilities::Hook> = None;
    for line in text.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with('│') { continue; }
        let cells: Vec<&str> = trimmed.split('│').map(str::trim).collect();
        if cells.len() < 3 { continue; }
        // Skip header row ("Status") and separator rows.
        if cells[1].eq_ignore_ascii_case("status") || cells[1].contains('─') { continue; }
        // Distinguish full row from continuation by status-cell emptiness.
        // Continuation rows have an empty/whitespace status cell.
        let status = cells[1];
        if !status.is_empty() {
            // Full row: flush previous, start a new hook.
            if let Some(h) = current.take() { hooks.push(h); }
            let name = cells.get(2).copied().unwrap_or("");
            let enabled = status.contains("ready") || (status.contains("enabled") && !status.contains("disabled"));
            current = Some(super::capabilities::Hook {
                id: name.to_string(),
                name: name.to_string(),
                event: String::new(),
                enabled,
                raw: serde_json::json!({}),
            });
        } else if let Some(ref mut h) = current {
            // Continuation row: append wrapped cells[2] (name/desc) and cells[3] (desc) into the hook.
            if let Some(c) = cells.get(2) {
                if !c.is_empty() { h.name.push_str(c); h.id.push_str(c); }
            }
            if let Some(c) = cells.get(3) {
                if !c.is_empty() {
                    if !h.raw.as_object().map(|o| o.contains_key("description")).unwrap_or(false) {
                        h.raw["description"] = serde_json::Value::String(c.to_string());
                    }
                }
            }
        }
    }
    if let Some(h) = current.take() { hooks.push(h); }
    hooks
}

impl super::capabilities::PluginsCapability for OpenClawBackend {
    fn plugins_list(&self) -> Result<Vec<super::capabilities::Plugin>, String> {
        // openclaw plugins list has no --json; capture stdout as text
        let output = std::process::Command::new("openclaw")
            .args(["plugins", "list"])
            .output()
            .map_err(|e| format!("Failed to run openclaw: {}", e))?;
        if !output.status.success() {
            return Err(format!("openclaw plugins list failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()));
        }
        Ok(parse_openclaw_plugins_text(&String::from_utf8_lossy(&output.stdout)))
    }
    fn plugins_install(&self, source: &str) -> Result<String, String> {
        openclaw_run(&["plugins", "install", source])
    }
    fn plugins_remove(&self, id: &str) -> Result<String, String> {
        openclaw_run(&["plugins", "disable", id])  // openclaw uses disable, not remove
    }
    fn plugins_set_enabled(&self, id: &str, enabled: bool) -> Result<String, String> {
        let action = if enabled { "enable" } else { "disable" };
        openclaw_run(&["plugins", action, id])
    }
}

fn parse_openclaw_plugins_text(text: &str) -> Vec<super::capabilities::Plugin> {
    if text.trim().is_empty() || text.contains("No plugins") { return vec![]; }
    let mut plugins: Vec<super::capabilities::Plugin> = Vec::new();
    let mut current: Option<super::capabilities::Plugin> = None;
    for line in text.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with('│') { continue; }
        let cells: Vec<&str> = trimmed.split('│').map(str::trim).collect();
        if cells.len() < 7 { continue; }
        // Skip header/separator rows.
        if cells[1].eq_ignore_ascii_case("name") || cells[1].contains('─') || cells[1].contains('┼') { continue; }
        let status = cells[4];
        let is_data_row = status == "loaded" || status == "disabled"
            || (status.contains("enabled") && !status.contains("disabled"));
        if is_data_row {
            if let Some(p) = current.take() { plugins.push(p); }
            current = Some(super::capabilities::Plugin {
                id: cells[2].to_string(),
                name: cells[1].to_string(),
                version: cells[6].to_string(),
                enabled: status == "loaded" || (status.contains("enabled") && !status.contains("disabled")),
                raw: serde_json::json!({}),
            });
        } else if let Some(ref mut p) = current {
            // Continuation row: wrapped name (cells[1]) or wrapped source description (cells[5]).
            if !cells[1].is_empty() { p.name.push(' '); p.name.push_str(cells[1]); }
            if !cells[2].is_empty() { p.id.push_str(cells[2]); }
            if !cells[5].is_empty() {
                if !p.raw.as_object().map(|o| o.contains_key("description")).unwrap_or(false) {
                    p.raw["description"] = serde_json::Value::String(cells[5].to_string());
                }
            }
        }
    }
    if let Some(p) = current.take() { plugins.push(p); }
    plugins
}

fn openclaw_create_args(params: &NewCron) -> Vec<String> {
    let mut args = vec!["cron".into(), "add".into(), "--json".into(), "--name".into(), params.name.clone()];
    let is_interval = params.schedule.contains(char::is_alphabetic);
    if is_interval {
        args.push("--every".into()); args.push(params.schedule.clone());
    } else {
        args.push("--cron".into()); args.push(params.schedule.clone());
    }
    if let Some(m) = &params.message { args.push("--message".into()); args.push(m.clone()); }
    if let Some(a) = &params.agent   { args.push("--agent".into());   args.push(a.clone()); }
    args
}

fn parse_openclaw_skills(raw: serde_json::Value) -> Vec<super::capabilities::Skill> {
    let arr = match raw {
        serde_json::Value::Object(ref m) if m.contains_key("skills") => m["skills"].clone(),
        other => other,
    };
    let arr = arr.as_array().cloned().unwrap_or_default();
    arr.into_iter().map(|v| {
        let o = v.as_object().cloned().unwrap_or_default();
        let id = o.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let name = id.clone();
        let description = o.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let version = o.get("version").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let enabled = !o.get("disabled").and_then(|v| v.as_bool()).unwrap_or(false);
        super::capabilities::Skill {
            id, name, version, description, enabled, raw: v,
        }
    }).collect()
}

fn parse_openclaw_mcp(raw: serde_json::Value) -> Vec<super::capabilities::McpServer> {
    let map = raw.as_object().cloned().unwrap_or_default();
    let servers_val = map.get("servers").cloned().unwrap_or(serde_json::Value::Object(Default::default()));
    let servers_obj = servers_val.as_object().cloned().unwrap_or_default();
    let mut entries: Vec<(String, serde_json::Value)> = servers_obj.into_iter().collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    entries.into_iter().map(|(name, cfg)| {
        let cfg_obj = cfg.as_object().cloned().unwrap_or_default();
        let transport = cfg_obj.get("command")
            .and_then(|v| v.as_str())
            .unwrap_or("?")
            .to_string();
        let disabled = cfg_obj.get("disabled").and_then(|v| v.as_bool()).unwrap_or(false);
        let status = if disabled { "disabled".to_string() } else { "enabled".to_string() };
        super::capabilities::McpServer {
            name,
            transport,
            status,
            raw: cfg,
        }
    }).collect()
}

fn normalise_openclaw_job(raw: serde_json::Value) -> CronJob {
    let obj = raw.as_object().cloned().unwrap_or_default();
    let id = obj.get("id").and_then(|v| v.as_str())
        .or_else(|| obj.get("jobId").and_then(|v| v.as_str()))
        .or_else(|| obj.get("name").and_then(|v| v.as_str()))
        .unwrap_or("")
        .to_string();
    let name = obj.get("name").and_then(|v| v.as_str()).unwrap_or(&id).to_string();
    let schedule = obj.get("cron").and_then(|v| v.as_str())
        .or_else(|| obj.get("every").and_then(|v| v.as_str()))
        .or_else(|| obj.get("schedule").and_then(|v| v.as_str()))
        .unwrap_or("")
        .to_string();
    let enabled = match obj.get("enabled").and_then(|v| v.as_bool()) {
        Some(b) => b,
        None => !obj.get("disabled").and_then(|v| v.as_bool()).unwrap_or(false),
    };
    CronJob {
        id, name, schedule, enabled,
        last_run: obj.get("lastRun").and_then(|v| v.as_str()).map(String::from)
            .or_else(|| obj.get("last_run").and_then(|v| v.as_str()).map(String::from)),
        next_run: obj.get("nextRun").and_then(|v| v.as_str()).map(String::from)
            .or_else(|| obj.get("next_run").and_then(|v| v.as_str()).map(String::from)),
        agent: obj.get("agent").and_then(|v| v.as_str()).map(String::from),
        message: obj.get("message").and_then(|v| v.as_str()).map(String::from),
        raw,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn normalise_openclaw_job_basic() {
        let raw = json!({
            "id": "abc",
            "name": "nightly",
            "cron": "0 2 * * *",
            "enabled": true,
            "agent": "default",
            "message": "run report"
        });
        let job = normalise_openclaw_job(raw);
        assert_eq!(job.id, "abc");
        assert_eq!(job.name, "nightly");
        assert_eq!(job.schedule, "0 2 * * *");
        assert!(job.enabled);
        assert_eq!(job.agent.as_deref(), Some("default"));
    }

    #[test]
    fn normalise_openclaw_job_disabled_via_disabled_flag() {
        let raw = json!({ "id": "x", "name": "x", "cron": "* * * * *", "disabled": true });
        let job = normalise_openclaw_job(raw);
        assert!(!job.enabled);
    }

    #[test]
    fn normalise_openclaw_job_falls_back_to_every() {
        let raw = json!({ "id": "x", "name": "x", "every": "30m" });
        let job = normalise_openclaw_job(raw);
        assert_eq!(job.schedule, "30m");
    }

    #[test]
    fn openclaw_create_args() {
        let args = super::openclaw_create_args(&NewCron {
            name: "nightly".into(),
            schedule: "0 2 * * *".into(),
            message: Some("do thing".into()),
            agent: Some("default".into()),
        });
        assert_eq!(args, vec![
            "cron", "add", "--json", "--name", "nightly",
            "--cron", "0 2 * * *",
            "--message", "do thing",
            "--agent", "default",
        ]);
    }

    #[test]
    fn openclaw_create_args_every_fallback() {
        let args = super::openclaw_create_args(&NewCron {
            name: "tick".into(), schedule: "30m".into(), message: None, agent: None,
        });
        assert!(args.contains(&"--every".to_string()));
        assert!(args.contains(&"30m".to_string()));
        assert!(!args.contains(&"--cron".to_string()));
    }

    #[test]
    fn openclaw_skills_normalises_object() {
        let raw = json!({
            "skills": [{
                "name": "code-review",
                "description": "Review code",
                "emoji": "🔍",
                "eligible": true,
                "disabled": false,
                "homepage": "https://example.com",
                "bundled": true,
                "source": "openclaw-bundled"
            }]
        });
        let skills = parse_openclaw_skills(raw);
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].id, "code-review");
        assert_eq!(skills[0].name, "code-review");
        assert!(skills[0].enabled);
    }

    #[test]
    fn openclaw_skills_disabled_flag() {
        let raw = json!({"skills": [{"name": "x", "description": "", "disabled": true}]});
        let skills = parse_openclaw_skills(raw);
        assert_eq!(skills.len(), 1);
        assert!(!skills[0].enabled);
    }

    #[test]
    fn openclaw_skills_root_array() {
        let raw = json!([{"name": "a", "description": ""}]);
        let skills = parse_openclaw_skills(raw);
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "a");
    }

    #[test]
    fn openclaw_mcp_two_servers_in_servers_object() {
        let raw = json!({
            "servers": {
                "mx_data": {"command": "uvx", "args": ["mxAi/mcp-mxdata"]},
                "codegraph": {"command": "codegraph", "args": ["serve", "--mcp"]}
            }
        });
        let servers = parse_openclaw_mcp(raw);
        assert_eq!(servers.len(), 2);
        // Output is sorted by name for determinism (HashMap order is unstable).
        assert_eq!(servers[0].name, "codegraph");
        assert_eq!(servers[0].transport, "codegraph");
        assert!(servers[0].status.contains("enabled"));
        assert_eq!(servers[1].name, "mx_data");
        assert_eq!(servers[1].transport, "uvx");
    }

    #[test]
    fn openclaw_mcp_empty_servers_object() {
        let raw = json!({"servers": {}});
        let servers = parse_openclaw_mcp(raw);
        assert!(servers.is_empty());
    }

    #[test]
    fn openclaw_mcp_single_server_uses_command_as_transport() {
        let raw = json!({"servers": {"x": {"command": "foo"}}});
        let servers = parse_openclaw_mcp(raw);
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].name, "x");
        assert_eq!(servers[0].transport, "foo");
        assert!(servers[0].status.contains("enabled"));
    }

    #[test]
    fn openclaw_mcp_disabled_flag_marks_status_disabled() {
        let raw = json!({"servers": {"paused_one": {"command": "foo", "disabled": true}}});
        let servers = parse_openclaw_mcp(raw);
        assert_eq!(servers.len(), 1);
        assert!(servers[0].status.contains("disabled"));
    }

    #[test]
    fn openclaw_memory_status_default() {
        let raw = json!({});
        let s = parse_openclaw_memory_status(raw);
        assert_eq!(s.provider, "builtin");
        assert!(s.builtin_active);
    }

    #[test]
    fn openclaw_memory_status_with_provider() {
        let raw = json!({"provider": "external"});
        let s = parse_openclaw_memory_status(raw);
        assert_eq!(s.provider, "external");
        assert!(!s.builtin_active);
    }

    // Real fixture captured verbatim from `openclaw plugins list` (2026.4.11).
    const OPENCLAW_PLUGINS_FIXTURE: &str = "\
Plugins (54/97 loaded)
Source roots:
  stock: /opt/homebrew/lib/node_modules/openclaw/dist/extensions

┌──────────────┬──────────┬──────────┬──────────┬──────────────────────────────────────────────────────────┬───────────┐
│ Name         │ ID       │ Format   │ Status   │ Source                                                   │ Version   │
├──────────────┼──────────┼──────────┼──────────┼──────────────────────────────────────────────────────────┼───────────┤
│ ACPX Runtime │ acpx     │ openclaw │ loaded   │ stock:acpx/index.js                                      │ 2026.4.11 │
│              │          │          │          │ Embedded ACP runtime backend with plugin-owned session   │           │
│              │          │          │          │ and transport management.                                │           │
│ Active       │ active-  │ openclaw │ disabled │ stock:active-memory/index.js                             │ 2026.4.11 │
│ Memory       │ memory   │          │          │ Runs a bounded blocking memory sub-agent before          │           │
│              │          │          │          │ eligible conversational replies and injects relevant     │           │
│              │          │          │          │ memory into prompt context.                              │           │
└──────────────┴──────────┴──────────┴──────────┴──────────────────────────────────────────────────────────┴───────────┘
";

    #[test]
    fn openclaw_plugins_parses_table() {
        let plugins = parse_openclaw_plugins_text(OPENCLAW_PLUGINS_FIXTURE);
        assert_eq!(plugins.len(), 2);
        assert_eq!(plugins[0].id, "acpx");
        assert!(plugins[0].enabled);
        assert_eq!(plugins[0].name, "ACPX Runtime");
        assert_eq!(plugins[1].id, "active-memory");
        assert_eq!(plugins[1].name, "Active Memory");
        assert!(!plugins[1].enabled);
    }

    #[test]
    fn openclaw_plugins_empty() {
        let plugins = parse_openclaw_plugins_text("No plugins.\n");
        assert!(plugins.is_empty());
    }

    // Real fixture captured verbatim from `openclaw hooks list` (2026.4.11).
    const OPENCLAW_HOOKS_FIXTURE: &str = "\
Hooks (5/5 ready)
┌──────────┬────────────────────────────────┬────────────────────────────────┬────────────┐
│ Status   │ Hook                           │ Description                    │ Source     │
├──────────┼────────────────────────────────┼────────────────────────────────┼────────────┤
│ ✓ ready  │ 🚀 boot-md                     │ Run BOOT.md on gateway startup │ openclaw-  │
│          │                                │                                │ bundled    │
│ ✓ ready  │ 📎 bootstrap-extra-files       │ Inject additional files        │ openclaw-  │
│          │                                │                                │ bundled    │
└──────────┴────────────────────────────────┴────────────────────────────────┴────────────┘
";

    #[test]
    fn openclaw_hooks_parses_table() {
        let hooks = parse_openclaw_hooks_text(OPENCLAW_HOOKS_FIXTURE);
        assert_eq!(hooks.len(), 2);
        assert_eq!(hooks[0].id, "🚀 boot-md");
        assert!(hooks[0].enabled);  // "ready" → enabled
        assert_eq!(hooks[1].id, "📎 bootstrap-extra-files");
    }

    #[test]
    fn is_openclaw_process_accepts_openclaw_binary() {
        assert!(is_openclaw_process("openclaw"));
        assert!(is_openclaw_process("/opt/homebrew/bin/openclaw"));
        assert!(is_openclaw_process("OpenClaw"));
        assert!(is_openclaw_process("openclaw-dev-server"));
    }

    #[test]
    fn is_openclaw_process_accepts_node_runtime() {
        assert!(is_openclaw_process("node"));
    }

    #[test]
    fn is_openclaw_process_rejects_unrelated_processes() {
        assert!(!is_openclaw_process("nc"));
        assert!(!is_openclaw_process("python3"));
        assert!(!is_openclaw_process(""));
        assert!(!is_openclaw_process("/usr/local/bin/node"));
        assert!(!is_openclaw_process("rustc"));
    }
}
