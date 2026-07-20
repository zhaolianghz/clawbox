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

use super::{Backend, GatewayStatus};

/// Trust boundary: `gateway_pid()` discovers any PID listening on
/// `GATEWAY_PORT` and trusts that it is openclaw's gateway. We re-check the
/// process's comm (via `ps -o comm=`) and reject anything that is not
/// openclaw itself or the Node.js interpreter running openclaw's bundled JS.
/// This guards against a stale dev server or stray `nc -l 18789` being
/// reported as a running gateway.
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
        // openclaw 没有 `mcp unset` 子命令;删除走 config unset 点路径
        openclaw_run(&["config", "unset", &format!("mcp.servers.{}", name)])
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
    // Real fixture captured verbatim from `openclaw hooks list` (2026.4.11).
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
