//! Tauri commands for MCP unified deployment. Thin wrappers: all logic
//! lives in `crate::sync` / `crate::commands::config` so it stays testable
//! against a tempdir home.

use crate::commands::config::{load_config, real_home, save_config, McpServerSpec};
use crate::sync::{self, AgentPlan, ApplyResult};
use std::collections::BTreeMap;
use std::path::Path;

#[tauri::command]
pub async fn config_mcp_list() -> Result<BTreeMap<String, McpServerSpec>, String> {
    Ok(load_config(&real_home())?.mcp_servers)
}

/// Home-parameterized core of `config_mcp_upsert` so tests can point it at a
/// tempdir. A corrupt config file must fail here *before* any write.
pub fn mcp_upsert_at(home: &Path, name: String, spec: McpServerSpec) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("server name must not be empty".to_string());
    }
    if !matches!(spec.kind.as_str(), "stdio" | "http") {
        return Err(format!("unsupported server kind: {}", spec.kind));
    }
    let mut config = load_config(home)?;
    config.mcp_servers.insert(name, spec);
    save_config(home, &config)
}

#[tauri::command]
pub async fn config_mcp_upsert(name: String, spec: McpServerSpec) -> Result<(), String> {
    mcp_upsert_at(&real_home(), name, spec)
}

#[tauri::command]
pub async fn config_mcp_remove(name: String) -> Result<(), String> {
    let home = real_home();
    let mut config = load_config(&home)?;
    if config.mcp_servers.remove(&name).is_none() {
        return Err(format!("unknown MCP server: {}", name));
    }
    save_config(&home, &config)
}

#[tauri::command]
pub async fn sync_mcp_plan() -> Result<Vec<AgentPlan>, String> {
    let home = real_home();
    let config = load_config(&home)?;
    Ok(sync::plan_all(&home, &config.mcp_servers, &config.mcp_managed))
}

/// Apply to the selected agents one by one. Per-agent failures land in the
/// corresponding ApplyResult; a success updates that agent's `mcp_managed`
/// entry to exactly the set deployed this run.
#[tauri::command]
pub async fn sync_mcp_apply(agent_ids: Vec<String>) -> Result<Vec<ApplyResult>, String> {
    let home = real_home();
    let mut config = load_config(&home)?;
    let mut results = Vec::with_capacity(agent_ids.len());

    for id in agent_ids {
        let Some(adapter) = sync::find_adapter(&id) else {
            results.push(ApplyResult {
                agent_id: id,
                ok: false,
                backup_path: None,
                applied: 0,
                error: Some("unknown agent".to_string()),
            });
            continue;
        };
        let managed = config.mcp_managed.get(&id).cloned().unwrap_or_default();
        let result = sync::apply_one(&home, adapter, &config.mcp_servers, &managed);
        if result.ok {
            config
                .mcp_managed
                .insert(id, adapter.deployed_names(&config.mcp_servers));
            save_config(&home, &config)?;
        }
        results.push(result);
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::config::clawbox_config_path;
    use crate::sync::test_util::TempHome;
    use std::fs;

    fn stdio_spec() -> McpServerSpec {
        McpServerSpec {
            kind: "stdio".to_string(),
            command: Some("npx".to_string()),
            args: vec!["my-mcp".to_string()],
            env: BTreeMap::new(),
            url: None,
            headers: BTreeMap::new(),
            enabled: true,
        }
    }

    #[test]
    fn upsert_on_corrupt_config_fails_and_leaves_file_untouched() {
        let home = TempHome::new();
        let path = clawbox_config_path(home.path());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let corrupt = r#"{"models": TRUNCATED"#;
        fs::write(&path, corrupt).unwrap();

        let err = mcp_upsert_at(home.path(), "srv".to_string(), stdio_spec()).unwrap_err();
        assert!(err.contains("corrupt"), "unexpected error: {}", err);

        // The whole point: the broken file must not be overwritten.
        assert_eq!(fs::read_to_string(&path).unwrap(), corrupt);
    }

    #[test]
    fn upsert_on_missing_config_creates_it() {
        let home = TempHome::new();
        mcp_upsert_at(home.path(), "srv".to_string(), stdio_spec()).unwrap();
        let loaded = load_config(home.path()).unwrap();
        assert!(loaded.mcp_servers.contains_key("srv"));
    }
}
