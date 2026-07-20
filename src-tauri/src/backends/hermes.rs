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
        let args = hermes_mcp_add_args(name, config_json)?;
        // hermes `mcp add` 不是 upsert:同名条目已存在时先移除再添加保证幂等
        // (update 语义由 CliMcpAdapter 的 re-add 承载)。移除失败(不存在)忽略。
        let _ = run_hermes(&["mcp", "remove", name]);
        let refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        // add 完成工具发现后会交互确认「Enable all N tools? [Y/n/select]」,
        // 无 TTY 时读到 EOF 直接 Cancelled——必须喂 Y 走全启用。
        run_hermes_with_stdin(&refs, "Y\n")
    }
    fn mcp_remove(&self, name: &str) -> Result<String, String> {
        run_hermes(&["mcp", "remove", name])
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

/// 同 run_hermes,但向子进程 stdin 写入应答(供 `mcp add` 的交互确认用)。
fn run_hermes_with_stdin(args: &[&str], stdin_input: &str) -> Result<String, String> {
    use std::io::Write;
    use std::process::Stdio;
    let mut child = Command::new("hermes")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to run hermes: {}", e))?;
    if let Some(mut si) = child.stdin.take() {
        let _ = si.write_all(stdin_input.as_bytes());
    }
    let output = child
        .wait_with_output()
        .map_err(|e| format!("Failed to run hermes: {}", e))?;
    if !output.status.success() {
        return Err(format!("hermes {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// hermes `mcp add` 是 flags 语法(没有 --config <json>):
///   stdio: mcp add <name> --command CMD [--env K=V ...] [--args A1 A2 ...]
///   http:  mcp add <name> --url URL
/// `--args` 是 argparse REMAINDER,必须是最后一个选项;CLI 无自定义 header 通道。
fn hermes_mcp_add_args(name: &str, config_json: &str) -> Result<Vec<String>, String> {
    let cfg: serde_json::Value = serde_json::from_str(config_json)
        .map_err(|e| format!("invalid config json: {}", e))?;
    let mut out: Vec<String> = vec!["mcp".into(), "add".into(), name.into()];
    if let Some(url) = cfg.get("url").and_then(|v| v.as_str()) {
        let has_headers = cfg
            .get("headers")
            .and_then(|h| h.as_object())
            .is_some_and(|h| !h.is_empty());
        if has_headers {
            return Err("hermes CLI does not support custom HTTP headers".into());
        }
        out.push("--url".into());
        out.push(url.into());
        return Ok(out);
    }
    let cmd = cfg
        .get("command")
        .and_then(|v| v.as_str())
        .filter(|c| !c.is_empty())
        .ok_or_else(|| "stdio server has no command".to_string())?;
    out.push("--command".into());
    out.push(cmd.into());
    if let Some(env) = cfg.get("env").and_then(|v| v.as_object()).filter(|e| !e.is_empty()) {
        out.push("--env".into());
        for (k, v) in env {
            out.push(format!("{}={}", k, v.as_str().unwrap_or_default()));
        }
    }
    if let Some(args) = cfg.get("args").and_then(|v| v.as_array()).filter(|a| !a.is_empty()) {
        out.push("--args".into());
        for a in args {
            out.push(a.as_str().unwrap_or_default().to_string());
        }
    }
    Ok(out)
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
    fn mcp_add_args_stdio_puts_args_last() {
        // --args 是 REMAINDER,必须收尾;--env 在其前
        let cfg = r#"{"command":"npx","args":["-y","@modelcontextprotocol/server-everything"],"env":{"K":"V"}}"#;
        let args = hermes_mcp_add_args("clawbox-test", cfg).unwrap();
        assert_eq!(
            args,
            vec![
                "mcp", "add", "clawbox-test", "--command", "npx", "--env", "K=V", "--args", "-y",
                "@modelcontextprotocol/server-everything",
            ]
        );
        // 无 env / 无 args 时不产出对应 flag
        let args = hermes_mcp_add_args("s", r#"{"command":"uvx","args":[],"env":{}}"#).unwrap();
        assert_eq!(args, vec!["mcp", "add", "s", "--command", "uvx"]);
    }

    #[test]
    fn mcp_add_args_http_and_error_cases() {
        let args = hermes_mcp_add_args("web", r#"{"type":"http","url":"https://x/mcp","headers":{}}"#).unwrap();
        assert_eq!(args, vec!["mcp", "add", "web", "--url", "https://x/mcp"]);
        // hermes CLI 无自定义 header 通道 → 明确报错而不是静默丢弃
        let err = hermes_mcp_add_args("web", r#"{"url":"https://x/mcp","headers":{"A":"b"}}"#).unwrap_err();
        assert!(err.contains("header"), "{}", err);
        assert!(hermes_mcp_add_args("s", r#"{"args":[]}"#).is_err());
        assert!(hermes_mcp_add_args("s", "{ nope").is_err());
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
    // Real fixture captured verbatim from `hermes tools list` (v0.11.0).
    // Real fixture captured verbatim from `hermes hooks list` (v0.11.0).
    // Hypothetical fixture (per hermes hooks docs suggestion — actual format
    // unverified on this host since live hermes has no hooks configured).
}
