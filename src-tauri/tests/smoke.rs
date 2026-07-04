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
