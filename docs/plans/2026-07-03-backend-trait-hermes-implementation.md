# Backend Trait Abstraction + Hermes Co-Management — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Finish the in-progress openclaw refactor and add Hermes as a second managed backend running in parallel, surfaced through the same UI. MVP covers `gateway` + `cron` only.

**Architecture:** A `Backend` trait with `OpenClawBackend` and `HermesBackend` implementations. Each backend normalises its CLI output into a shared `CronJob` model. Aggregate Tauri commands (`cron_list_all`, `gateway_status_all`, etc.) iterate both backends concurrently and return tagged results. Frontend groups UI by backend with a `backend` field on every action.

**Tech Stack:** Rust 2021 + Tauri v2 + serde + rayon (new dep). Svelte 5 + TypeScript + svelte-check + vitest (new).

---

## Phase A — Scaffold

### Task 1: Add rayon dependency

**Files:**
- Modify: `src-tauri/Cargo.toml`

**Step 1: Add rayon**

```toml
[dependencies]
tauri = { version = "2", features = [] }
tauri-plugin-shell = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
dirs = "5"
time = "0.3"
rayon = "1.10"
```

**Step 2: Verify it builds**

Run: `cd src-tauri && cargo check`
Expected: success, "Compiling rayon".

**Step 3: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "build: add rayon for parallel backend calls"
```

---

## Phase B — Core Types

### Task 2: Create backends module with shared types

**Files:**
- Create: `src-tauri/src/backends/mod.rs`

**Step 1: Create the file with shared types and the trait**

```rust
use serde::Serialize;

#[derive(Serialize, Clone, Debug)]
pub struct CronJob {
    pub id: String,
    pub name: String,
    pub schedule: String,
    pub enabled: bool,
    pub last_run: Option<String>,
    pub next_run: Option<String>,
    pub agent: Option<String>,
    pub message: Option<String>,
    pub raw: serde_json::Value,
}

#[derive(Serialize, Clone, Debug)]
pub struct NewCron {
    pub name: String,
    pub schedule: String,
    pub message: Option<String>,
    pub agent: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct GatewayStatus {
    pub status: String,        // "running" | "stopped"
    pub version: String,
    pub pid: Option<i32>,
}

#[derive(Serialize, Clone, Debug)]
pub struct BackendInfo {
    pub id: String,
    pub display_name: String,
    pub version: String,
    pub installed: bool,
}

pub trait Backend: Send + Sync {
    fn id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    fn version(&self) -> String;
    fn is_installed(&self) -> bool;

    fn gateway_status(&self) -> Result<GatewayStatus, String>;
    fn gateway_start(&self) -> Result<String, String>;
    fn gateway_stop(&self) -> Result<String, String>;

    fn cron_list(&self) -> Result<Vec<CronJob>, String>;
    fn cron_create(&self, params: NewCron) -> Result<String, String>;
    fn cron_remove(&self, id: &str) -> Result<String, String>;
    fn cron_set_enabled(&self, id: &str, enabled: bool) -> Result<String, String>;
    fn cron_run(&self, id: &str) -> Result<String, String>;
}

pub fn backends() -> &'static [Box<dyn Backend>] {
    static INSTANCES: std::sync::OnceLock<Vec<Box<dyn Backend>>> = std::sync::OnceLock::new();
    INSTANCES.get_or_init(|| {
        vec![
            Box::new(super::openclaw::OpenClawBackend),
            Box::new(super::hermes::HermesBackend),
        ]
    }).as_slice()
}

pub fn find_backend(id: &str) -> Option<&'static dyn Backend> {
    backends().iter().find(|b| b.id() == id).map(|b| b.as_ref())
}
```

**Step 2: Create empty submodule files**

Create `src-tauri/src/backends/openclaw.rs`:
```rust
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
```

Create `src-tauri/src/backends/hermes.rs`:
```rust
use super::{Backend, CronJob, GatewayStatus, NewCron};

pub struct HermesBackend;

