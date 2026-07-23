pub mod agents;
pub mod backends;
pub mod path_env;
pub mod sync;
pub mod commands;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Must run before the builder spawns any threads: set_var is process-wide.
    path_env::init();
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::aggregate::list_backends,
            commands::aggregate::gateway_status_all,
            commands::aggregate::memory_status_all,
            commands::aggregate::memory_index,
            commands::aggregate::memory_reset,
            commands::config::get_config,
            commands::config::set_config,
            commands::config::config_providers_get,
            commands::config::config_providers_set,
            commands::provider_test::provider_test,
            commands::cc_switch::cc_switch_import_preview,
            commands::install::check_system,
            commands::install::install_openclaw,
            commands::install::install_nodejs,
            commands::install::check_update,
            commands::install::check_openclaw_update,
            commands::agents::agents_list,
            commands::agents::agent_install,
            commands::sync::config_mcp_list,
            commands::sync::config_mcp_upsert,
            commands::sync::config_mcp_remove,
            commands::sync::sync_mcp_plan,
            commands::sync::sync_mcp_apply,
            commands::sync::agent_sync_overview,
            commands::sync::agent_provider_bind,
            commands::sync::agent_providers_get,
            commands::sync::skills_library_list,
            commands::sync::skills_import,
            commands::sync::skills_library_remove,
            commands::sync::skills_scan,
            commands::sync::skills_adopt,
            commands::sync::sync_skills_plan,
            commands::sync::sync_skills_apply,
            commands::sync::skills_repo_discover,
            commands::sync::skills_repo_install,
            commands::sync::skills_check_updates,
            commands::sync::skills_update,
            commands::sync::memory_read,
            commands::sync::memory_write,
            commands::sync::memory_targets,
            commands::sync::memory_target_content,
            commands::sync::sync_memory_plan,
            commands::sync::sync_memory_apply,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}