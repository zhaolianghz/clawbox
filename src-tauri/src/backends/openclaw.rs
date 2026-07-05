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

fn gateway_pid() -> Option<i32> {
    Command::new("lsof")
        .args(["-t", "-i", &format!(":{}", GATEWAY_PORT), "-sTCP:LISTEN"])
        .output().ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8_lossy(&o.stdout).lines().next()
            .and_then(|l| l.trim().parse::<i32>().ok()))
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
}
