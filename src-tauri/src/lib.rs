pub mod backends;
pub mod acp;
mod commands;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
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
            commands::aggregate::skills_list_all,
            commands::aggregate::skills_install,
            commands::aggregate::skills_uninstall,
            commands::aggregate::skills_set_enabled,
            commands::aggregate::mcp_list_all,
            commands::aggregate::mcp_add,
            commands::aggregate::mcp_remove,
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
            commands::aggregate::get_stats,
            commands::config::get_config,
            commands::config::set_config,
            commands::chat::get_gateway_token,
            commands::feedback::feedback_submit,
            commands::feedback::feedback_list,
            commands::install::check_system,
            commands::install::install_openclaw,
            commands::install::install_nodejs,
            commands::install::check_update,
            commands::install::check_openclaw_update,
            commands::logs::get_log_files,
            commands::logs::get_log_content,
            commands::acp::acp_list_adapters,
            commands::acp::acp_install_adapter,
            commands::acp::review_run,
            commands::acp::review_list,
            commands::acp::review_get,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}