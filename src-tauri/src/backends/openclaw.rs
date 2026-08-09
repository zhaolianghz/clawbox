use serde_json::Value;

const GATEWAY_PORT: u16 = 18789;

/// Run an `openclaw` subcommand and parse its stdout as JSON.
pub fn openclaw_json(args: &[&str]) -> Result<Value, String> {
    let output = crate::proc::command("openclaw")
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
    let output = crate::proc::command("openclaw")
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
/// process's name (unix `ps -o comm=`, Windows `tasklist`) and reject anything
/// that is not openclaw itself or the Node.js interpreter running openclaw's
/// bundled JS. This guards against a stale dev server or stray `nc -l 18789`
/// being reported as a running gateway.
fn gateway_pid() -> Option<i32> {
    let pid = listening_pid()?;
    if is_openclaw_process(&process_name(pid)) { Some(pid) } else { None }
}

/// PID of the socket listening on `GATEWAY_PORT`, if any.
#[cfg(not(windows))]
fn listening_pid() -> Option<i32> {
    crate::proc::command("lsof")
        .args(["-t", "-i", &format!(":{}", GATEWAY_PORT), "-sTCP:LISTEN"])
        .output().ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8_lossy(&o.stdout).lines().next()
            .and_then(|l| l.trim().parse::<i32>().ok()))
}

/// Windows ships neither lsof nor ps; `netstat -ano` is the built-in that
/// carries the owning PID. Without this the gateway reads "stopped" on every
/// Windows box no matter what's running.
#[cfg(windows)]
fn listening_pid() -> Option<i32> {
    let out = crate::proc::command("netstat")
        .args(["-ano", "-p", "TCP"])
        .output().ok()
        .filter(|o| o.status.success())?;
    parse_netstat_listening_pid(&String::from_utf8_lossy(&out.stdout), GATEWAY_PORT)
}

/// Pure helper: the PID owning the listening socket on `port` in `netstat -ano`
/// output, whose columns are `TCP <local> <remote> <state> <pid>`.
///
/// We key off the wildcard remote address (`0.0.0.0:0` / `[::]:0`) that only a
/// listening socket carries, never off the literal word LISTENING: the state
/// column's localization varies by Windows locale, and matching the local port
/// alone would also match a *client* socket connected to the gateway.
///
/// Compiled on every platform (hence the allow): the parsing is where the bugs
/// live, and gating it behind `cfg(windows)` would hide it from the macOS/CI
/// test run that is the only one anyone routinely executes.
#[cfg_attr(not(windows), allow(dead_code))]
fn parse_netstat_listening_pid(out: &str, port: u16) -> Option<i32> {
    let local_suffix = format!(":{}", port);
    out.lines().find_map(|line| {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 5 || !cols[0].eq_ignore_ascii_case("tcp") {
            return None;
        }
        if !cols[1].ends_with(&local_suffix) || !cols[2].ends_with(":0") {
            return None;
        }
        cols[4].parse::<i32>().ok()
    })
}

