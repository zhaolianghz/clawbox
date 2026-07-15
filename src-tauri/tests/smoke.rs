// Integration smoke test: discover both backends on the host and exercise
// their detect/cron-list gateway-status paths against the actual binaries.
// Skipped automatically if either CLI is missing so CI without both backends
// still passes.

use clawbox_lib::backends;

fn openclaw_installed() -> bool {
    std::process::Command::new("openclaw")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn hermes_installed() -> bool {
    std::process::Command::new("hermes")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[test]
fn openclaw_is_discoverable() {
    if !openclaw_installed() {
        eprintln!("openclaw missing — skipping");
        return;
    }
    let bs = backends::backends();
    let oc = bs.iter().find(|b| b.id() == "openclaw").expect("openclaw backend registered");
    assert!(oc.is_installed());
    let v = oc.version();
    assert!(v.contains("OpenClaw"), "expected version to mention OpenClaw, got {v:?}");
}

#[test]
fn hermes_is_discoverable() {
    if !hermes_installed() {
        eprintln!("hermes missing — skipping");
        return;
    }
    let bs = backends::backends();
    let h = bs.iter().find(|b| b.id() == "hermes").expect("hermes backend registered");
    assert!(h.is_installed());
}

#[test]
fn hermes_cron_list_parses_real_output() {
    if !hermes_installed() {
        eprintln!("hermes missing — skipping");
        return;
    }
    let bs = backends::backends();
    let h = bs.iter().find(|b| b.id() == "hermes").unwrap();
    let jobs = h.cron_list().expect("hermes cron list should run");
    // Empty state is fine; we just need the call to round-trip a parsed shape.
    for j in &jobs {
        assert!(!j.id.is_empty(), "parsed job id should not be empty");
        assert!(!j.name.is_empty(), "parsed job name should not be empty");
    }
}

#[test]
fn skills_list_runs_against_live_backends() {
    if !openclaw_installed() && !hermes_installed() {
        eprintln!("neither backend installed — skipping");
        return;
    }
    let entries = clawbox_lib::backends::entries();
    let mut found_any = false;
    for entry in entries {
        if !entry.backend.is_installed() { continue; }
        if let Some(skills) = entry.skills {
            found_any = true;
            let result = skills.skills_list();
            // Either Ok or Err depending on gateway state; we just verify the trait method is reachable.
            eprintln!("{}: skills_list reachable, ok={}", entry.backend.id(), result.is_ok());
        }
    }
    assert!(found_any, "expected at least one installed backend with skills capability");
}

#[test]
fn mcp_list_runs_against_live_backends() {
    if !openclaw_installed() && !hermes_installed() {
        eprintln!("skip");
        return;
    }
    let entries = clawbox_lib::backends::entries();
    let mut found = false;
    for entry in entries {
        if !entry.backend.is_installed() { continue; }
        if let Some(mcp) = entry.mcp {
            found = true;
            let r = mcp.mcp_list();
            eprintln!("{}: mcp_list reachable, ok={}", entry.backend.id(), r.is_ok());
        }
    }
    assert!(found);
}

#[test]
fn memory_status_runs_against_live_backends() {
    if !openclaw_installed() && !hermes_installed() {
        eprintln!("skip");
        return;
    }
    let entries = clawbox_lib::backends::entries();
    let mut found = false;
    for entry in entries {
        if !entry.backend.is_installed() { continue; }
        if let Some(mem) = entry.memory {
            found = true;
            let r = mem.memory_status();
            eprintln!("{}: memory_status reachable, ok={}", entry.backend.id(), r.is_ok());
        }
    }
    assert!(found);
}

#[test]
fn plugins_list_runs_against_live_backends() {
    if !openclaw_installed() && !hermes_installed() {
        eprintln!("skip");
        return;
    }
    let entries = clawbox_lib::backends::entries();
    let mut found = false;
    for entry in entries {
        if !entry.backend.is_installed() { continue; }
        if let Some(plugins) = entry.plugins {
            found = true;
            let r = plugins.plugins_list();
            eprintln!("{}: plugins_list reachable, ok={}", entry.backend.id(), r.is_ok());
        }
    }
    assert!(found);
}

#[test]
fn tools_list_only_hermes() {
    let entries = clawbox_lib::backends::entries();
    let hermes_entry = entries.iter().find(|e| e.backend.id() == "hermes").unwrap();
    let openclaw_entry = entries.iter().find(|e| e.backend.id() == "openclaw").unwrap();
    assert!(hermes_entry.tools.is_some(), "hermes should impl ToolsCapability");
    assert!(openclaw_entry.tools.is_none(), "openclaw has no tools subcommand");
}

// Live ACP smoke: drives the real claude-agent-acp bridge end-to-end
// (spawn -> initialize -> session/new -> session/prompt). Skipped when the
// bridge binary is missing. Cold start can take ~40-90s on first run.
#[tokio::test]
async fn acp_claude_handshake_and_prompt() {
    use clawbox_lib::acp::adapters::find_adapter;
    use clawbox_lib::acp::permission::PermissionPolicy;
    use clawbox_lib::acp::session::AcpSession;

    let adapter = find_adapter("claude-agent-acp").unwrap();
    if !adapter.is_installed() {
        eprintln!("skip: claude-agent-acp not installed");
        return;
    }

    let cwd = std::env::temp_dir();
    let session = AcpSession::start("claude-agent-acp", &cwd, PermissionPolicy::ReadOnly)
        .await
        .expect("session start");

    let result = session
        .prompt("Reply with exactly the word: pong")
        .await
        .expect("prompt");

    eprintln!("stop_reason={:?} reply={:?}", result.stop_reason, result.text);
    assert!(!result.stop_reason.is_empty());
    assert!(
        result.text.to_lowercase().contains("pong"),
        "expected 'pong' in reply, got: {}",
        result.text
    );
}

#[test]
fn hooks_list_runs_against_live_backends() {
    if !openclaw_installed() && !hermes_installed() {
        eprintln!("skip");
        return;
    }
    let entries = clawbox_lib::backends::entries();
    let mut found = false;
    for entry in entries {
        if !entry.backend.is_installed() { continue; }
        if let Some(hooks) = entry.hooks {
            found = true;
            let r = hooks.hooks_list();
            eprintln!("{}: hooks_list reachable, ok={}", entry.backend.id(), r.is_ok());
        }
    }
    assert!(found);
}
