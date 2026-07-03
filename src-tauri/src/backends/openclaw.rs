use std::process::Command;
use serde_json::Value;

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

pub struct OpenClawBackend;

impl Backend for OpenClawBackend {
    fn id(&self) -> &'static str { "openclaw" }
    fn display_name(&self) -> &'static str { "OpenClaw" }
    fn version(&self) -> String { "unknown".into() }
    fn is_installed(&self) -> bool { false }
    fn gateway_status(&self) -> Result<GatewayStatus, String> { unimplemented!() }
    fn gateway_start(&self) -> Result<String, String> { unimplemented!() }
    fn gateway_stop(&self) -> Result<String, String> { unimplemented!() }
    fn cron_list(&self) -> Result<Vec<CronJob>, String> { unimplemented!() }
    fn cron_create(&self, _params: NewCron) -> Result<String, String> { unimplemented!() }
    fn cron_remove(&self, _id: &str) -> Result<String, String> { unimplemented!() }
    fn cron_set_enabled(&self, _id: &str, _enabled: bool) -> Result<String, String> { unimplemented!() }
    fn cron_run(&self, _id: &str) -> Result<String, String> { unimplemented!() }
}
