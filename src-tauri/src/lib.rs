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