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

// ---- unified agent registry smoke (real binaries on this host) ----

#[test]
fn agent_registry_detection_is_consistent_with_direct_probes() {
    clawbox_lib::path_env::init();
    let statuses = clawbox_lib::agents::list_agent_status();
    assert_eq!(statuses.len(), 10);

    // Every status must agree with probing the binary directly.
    for s in &statuses {
        let def = clawbox_lib::agents::find_agent(&s.id).unwrap();
        let direct = std::process::Command::new(def.binary)
            .args(def.check_probe)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        assert_eq!(
            s.installed, direct,
            "detection mismatch for {} (status={}, direct={})",
            s.id, s.installed, direct
        );
    }
}

#[test]
fn node_is_detected_when_present() {
    // node is a hard prerequisite of this repo's own toolchain; if the dev
    // machine has it, the registry must see it (PATH fix regression guard).
    clawbox_lib::path_env::init();
    let has_node = std::process::Command::new("node")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !has_node {
        eprintln!("skip: node not on this host");
        return;
    }
    let statuses = clawbox_lib::agents::list_agent_status();
    let node = statuses.iter().find(|s| s.id == "node").unwrap();
    assert!(node.installed, "registry failed to detect node");
    assert!(node.version.is_some());
}

#[test]
fn npm_package_names_exist_on_registry() {
    // Guards against typo'd package names in the agent registry: `npm view`
    // resolves each against the live npm registry. Needs network + npm;
    // skipped when npm is absent (matches this file's skip convention).
    let npm_ok = std::process::Command::new("npm")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !npm_ok {
        eprintln!("skip: npm not on this host");
        return;
    }
    for def in clawbox_lib::agents::agents() {
        if let clawbox_lib::agents::InstallMethod::Npm { package, .. } = def.install {
            let out = std::process::Command::new("npm")
                .args(["view", package, "version"])
                .output()
                .expect("npm view runs");
            assert!(
                out.status.success(),
                "npm package for {} not found: {}",
                def.id,
                package
            );
        }
    }
}

#[test]
#[ignore = "mutates global npm state; run explicitly: cargo test --test smoke -- --ignored"]
fn gated_npm_install_is_idempotent() {
    // Reinstall an already-installed npm agent (idempotent upgrade path) to
    // exercise run_install end-to-end. Ignored by default: it hits the
    // network and rewrites the global npm bin links.
    clawbox_lib::path_env::init();
    let def = clawbox_lib::agents::find_agent("openclaw").unwrap();
    if !def.is_installed() {
        eprintln!("skip: openclaw not installed; not installing fresh");
        return;
    }
    let result = clawbox_lib::agents::install::run_install(def);
    assert!(result.is_ok(), "reinstall failed: {:?}", result.err());
    assert!(def.is_installed(), "binary vanished after reinstall");
}
