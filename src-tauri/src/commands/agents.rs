//! Tauri command layer for the unified agent registry.

use crate::agents::{self, AgentStatus, InstallMethod};
use crate::path_env::{self, PathInitStatus};

#[tauri::command]
pub async fn agents_list() -> Vec<AgentStatus> {
    // 12 parallel binary probes — keep them off the main thread.
    tauri::async_runtime::spawn_blocking(agents::list_agent_status)
        .await
        .unwrap_or_default()
}

#[tauri::command]
pub async fn agent_install(id: String) -> Result<String, String> {
    let def = agents::find_agent(&id).ok_or_else(|| format!("unknown agent: {}", id))?;
    match def.install {
        // node reuses the existing brew/winget flow.
        InstallMethod::PlatformPkg => super::install::install_nodejs().await,
        InstallMethod::DetectOnly => Err(format!(
            "{} must be installed manually",
            def.label
        )),
        _ => tauri::async_runtime::spawn_blocking(move || agents::install::run_install(def))
            .await
            .map_err(|e| e.to_string())?,
    }
}

/// Report whether PATH resolution at startup recovered the interactive-shell
/// PATH or degraded to well-known dirs. Drives the agents-page warning banner
/// (GH#3: installed agents may read "not installed" when PATH is degraded).
#[tauri::command]
pub async fn path_env_status() -> PathInitStatus {
    path_env::init_status()
}
