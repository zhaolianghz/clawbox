use serde::Serialize;
use std::process::Command;

#[derive(Serialize)]
pub struct GatewayStatus {
    pub status: String,
    pub version: String,
    pub pid: Option<i32>,
}

#[tauri::command]
pub fn get_gateway_status() -> GatewayStatus {
    let output = Command::new("pgrep")
        .arg("-f")
        .arg("openclaw gateway")
        .output();

    let pid = match output {
        Ok(output) if output.status.success() => String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse::<i32>()
            .ok(),
        _ => None,
    };

    let status = if pid.is_some() { "running" } else { "stopped" };

    let version = Command::new("openclaw")
        .arg("--version")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    GatewayStatus {
        status: status.to_string(),
        version,
        pid,
    }
}

#[tauri::command]
pub fn start_gateway() -> Result<String, String> {
    let output = Command::new("openclaw").arg("gateway").arg("start").spawn();

    match output {
        Ok(_) => Ok("Gateway starting".to_string()),
        Err(e) => Err(format!("Failed to start gateway: {}", e)),
    }
}

#[tauri::command]
pub fn stop_gateway() -> Result<String, String> {
    let output = Command::new("openclaw").arg("gateway").arg("stop").spawn();

    match output {
        Ok(_) => Ok("Gateway stopping".to_string()),
        Err(e) => Err(format!("Failed to stop gateway: {}", e)),
    }
}
