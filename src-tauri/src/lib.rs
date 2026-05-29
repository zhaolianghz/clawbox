mod commands;
use commands::{config, gateway, install, logs};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            gateway::get_gateway_status,
            gateway::start_gateway,
            gateway::stop_gateway,
            gateway::get_gateway_token,
            config::get_config,
            config::set_config,
            install::check_system,
            install::install_openclaw,
            install::check_update,
            install::check_openclaw_update,
            logs::get_log_files,
            logs::get_log_content,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
