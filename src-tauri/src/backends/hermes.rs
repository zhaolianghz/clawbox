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
}