/// Process name for `pid`, or "" when it can't be determined.
#[cfg(not(windows))]
fn process_name(pid: i32) -> String {
    crate::proc::command("ps")
        .args(["-p", &pid.to_string(), "-o", "comm="])
        .output().ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

#[cfg(windows)]
fn process_name(pid: i32) -> String {
    crate::proc::command("tasklist")
        // CSV + no header: one quoted row we can parse without column math.
        .args(["/FI", &format!("PID eq {}", pid), "/FO", "CSV", "/NH"])
        .output().ok()
        .filter(|o| o.status.success())
        .map(|o| parse_tasklist_image_name(&String::from_utf8_lossy(&o.stdout)).unwrap_or_default())
        .unwrap_or_default()
}

/// Pure helper: image name out of a `tasklist /FO CSV /NH` row, e.g.
/// `"node.exe","12345","Console","1","54,321 K"` -> `node.exe`. A filter that
/// matched nothing prints a localized `INFO:` line instead of a quoted row,
/// which yields None (and thus a rejected PID). Compiled on every platform so
/// the macOS/CI test run covers it — see parse_netstat_listening_pid.
#[cfg_attr(not(windows), allow(dead_code))]
fn parse_tasklist_image_name(out: &str) -> Option<String> {
    let row = out.lines().map(str::trim).find(|l| l.starts_with('"'))?;
    let name = row.trim_start_matches('"').split('"').next()?;
    if name.is_empty() { None } else { Some(name.to_string()) }
}

/// Pure helper: returns true when `name` (a `ps -o comm=` value on unix, a
/// tasklist image name on Windows) looks like the openclaw gateway process.
/// Accepts the `openclaw` binary itself and any `node` process (openclaw ships
/// a Node.js entrypoint) — including their `.exe` forms.
fn is_openclaw_process(name: &str) -> bool {
    let lower = name.trim().to_lowercase();
    lower.contains("openclaw") || lower.starts_with("node")
}

pub struct OpenClawBackend;

impl Backend for OpenClawBackend {
    fn id(&self) -> &'static str { "openclaw" }
    fn display_name(&self) -> &'static str { "OpenClaw" }
    fn version(&self) -> String {
        crate::proc::command("openclaw").arg("--version").output().ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "unknown".into())
    }

    fn is_installed(&self) -> bool {
        crate::proc::command("openclaw").arg("--version").output()
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

    #[test]
    fn is_openclaw_process_accepts_windows_image_names() {
        // tasklist reports "node.exe" / "openclaw.exe", not bare names.
        assert!(is_openclaw_process("node.exe"));
        assert!(is_openclaw_process("openclaw.exe"));
        assert!(!is_openclaw_process("nc.exe"));
    }

    /// `netstat -ano -p TCP` on Windows, header included.
    const NETSTAT_FIXTURE: &str = "\
Active Connections

  Proto  Local Address          Foreign Address        State           PID
  TCP    0.0.0.0:135            0.0.0.0:0              LISTENING       1044
  TCP    127.0.0.1:18789        127.0.0.1:52341        ESTABLISHED     4711
  TCP    0.0.0.0:18789          0.0.0.0:0              LISTENING       9312
  TCP    [::]:18789             [::]:0                 LISTENING       9312
";

    #[test]
    fn netstat_picks_the_listener_not_a_client_socket() {
        // 4711 is a client connected *to* the gateway and shares the local
        // port; only the wildcard-remote row is the gateway itself.
        assert_eq!(parse_netstat_listening_pid(NETSTAT_FIXTURE, 18789), Some(9312));
    }

    #[test]
    fn netstat_ignores_the_localized_state_column() {
        // German/Chinese/... Windows renders the state word differently; the
        // parse must not depend on it.
        let out = "  TCP    0.0.0.0:18789    0.0.0.0:0    ABHÖREN    777\n";
        assert_eq!(parse_netstat_listening_pid(out, 18789), Some(777));
    }

    #[test]
    fn netstat_matches_the_whole_port_only() {
        // Suffix matching must not let 18789 satisfy a probe for 8789, nor a
        // 118789 listener satisfy one for 18789.
        assert_eq!(parse_netstat_listening_pid(NETSTAT_FIXTURE, 8789), None);
        let out = "  TCP    0.0.0.0:118789    0.0.0.0:0    LISTENING    777\n";
        assert_eq!(parse_netstat_listening_pid(out, 18789), None);
    }

    #[test]
    fn netstat_returns_none_when_port_is_free() {
        assert_eq!(parse_netstat_listening_pid(NETSTAT_FIXTURE, 4000), None);
        assert_eq!(parse_netstat_listening_pid("", 18789), None);
    }

    #[test]
    fn tasklist_extracts_the_image_name() {
        let out = "\"node.exe\",\"9312\",\"Console\",\"1\",\"54,321 K\"\r\n";
        assert_eq!(parse_tasklist_image_name(out).as_deref(), Some("node.exe"));
    }

    #[test]
    fn tasklist_returns_none_when_the_filter_matched_nothing() {
        // Localized INFO line, no quoted row — the PID must end up rejected
        // rather than accepted on an empty name.
        let out = "INFO: No tasks are running which match the specified criteria.\r\n";
        assert_eq!(parse_tasklist_image_name(out), None);
        assert!(!is_openclaw_process(&parse_tasklist_image_name(out).unwrap_or_default()));
    }
}
