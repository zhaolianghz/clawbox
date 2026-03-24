use serde::Serialize;
use std::process::Command;

#[derive(Serialize)]
pub struct SystemCheck {
    pub nodejs: ComponentStatus,
    pub openclaw: ComponentStatus,
    pub platform: String,
    pub is_china: bool,
}

#[derive(Serialize)]
pub struct ComponentStatus {
    pub installed: bool,
    pub version: Option<String>,
}

fn check_command_version(cmd: &str, version_arg: &str) -> ComponentStatus {
    let output = Command::new(cmd).arg(version_arg).output();

    match output {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout)
                .trim()
                .lines()
                .next()
                .map(|s| s.to_string());
            ComponentStatus {
                installed: true,
                version,
            }
        }
        _ => ComponentStatus {
            installed: false,
            version: None,
        },
    }
}

fn check_china_network() -> bool {
    std::env::var("LANG")
        .map(|l| l.starts_with("zh"))
        .unwrap_or(false)
}

#[tauri::command]
pub fn check_system() -> SystemCheck {
    SystemCheck {
        nodejs: check_command_version("node", "--version"),
        openclaw: check_command_version("openclaw", "--version"),
        platform: std::env::consts::OS.to_string(),
        is_china: check_china_network(),
    }
}

#[tauri::command]
pub fn install_openclaw(use_mirror: bool) -> Result<String, String> {
    let mut args = vec!["install", "-g", "openclaw"];

    if use_mirror {
        let registry = "--registry=https://registry.npmmirror.com";
        args.push(registry);
    }

    let output = Command::new("npm").args(&args).spawn();

    match output {
        Ok(_) => Ok("Installing openclaw...".to_string()),
        Err(e) => Err(format!("Failed to install openclaw: {}", e)),
    }
}