impl Backend for HermesBackend {
    fn id(&self) -> &'static str { "hermes" }
    fn display_name(&self) -> &'static str { "Hermes" }
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
```

**Step 3: Wire up the module in commands**

Modify `src-tauri/src/commands/mod.rs`:
```rust
pub mod config;
pub mod install;
pub mod logs;
pub mod aggregate;

pub mod backends;
```

Wait — `backends/` is a sibling module of `commands/`, not inside it. Move it: remove `pub mod backends;` from `commands/mod.rs`. Instead, in `src-tauri/src/lib.rs` add `mod backends;` alongside `mod commands;`.

Final `src-tauri/src/lib.rs`:
```rust
mod backends;
mod commands;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            // commands registered in later tasks
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

`commands/mod.rs` (final):
```rust
pub mod aggregate;
pub mod config;
pub mod install;
pub mod logs;
```

**Step 4: Verify it builds (will fail until we register at least one command; use a placeholder)**

Temporarily register `aggregate::placeholder` to verify compilation. After this task, add this aggregate command to make `cargo check` pass:

`src-tauri/src/commands/aggregate.rs`:
```rust
#[tauri::command]
pub fn placeholder() -> &'static str { "ok" }
```

`lib.rs` invoke_handler:
```rust
.invoke_handler(tauri::generate_handler![
    commands::aggregate::placeholder,
])
```

Run: `cd src-tauri && cargo check`
Expected: success.

**Step 5: Commit**

```bash
git add src-tauri/src/backends src-tauri/src/commands/mod.rs src-tauri/src/commands/aggregate.rs src-tauri/src/lib.rs
git commit -m "feat(backends): add Backend trait + CronJob + GatewayStatus types"
```

---

## Phase C — OpenClawBackend

### Task 3: Move shared openclaw helpers to backends/openclaw.rs

**Files:**
- Modify: `src-tauri/src/backends/openclaw.rs`

**Step 1: Add the CLI helpers**

Append at the top of `src-tauri/src/backends/openclaw.rs`:
```rust
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
```

**Step 2: Verify it compiles**

Run: `cd src-tauri && cargo check`
Expected: success (helpers added; trait still unimplemented).

**Step 3: Commit**

```bash
git add src-tauri/src/backends/openclaw.rs
git commit -m "refactor(openclaw): move CLI helpers into backends module"
```

---

### Task 4: Implement OpenClawBackend::version and is_installed

**Files:**
- Modify: `src-tauri/src/backends/openclaw.rs`

**Step 1: Replace the stubs**

```rust
use std::process::Command;
use serde_json::Value;
use super::{Backend, CronJob, GatewayStatus, NewCron};

const GATEWAY_PORT: u16 = 18789;

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

    fn gateway_status(&self) -> Result<GatewayStatus, String> { unimplemented!() }
    fn gateway_start(&self) -> Result<String, String> { unimplemented!() }
    fn gateway_stop(&self) -> Result<String, String> { unimplemented!() }
    fn cron_list(&self) -> Result<Vec<CronJob>, String> { unimplemented!() }
    fn cron_create(&self, _params: NewCron) -> Result<String, String> { unimplemented!() }
    fn cron_remove(&self, _id: &str) -> Result<String, String> { unimplemented!() }
    fn cron_set_enabled(&self, _id: &str, _enabled: bool) -> Result<String, String> { unimplemented!() }
    fn cron_run(&self, _id: &str) -> Result<String, String> { unimplemented!() }
}
```

**Step 2: Build**

Run: `cd src-tauri && cargo check`
Expected: success.

**Step 3: Commit**

```bash
git add src-tauri/src/backends/openclaw.rs
git commit -m "feat(openclaw): implement version + is_installed"
```

---

### Task 5: Implement OpenClawBackend::gateway_*

**Files:**
- Modify: `src-tauri/src/backends/openclaw.rs`

**Step 1: Add helper and methods**

Replace the `unimplemented!()` gateway stubs with:

```rust
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
```

Add the helper function (module-level, above the impl):
```rust
fn gateway_pid() -> Option<i32> {
    Command::new("lsof")
        .args(["-t", "-i", &format!(":{}", GATEWAY_PORT), "-sTCP:LISTEN"])
        .output().ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8_lossy(&o.stdout).lines().next()
            .and_then(|l| l.trim().parse::<i32>().ok()))
}
```

**Step 2: Build**

Run: `cd src-tauri && cargo check`
Expected: success.

**Step 3: Commit**

```bash
git add src-tauri/src/backends/openclaw.rs
git commit -m "feat(openclaw): implement gateway lifecycle"
```

---

### Task 6: Implement OpenClawBackend::cron_list with normaliser

**Files:**
- Modify: `src-tauri/src/backends/openclaw.rs`

**Step 1: Write the failing unit test**

Append to `src-tauri/src/backends/openclaw.rs`:
```rust
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
}
```

Add the function under test (above `mod tests`):
```rust
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
```

**Step 2: Run tests; they fail**

Run: `cd src-tauri && cargo test normalise_openclaw_job -- --nocapture`
Expected: failure (function not yet implemented).

**Step 3: Implement the method on the trait**

Replace `fn cron_list(&self) -> Result<Vec<CronJob>, String> { unimplemented!() }` with:
```rust
    fn cron_list(&self) -> Result<Vec<CronJob>, String> {
        let raw = openclaw_json(&["cron", "list", "--json"])?;
        let arr = match raw {
            serde_json::Value::Object(ref m) if m.contains_key("jobs") => m["jobs"].clone(),
            other => other,
        };
        let arr = arr.as_array().cloned().unwrap_or_default();
        Ok(arr.into_iter().map(normalise_openclaw_job).collect())
    }
```

**Step 4: Run tests; pass**

Run: `cd src-tauri && cargo test openclaw -- --nocapture`
Expected: 3 tests pass.

**Step 5: Commit**

```bash
git add src-tauri/src/backends/openclaw.rs
git commit -m "feat(openclaw): implement cron_list with normaliser (TDD)"
```

---

### Task 7: Implement OpenClawBackend::cron_create/remove/set_enabled/run

**Files:**
- Modify: `src-tauri/src/backends/openclaw.rs`

**Step 1: Write tests for set_enabled arg mapping**

Append to `mod tests`:
```rust
#[test]
fn openclaw_create_args() {
    let args = openclaw_create_args(&NewCron {
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
    let args = openclaw_create_args(&NewCron {
        name: "tick".into(), schedule: "30m".into(), message: None, agent: None,
    });
    assert!(args.contains(&"--every".to_string()));
    assert!(args.contains(&"30m".to_string()));
    assert!(!args.contains(&"--cron".to_string()));
}
```

Add the helper:
```rust
fn openclaw_create_args(params: &NewCron) -> Vec<String> {
    let mut args = vec!["cron".into(), "add".into(), "--json".into(), "--name".into(), params.name.clone()];
    // prefer "schedule" interpreted as cron expression; if it contains non-cron chars (e.g. "30m"),
    // treat as interval.
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
```

**Step 2: Run tests; they fail**

Run: `cd src-tauri && cargo test openclaw_create_args`
Expected: failure (function not defined).

**Step 3: Implement trait methods**

Replace the remaining `unimplemented!()` cron stubs with:
```rust
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
```

**Step 4: Run tests; pass**

Run: `cd src-tauri && cargo test`
Expected: all openclaw tests pass.

**Step 5: Commit**

```bash
git add src-tauri/src/backends/openclaw.rs
git commit -m "feat(openclaw): implement cron create/remove/enable/run (TDD)"
```

---

## Phase D — HermesBackend

### Task 8: Implement HermesBackend::version and is_installed

**Files:**
- Modify: `src-tauri/src/backends/hermes.rs`

**Step 1: Replace version/is_installed stubs**

```rust
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
    fn cron_list(&self) -> Result<Vec<CronJob>, String> { unimplemented!() }
    fn cron_create(&self, _params: NewCron) -> Result<String, String> { unimplemented!() }
    fn cron_remove(&self, _id: &str) -> Result<String, String> { unimplemented!() }
    fn cron_set_enabled(&self, _id: &str, _enabled: bool) -> Result<String, String> { unimplemented!() }
    fn cron_run(&self, _id: &str) -> Result<String, String> { unimplemented!() }
}
```

**Step 2: Build**

Run: `cd src-tauri && cargo check`
Expected: success.

**Step 3: Commit**

```bash
git add src-tauri/src/backends/hermes.rs
git commit -m "feat(hermes): implement version + is_installed"
```

---

### Task 9: Implement parse_hermes_cron_text with TDD

**Files:**
- Modify: `src-tauri/src/backends/hermes.rs`

**Step 1: Write tests**

Append at the end of `src-tauri/src/backends/hermes.rs`:
```rust
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
```

**Step 2: Run; fail**

Run: `cd src-tauri && cargo test parse_hermes_cron_text`
Expected: failure (function not defined).

**Step 3: Implement parse_hermes_cron_text**

Add above the `mod tests`:
```rust
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
            // End-of-record delimiter or unrecognised line: flush.
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
```

**Step 4: Run tests; pass**

Run: `cd src-tauri && cargo test parse_hermes_cron_text`
Expected: 4 tests pass.

**Step 5: Commit**

```bash
git add src-tauri/src/backends/hermes.rs
git commit -m "feat(hermes): implement parse_hermes_cron_text (TDD)"
```

---

### Task 10: Implement HermesBackend::cron_list

**Files:**
- Modify: `src-tauri/src/backends/hermes.rs`

**Step 1: Implement cron_list**

Replace the unimplemented stub:
```rust
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
```

**Step 2: Build**

Run: `cd src-tauri && cargo check`
Expected: success.

**Step 3: Commit**

```bash
git add src-tauri/src/backends/hermes.rs
git commit -m "feat(hermes): implement cron_list"
```

---

### Task 11: Implement HermesBackend::cron_create/remove/set_enabled/run (TDD)

**Files:**
- Modify: `src-tauri/src/backends/hermes.rs`

**Step 1: Write tests for arg mapping**

Append to `mod tests`:
```rust
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
```

Add helper functions:
```rust
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
```

**Step 2: Run; fail**

Run: `cd src-tauri && cargo test hermes_`
Expected: failure (helpers not defined).

**Step 3: Implement trait methods**

Replace remaining `unimplemented!()` cron stubs:
```rust
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
```

Add `run_hermes` helper above the trait impl:
```rust
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
```

**Step 4: Run tests; pass**

Run: `cd src-tauri && cargo test`
Expected: all hermes tests pass.

**Step 5: Commit**

```bash
git add src-tauri/src/backends/hermes.rs
git commit -m "feat(hermes): implement cron create/remove/enable/run (TDD)"
```

---

### Task 12: Implement HermesBackend::gateway_*

**Files:**
- Modify: `src-tauri/src/backends/hermes.rs`

**Step 1: Implement gateway methods**

Replace the gateway stubs:
```rust
    fn gateway_status(&self) -> Result<GatewayStatus, String> {
        let text = run_hermes(&["gateway", "status"]).unwrap_or_default();
        // Hermes plist output contains "LastExitStatus" and "PID". Treat
        // presence of "PID" line as running.
        let running = text.lines().any(|l| l.trim().starts_with("PID") && l.contains("="));
        Ok(GatewayStatus {
            status: if running { "running" } else { "stopped" }.into(),
            version: self.version(),
            pid: extract_pid(&text),
        })
    }

    fn gateway_start(&self) -> Result<String, String> {
        run_hermes(&["gateway", "start"])
    }

    fn gateway_stop(&self) -> Result<String, String> {
        run_hermes(&["gateway", "stop"])
    }
```

Add helpers above `mod tests`:
```rust
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
```

**Step 2: Add tests**

Append to `mod tests`:
```rust
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
```

**Step 3: Run tests**

Run: `cd src-tauri && cargo test hermes`
Expected: all hermes tests pass.

**Step 4: Commit**

```bash
git add src-tauri/src/backends/hermes.rs
git commit -m "feat(hermes): implement gateway lifecycle"
```

---

## Phase E — Aggregate Commands

### Task 13: Implement aggregate commands

**Files:**
- Modify: `src-tauri/src/commands/aggregate.rs`

**Step 1: Replace the placeholder with full implementation**

```rust
use rayon::prelude::*;
use serde::Serialize;

use crate::backends::{self, BackendInfo, Backend, CronJob, NewCron};

#[derive(Serialize)]
pub struct TaggedCronJob {
    pub backend: String,
    pub job: CronJob,
}

#[derive(Serialize)]
pub struct BackendError {
    pub backend: String,
    pub message: String,
}

#[derive(Serialize)]
pub struct CronListAllResult {
    pub jobs: Vec<TaggedCronJob>,
    pub errors: Vec<BackendError>,
}

fn collect_backends<F, T>(f: F) -> (Vec<(String, T)>, Vec<BackendError>)
where
    F: Fn(&dyn Backend) -> Result<T, String> + Sync + Send,
    T: Send,
{
    let results: Vec<_> = backends::backends().par_iter()
        .map(|b| {
            if !b.is_installed() {
                return (b.id().to_string(), None, None);
            }
            match f(b.as_ref()) {
                Ok(v) => (b.id().to_string(), Some(v), None),
                Err(e) => (b.id().to_string(), None, Some(BackendError {
                    backend: b.id().to_string(), message: e,
                })),
            }
        }).collect();

    let mut values = Vec::new();
    let mut errors = Vec::new();
    for (id, val, err) in results {
        if let Some(e) = err { errors.push(e); }
        if let Some(v) = val { values.push((id, v)); }
    }
    (values, errors)
}

#[tauri::command]
pub fn list_backends() -> Vec<BackendInfo> {
    backends::backends().iter().map(|b| BackendInfo {
        id: b.id().to_string(),
        display_name: b.display_name().to_string(),
        version: b.version(),
        installed: b.is_installed(),
    }).collect()
}

#[tauri::command]
pub fn gateway_status_all() -> Vec<crate::backends::GatewayStatus> {
    let (pairs, _errors) = collect_backends(|b| b.gateway_status());
    // For now, errors here are silenced; per-backend status will be
    // re-checked from UI when "not installed" appears.
    pairs.into_iter().map(|(_, s)| s).collect()
}

#[tauri::command]
pub fn gateway_start(backend: String) -> Result<String, String> {
    backends::find_backend(&backend)
        .ok_or_else(|| format!("Unknown backend: {}", backend))?
        .gateway_start()
}

#[tauri::command]
pub fn gateway_stop(backend: String) -> Result<String, String> {
    backends::find_backend(&backend)
        .ok_or_else(|| format!("Unknown backend: {}", backend))?
        .gateway_stop()
}

#[tauri::command]
pub fn cron_list_all() -> CronListAllResult {
    let (pairs, errors) = collect_backends(|b| b.cron_list());
    let jobs = pairs.into_iter()
        .flat_map(|(id, js)| js.into_iter().map(move |j| TaggedCronJob {
            backend: id.clone(), job: j,
        }))
        .collect();
    CronListAllResult { jobs, errors }
}

#[tauri::command]
pub fn cron_create(backend: String, params: NewCron) -> Result<String, String> {
    backends::find_backend(&backend)
        .ok_or_else(|| format!("Unknown backend: {}", backend))?
        .cron_create(params)
}

#[tauri::command]
pub fn cron_remove(backend: String, id: String) -> Result<String, String> {
    backends::find_backend(&backend)
        .ok_or_else(|| format!("Unknown backend: {}", backend))?
        .cron_remove(&id)
}

#[tauri::command]
pub fn cron_set_enabled(backend: String, id: String, enabled: bool) -> Result<String, String> {
    backends::find_backend(&backend)
        .ok_or_else(|| format!("Unknown backend: {}", backend))?
        .cron_set_enabled(&id, enabled)
}

#[tauri::command]
pub fn cron_run(backend: String, id: String) -> Result<String, String> {
    backends::find_backend(&backend)
        .ok_or_else(|| format!("Unknown backend: {}", backend))?
        .cron_run(&id)
}
```

**Step 2: Register in lib.rs**

Replace `src-tauri/src/lib.rs`:
```rust
mod backends;
mod commands;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            commands::aggregate::list_backends,
            commands::aggregate::gateway_status_all,
            commands::aggregate::gateway_start,
            commands::aggregate::gateway_stop,
            commands::aggregate::cron_list_all,
            commands::aggregate::cron_create,
            commands::aggregate::cron_remove,
            commands::aggregate::cron_set_enabled,
            commands::aggregate::cron_run,
            commands::config::get_config,
            commands::config::set_config,
            commands::install::check_system,
            commands::install::install_openclaw,
            commands::install::check_update,
            commands::install::check_openclaw_update,
            commands::logs::get_log_files,
            commands::logs::get_log_content,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

**Step 3: Verify build**

Run: `cd src-tauri && cargo check`
Expected: success.

**Step 4: Commit**

```bash
git add src-tauri/src/commands/aggregate.rs src-tauri/src/lib.rs
git commit -m "feat(aggregate): expose tagged list + per-backend actions"
```

---

## Phase F — Remove WIP Files

### Task 14: Delete obsolete WIP command files

**Files:**
- Delete: `src-tauri/src/commands/cron.rs`
- Delete: `src-tauri/src/commands/stats.rs`
- Delete: `src-tauri/src/commands/openclaw.rs`
- Delete: `src-tauri/src/commands/gateway.rs`

**Step 1: Delete the files**

```bash
rm src-tauri/src/commands/cron.rs \
   src-tauri/src/commands/stats.rs \
   src-tauri/src/commands/openclaw.rs \
   src-tauri/src/commands/gateway.rs
```

**Step 2: Verify build**

Run: `cd src-tauri && cargo check`
Expected: success (lib.rs no longer references them).

**Step 3: Commit**

```bash
git rm src-tauri/src/commands/cron.rs \
      src-tauri/src/commands/stats.rs \
      src-tauri/src/commands/openclaw.rs \
      src-tauri/src/commands/gateway.rs
git commit -m "refactor: drop obsolete per-CLI command files (absorbed into backends/)"
```

---

## Phase G — Frontend API

### Task 15: Add backends.ts

**Files:**
- Create: `src/lib/api/backends.ts`

**Step 1: Write the file**

```typescript
import { invoke } from '@tauri-apps/api/core';

export type BackendId = 'openclaw' | 'hermes';

export interface BackendInfo {
  id: BackendId;
  displayName: string;
  version: string;
  installed: boolean;
}

export async function list_backends(): Promise<BackendInfo[]> {
  try {
    const raw = await invoke<BackendInfo[]>('list_backends');
    return raw.map((b) => ({
      id: b.id as BackendId,
      displayName: b.displayName,
      version: b.version,
      installed: b.installed,
    }));
  } catch {
    return [];
  }
}
```

**Step 2: Commit**

```bash
git add src/lib/api/backends.ts
git commit -m "feat(api): add backends.ts"
```

---

### Task 16: Rewrite cron.ts

**Files:**
- Modify: `src/lib/api/cron.ts` (replace contents)

**Step 1: Replace with tagged shape**

```typescript
import { invoke } from '@tauri-apps/api/core';
import type { BackendId } from './backends';

export interface CronJob {
  id: string;
  name: string;
  schedule: string;
  enabled: boolean;
  lastRun?: string;
  nextRun?: string;
  agent?: string;
  message?: string;
  [key: string]: unknown;
}

export interface TaggedCronJob {
  backend: BackendId;
  job: CronJob;
}

export interface BackendError {
  backend: BackendId;
  message: string;
}

export interface CronListAllResult {
  jobs: TaggedCronJob[];
  errors: BackendError[];
}

export interface NewCron {
  name: string;
  schedule: string;
  message?: string;
  agent?: string;
}

export async function list_cron_all(): Promise<CronListAllResult> {
  try {
    return await invoke<CronListAllResult>('cron_list_all');
  } catch {
    return { jobs: [], errors: [] };
  }
}

export async function add_cron(backend: BackendId, params: NewCron): Promise<void> {
  await invoke('cron_create', { backend, params });
}

export async function remove_cron(backend: BackendId, id: string): Promise<void> {
  await invoke('cron_remove', { backend, id });
}

export async function set_cron_enabled(backend: BackendId, id: string, enabled: boolean): Promise<void> {
  await invoke('cron_set_enabled', { backend, id, enabled });
}

export async function run_cron(backend: BackendId, id: string): Promise<void> {
  await invoke('cron_run', { backend, id });
}
```

**Step 2: Commit**

```bash
git add src/lib/api/cron.ts
git commit -m "refactor(api): rewrite cron.ts for tagged backend shape"
```

---

### Task 17: Rewrite gateway.ts

**Files:**
- Modify: `src/lib/api/gateway.ts` (replace contents)

**Step 1: Replace contents**

```typescript
import { invoke } from '@tauri-apps/api/core';
import type { BackendId } from './backends';

export interface GatewayStatus {
  status: 'running' | 'stopped';
  version: string;
  pid?: number;
}

export interface TaggedGatewayStatus {
  backend: BackendId;
  status: GatewayStatus;
}

export async function list_gateway_statuses(): Promise<TaggedGatewayStatus[]> {
  try {
    const raw = await invoke<{ backend: string; status: GatewayStatus }[]>('gateway_status_all');
    return raw.map((r) => ({
      backend: r.backend as BackendId,
      status: r.status,
    }));
  } catch {
    return [];
  }
}

export async function start_gateway(backend: BackendId): Promise<string> {
  return await invoke<string>('gateway_start', { backend });
}

export async function stop_gateway(backend: BackendId): Promise<string> {
  return await invoke<string>('gateway_stop', { backend });
}
```

**Step 2: Commit**

```bash
git add src/lib/api/gateway.ts
git commit -m "refactor(api): rewrite gateway.ts for tagged backend shape"
```

---

### Task 18: Trim stats.ts to openclaw-only for MVP

**Files:**
- Modify: `src/lib/api/stats.ts` (note in header; no API change)

**Step 1: Update header doc comment**

Replace the file's leading comment block:
```typescript
// OpenClaw-only stats. Hermes stats (insights) land in a later iteration.
// Mirrors the Rust `Stats` struct.
```

Keep the rest of the file unchanged.

**Step 2: Commit**

```bash
git add src/lib/api/stats.ts
git commit -m "docs(stats): note openclaw-only scope"
```

---

## Phase H — Frontend UI

### Task 19: Update i18n keys

**Files:**
- Modify: `src/lib/i18n/en.json`
- Modify: `src/lib/i18n/zh.json`

**Step 1: Add backend keys**

In `en.json`, add (or merge) into the top-level object:
```json
{
  "backend": {
    "openclaw": "OpenClaw",
    "hermes": "Hermes",
    "notInstalled": "Not installed",
    "section": {
      "jobs": "{{name}} ({{count}} jobs)",
      "noJobs": "{{name}} — no jobs"
    }
  },
  "task": {
    "newBackend": "Backend"
  }
}
```

In `zh.json`:
```json
{
  "backend": {
    "openclaw": "OpenClaw",
    "hermes": "Hermes",
    "notInstalled": "未安装",
    "section": {
      "jobs": "{{name}}（{{count}} 个任务）",
      "noJobs": "{{name}} — 无任务"
    }
  },
  "task": {
    "newBackend": "后端"
  }
}
```

**Step 2: Verify Svelte parsing**

Run: `npm run check`
Expected: success (or only unrelated warnings).

**Step 3: Commit**

```bash
git add src/lib/i18n/en.json src/lib/i18n/zh.json
git commit -m "i18n: add backend labels"
```

---

### Task 20: Refactor tasks/+page.svelte for backend grouping

**Files:**
- Modify: `src/routes/tasks/+page.svelte`

**Step 1: Replace imports**

Replace the import block (current imports for `list_cron`, `add_cron`, etc.) with:
```typescript
import {
  list_cron_all, add_cron, remove_cron, set_cron_enabled, run_cron,
  type TaggedCronJob, type CronJob, type NewCron,
} from '$lib/api/cron';
import { list_backends, type BackendInfo } from '$lib/api/backends';
```

**Step 2: Add backend state and grouping**

After existing `let formError`, add:
```typescript
let backends = $state<BackendInfo[]>([]);
let grouped = $state<Record<string, TaggedCronJob[]>>({});
let errors = $state<{ backend: string; message: string }[]>([]);

async function refresh() {
  const [bl, cl] = await Promise.all([list_backends(), list_cron_all()]);
  backends = bl;
  errors = cl.errors;
  const map: Record<string, TaggedCronJob[]> = {};
  for (const b of bl) map[b.id] = [];
  for (const t of cl.jobs) {
    (map[t.backend] ??= []).push(t);
  }
  grouped = map;
}
```

Replace the existing `load()` (or whatever init function fetches data) to call `refresh()`.

**Step 3: Update create form**

Replace the create-job form to include a backend selector. Find the existing `<form>` block and modify:

Inside the form, before the existing `task-cron` input, add:
```svelte
<label for="task-backend">{$_('task.newBackend')}</label>
<select id="task-backend" bind:value={newBackend}>
  {#each backends.filter(b => b.installed) as b}
    <option value={b.id}>{b.displayName}</option>
  {/each}
</select>
```

Add state: `let newBackend = $state<BackendInfo['id']>('openclaw');`

In the submit handler, replace `add_cron({...})` with:
```typescript
const params: NewCron = {
  name: newName.trim(),
  schedule: newCron.trim(),
  message: newMessage.trim() || undefined,
  agent: newAgent.trim() || undefined,
};
await add_cron(newBackend, params);
```

**Step 4: Update per-row action handlers**

Each row's `remove`, `toggle`, `run` calls need to pass `t.backend`:

```svelte
<button onclick={() => remove_cron(t.backend, t.job.id)}>{$_('tasks.remove')}</button>
<button onclick={() => set_cron_enabled(t.backend, t.job.id, !t.job.enabled)}>...</button>
<button onclick={() => run_cron(t.backend, t.job.id)}>{$_('tasks.run')}</button>
```

**Step 5: Update list rendering to group by backend**

Wrap the existing list with grouped sections. Find the `{#each ... as t}` rendering loop and replace with:
```svelte
{#each backends as backend}
  <section class="backend-section">
    <header>
      <span class="backend-chip" data-backend={backend.id}>{backend.displayName}</span>
      {#if backend.installed}
        <span>{$_('backend.section.jobs', { values: { name: backend.displayName, count: grouped[backend.id]?.length ?? 0 } })}</span>
      {:else}
        <span class="empty">{$_('backend.notInstalled')}</span>
      {/if}
    </header>
    {#if backend.installed}
      {#each grouped[backend.id] ?? [] as t (t.backend + ':' + t.job.id)}
        ... existing row markup, but every `task` reference becomes `t.job`, every action uses `t.backend` ...
      {/each}
    {/if}
  </section>
{/each}
```

(Adapt field names — existing markup likely reads `task.name`, `task.schedule` etc.; change each to `t.job.name`, `t.job.schedule`.)

**Step 6: Run svelte-check**

Run: `npm run check`
Expected: success (or only unrelated warnings).

**Step 7: Commit**

```bash
git add src/routes/tasks/+page.svelte
git commit -m "feat(ui): group Tasks by backend"
```

---

### Task 21: Refactor monitor/+page.svelte for backend cards

**Files:**
- Modify: `src/routes/monitor/+page.svelte`

**Step 1: Update imports**

Replace the existing `get_stats, extractMetrics` import with:
```typescript
import { list_gateway_statuses, start_gateway, stop_gateway } from '$lib/api/gateway';
import { list_backends, type BackendInfo } from '$lib/api/backends';
```

**Step 2: Replace state**

Replace `gatewayRunning`, `rawHealth`, etc. with:
```typescript
let backends = $state<BackendInfo[]>([]);
let statuses = $state<Record<string, { status: 'running' | 'stopped'; version: string; pid?: number }>>({});

async function refresh() {
  const [bl, gs] = await Promise.all([list_backends(), list_gateway_statuses()]);
  backends = bl;
  const map: typeof statuses = {};
  for (const s of gs) map[s.backend] = s.status;
  statuses = map;
}
```

**Step 3: Render one gateway card per installed backend**

Replace the existing single gateway card with:
```svelte
<div class="gateway-grid">
  {#each backends.filter(b => b.installed) as b (b.id)}
    <div class="gateway-card glass-card">
      <header>
        <span class="backend-chip" data-backend={b.id}>{b.displayName}</span>
        <span class="version">v{b.version}</span>
      </header>
      <div class="status">{statuses[b.id]?.status ?? 'unknown'}</div>
      {#if statuses[b.id]?.status === 'running'}
        <button onclick={() => stop_gateway(b.id)}>Stop</button>
      {:else}
        <button onclick={() => start_gateway(b.id)}>Start</button>
      {/if}
    </div>
  {/each}
</div>
```

(Keep any unrelated parts of the page — usage stats, logs links — as-is.)

**Step 4: svelte-check**

Run: `npm run check`
Expected: success.

**Step 5: Commit**

```bash
git add src/routes/monitor/+page.svelte
git commit -m "feat(ui): render gateway cards per backend"
```

---

### Task 22: Refactor App.svelte dashboard stats cards

**Files:**
- Modify: `src/App.svelte`

**Step 1: Update imports**

Replace:
```typescript
import { get_stats, extractMetrics, type DashboardMetrics } from './lib/api/stats';
```
with:
```typescript
import { get_stats, extractMetrics, type DashboardMetrics } from './lib/api/stats';
import { list_gateway_statuses } from './lib/api/gateway';
import { list_backends, type BackendInfo } from './lib/api/backends';
```

**Step 2: Add backend-aware state**

After existing `let metrics`, add:
```typescript
let backends = $state<BackendInfo[]>([]);
let gatewayByBackend = $state<Record<string, { status: string; version: string }>>({});

async function refreshBackends() {
  const [bl, gs] = await Promise.all([list_backends(), list_gateway_statuses()]);
  backends = bl;
  const map: typeof gatewayByBackend = {};
  for (const s of gs) map[s.backend] = s.status;
  gatewayByBackend = map;
}
```

Call `refreshBackends()` inside the existing `onMount` / load function alongside the existing `get_stats` call.

**Step 3: Render one gateway card per installed backend**

Find the existing single gateway status section in the dashboard markup and replace it with:
```svelte
<div class="gateway-row">
  {#each backends.filter(b => b.installed) as b (b.id)}
    <div class="gateway-card">
      <span class="backend-chip" data-backend={b.id}>{b.displayName}</span>
      <span class="version">{b.version}</span>
      <span class="status" class:running={gatewayByBackend[b.id]?.status === 'running'}>
        {gatewayByBackend[b.id]?.status ?? 'unknown'}
      </span>
    </div>
  {/each}
</div>
```

**Step 4: svelte-check**

Run: `npm run check`
Expected: success.

**Step 5: Commit**

```bash
git add src/App.svelte
git commit -m "feat(ui): dashboard gateway card per backend"
```

---

## Phase I — Verify

### Task 23: Run full verification

**Step 1: Rust tests + check**

```bash
cd src-tauri && cargo test && cargo check
```
Expected: all tests pass.

**Step 2: Frontend type-check**

```bash
npm run check
```
Expected: success.

**Step 3: Boot the app**

```bash
npm run tauri dev
```

Manually verify (per design doc):
1. Dashboard renders one card per installed backend.
2. Tasks page: OpenClaw section shows existing jobs; Hermes section is empty (or shows real jobs).
3. Create a Hermes cron from the UI; reload; it persists.
4. Pause a Hermes cron; UI shows it disabled.
5. Move `hermes` binary aside; restart ClawBox; Hermes section shows "not installed"; OpenClaw section unaffected.
6. Stop OpenClaw gateway; OpenClaw status flips to `stopped`; Hermes card unaffected.

**Step 4: Commit any final fixes**

If manual verification surfaced issues, fix and commit individually with descriptive messages.

---

## Notes for the Implementer

- Do NOT change `commands/config.rs` (ClawBox's own config, unrelated to backend trait).
- Do NOT touch `commands/install.rs` — it's openclaw-specific and out of scope.
- `stats` and `config` modules stay openclaw-only for this iteration.
- If `parse_hermes_cron_text` encounters a `hermes cron list` output format that differs from the unit-test fixtures, write a new fixture for it, fail the test, then update the parser. Do not ship untested parser changes.
- Keep commits small and atomic — one task = one commit.