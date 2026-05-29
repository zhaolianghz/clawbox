use serde::Serialize;
use std::fs;
use std::process::Command;

#[derive(Serialize)]
pub struct GatewayStatus {
    pub status: String,
    pub version: String,
    pub pid: Option<i32>,
}

fn check_gateway_running() -> bool {
    let output = Command::new("lsof")
        .arg("-i")
        .arg(":18789")
        .output();

    match output {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            stdout.contains("LISTEN")
        }
        _ => false,
    }
}

#[tauri::command]
pub fn get_gateway_status() -> Result<GatewayStatus, String> {
    let running = check_gateway_running();
    let status = if running { "running" } else { "stopped" };

    let version = Command::new("openclaw")
        .arg("--version")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    Ok(GatewayStatus {
        status: status.to_string(),
        version,
        pid: None,
    })
}

#[tauri::command]
pub fn start_gateway() -> Result<String, String> {
    let mut child = Command::new("openclaw-gateway")
        .spawn()
        .map_err(|e| format!("Failed to start openclaw-gateway: {}", e))?;

    // Optionally wait for the child to start
    let _ = child.try_wait();

    Ok("Gateway starting".to_string())
}

#[tauri::command]
pub fn stop_gateway() -> Result<String, String> {
    Command::new("pkill")
        .arg("-f")
        .arg("openclaw-gateway")
        .output()
        .map_err(|e| format!("Failed to execute pkill: {}", e))?;

    Ok("Gateway stopping".to_string())
}

#[tauri::command]
pub fn get_gateway_token() -> Result<String, String> {
    let home = std::env::var("HOME").map_err(|_| "Failed to get HOME directory")?;
    let config_path = format!("{}/.openclaw/openclaw.json", home);

    let content = fs::read_to_string(&config_path)
        .map_err(|e| format!("Failed to read config: {}", e))?;

    let json: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse config JSON: {}", e))?;

    let token = json["gateway"]["auth"]["token"]
        .as_str()
        .ok_or_else(|| "No gateway token found in config".to_string())?;

    Ok(token.to_string())
}
