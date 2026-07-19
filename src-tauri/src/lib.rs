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
            commands::aggregate::skills_list_all,
            commands::aggregate::skills_install,
            commands::aggregate::skills_uninstall,
            commands::aggregate::skills_set_enabled,
            commands::aggregate::memory_status_all,
            commands::aggregate::memory_index,
            commands::aggregate::memory_reset,
            commands::aggregate::plugins_list_all,
            commands::aggregate::plugins_install,
            commands::aggregate::plugins_remove,
            commands::aggregate::plugins_set_enabled,
            commands::aggregate::tools_list_all,
            commands::aggregate::tools_set_enabled,
            commands::aggregate::hooks_list_all,
            commands::aggregate::hooks_set_enabled,
            commands::config::get_config,
            commands::config::set_config,
            commands::config::config_providers_get,
            commands::config::config_providers_set,
            commands::config::config_active_provider_get,
            commands::config::config_active_provider_set,
            commands::provider_test::provider_test,
            commands::feedback::feedback_submit,
            commands::feedback::feedback_list,
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
            commands::sync::sync_providers_plan,
            commands::sync::sync_providers_apply,
            commands::sync::sync_providers_status,
            commands::sync::agent_sync_overview,
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