use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct Config {
    pub models: HashMap<String, serde_json::Value>,
    pub channels: HashMap<String, serde_json::Value>,
    pub agents: HashMap<String, serde_json::Value>,
    pub skills: HashMap<String, serde_json::Value>,
}

fn config_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".clawbox")
        .join("config.json")
}

fn ensure_config_dir() -> Result<(), String> {
    let dir = config_path().parent().unwrap().to_path_buf();
    if !dir.exists() {
        fs::create_dir_all(&dir).map_err(|e| format!("Failed to create config dir: {}", e))?;
    }
    Ok(())
}

#[tauri::command]
pub async fn get_config() -> Result<Config, String> {
    let path = config_path();
    if !path.exists() {
        return Ok(Config::default());
    }

    let content = fs::read_to_string(&path).map_err(|e| format!("Failed to read config: {}", e))?;

    let config: Config = serde_json::from_str(&content).unwrap_or_else(|_| Config::default());

    Ok(config)
}

#[tauri::command]
pub async fn set_config(path: String, value: serde_json::Value) -> Result<(), String> {
    ensure_config_dir()?;

    let mut config = get_config().await.unwrap_or_default();

    let parts: Vec<&str> = path.split('.').collect();
    if parts.is_empty() {
        return Err("Invalid path".to_string());
    }

    match parts[0] {
        "models" => {
            if parts.len() > 1 {
                config.models.insert(parts[1].to_string(), value);
            }
        }
        "channels" => {
            if parts.len() > 1 {
                config.channels.insert(parts[1].to_string(), value);
            }
        }
        "agents" => {
            if parts.len() > 1 {
                config.agents.insert(parts[1].to_string(), value);
            }
        }
        "skills" => {
            if parts.len() > 1 {
                config.skills.insert(parts[1].to_string(), value);
            }
        }
        _ => return Err(format!("Unknown config section: {}", parts[0])),
    }

    let content = serde_json::to_string_pretty(&config)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;

    fs::write(config_path(), content).map_err(|e| format!("Failed to write config: {}", e))?;

    Ok(())
}
