use std::process::Command;

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
    fn gateway_status(&self) -> Result<GatewayStatus, String> { unimplemented!() }
    fn gateway_start(&self) -> Result<String, String> { unimplemented!() }
    fn gateway_stop(&self) -> Result<String, String> { unimplemented!() }
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
    fn cron_create(&self, _params: NewCron) -> Result<String, String> { unimplemented!() }
    fn cron_remove(&self, _id: &str) -> Result<String, String> { unimplemented!() }
    fn cron_set_enabled(&self, _id: &str, _enabled: bool) -> Result<String, String> { unimplemented!() }
    fn cron_run(&self, _id: &str) -> Result<String, String> { unimplemented!() }
}

fn parse_hermes_cron_text(text: &str) -> Vec<CronJob> {
    let trimmed = text.trim();
    if trimmed.is_empty()
        || trimmed.contains("No scheduled jobs")
        || trimmed.starts_with("Create one with")
    {
        return vec![];
    }

    let mut jobs = Vec::new();
    let mut current: Option<std::collections::BTreeMap<String, String>> = None;

    for raw_line in text.lines() {
        let line = raw_line.trim_end();
        if line.trim().is_empty() { continue; }
        if line.starts_with("---") {
            if let Some(map) = current.take() {
                jobs.push(map_to_job(map));
            }
            continue;
        }
        if let Some((k, v)) = line.split_once(':') {
            let key = k.trim().to_string();
            let val = v.trim().to_string();
            current.get_or_insert_with(Default::default).insert(key, val);
        } else if let Some(map) = current.take() {
            jobs.push(map_to_job(map));
        }
    }
    if let Some(map) = current.take() {
        jobs.push(map_to_job(map));
    }
    jobs
}

fn map_to_job(map: std::collections::BTreeMap<String, String>) -> CronJob {
    let id = map.get("job_id").cloned().unwrap_or_default();
    let name = map.get("name").cloned().unwrap_or_else(|| id.clone());
    let schedule = map.get("schedule").cloned().unwrap_or_default();
    let enabled = map.get("enabled").map(|s| s == "true").unwrap_or(true);
    let last_run = map.get("last_run").cloned();
    let next_run = map.get("next_run").cloned();
    let message = map.get("prompt").cloned();
    let raw = serde_json::to_value(&map).unwrap_or(serde_json::Value::Null);
    CronJob { id, name, schedule, enabled, last_run, next_run, agent: None, message, raw }
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

    #[test]
    fn parses_single_job() {
        let text = "\
job_id: abc123
name:    nightly
schedule: 0 2 * * *
enabled:  true
next_run: 2026-07-04T02:00:00Z
prompt:  Run the nightly report
";
        let jobs = parse_hermes_cron_text(text);
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].id, "abc123");
        assert_eq!(jobs[0].name, "nightly");
        assert_eq!(jobs[0].schedule, "0 2 * * *");
        assert!(jobs[0].enabled);
        assert_eq!(jobs[0].next_run.as_deref(), Some("2026-07-04T02:00:00Z"));
        assert_eq!(jobs[0].message.as_deref(), Some("Run the nightly report"));
    }

    #[test]
    fn parses_multiple_jobs() {
        let text = "\
job_id: a
schedule: 30m
enabled:  true
---
job_id: b
schedule: every 2h
enabled:  false
";
        let jobs = parse_hermes_cron_text(text);
        assert_eq!(jobs.len(), 2);
        assert_eq!(jobs[0].id, "a");
        assert!(jobs[0].enabled);
        assert_eq!(jobs[1].id, "b");
        assert!(!jobs[1].enabled);
    }

    #[test]
    fn parses_with_skill_and_repeat() {
        let text = "\
job_id: c
schedule: 0 9 * * *
enabled:  true
repeat:   5
skills:   [\"web-search\", \"summarize\"]
";
        let jobs = parse_hermes_cron_text(text);
        assert_eq!(jobs.len(), 1);
        assert!(jobs[0].raw.get("repeat").is_some());
        assert!(jobs[0].raw.get("skills").is_some());
    }
}
