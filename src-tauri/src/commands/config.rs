use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

fn default_true() -> bool {
    true
}

/// Canonical MCP server spec — ClawBox's single source of truth. Adapters in
/// `crate::sync` translate this into each agent's native config format.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct McpServerSpec {
    /// "stdio" | "http"
    pub kind: String,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

/// Model provider entry. camelCase on the wire so the frontend `ModelProvider`
/// type maps field-for-field with zero conversion.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSpec {
    pub id: String,
    pub name: String,
    pub api_key: String,
    pub base_url: String,
    #[serde(default)]
    pub default_model: String,
    /// Configured model ids for this provider. Absent in pre-models configs.
    #[serde(default)]
    pub models: Vec<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Config {
    pub models: HashMap<String, serde_json::Value>,
    pub channels: HashMap<String, serde_json::Value>,
    pub agents: HashMap<String, serde_json::Value>,
    pub skills: HashMap<String, serde_json::Value>,
    /// Canonical MCP server registry (name -> spec). BTreeMap for stable order.
    #[serde(default)]
    pub mcp_servers: BTreeMap<String, McpServerSpec>,
    /// agent_id -> server names deployed by the last successful sync. Drives
    /// remove-detection: only names we previously wrote may be removed.
    #[serde(default)]
    pub mcp_managed: HashMap<String, Vec<String>>,
    /// Configured model providers. Managed via config_providers_get/set.
    #[serde(default)]
    pub providers: Vec<ProviderSpec>,
}

/// ClawBox config path resolved against an explicit home dir so tests can
/// point it at a tempdir without touching the real user config.
pub fn clawbox_config_path(home: &Path) -> PathBuf {
    home.join(".clawbox").join("config.json")
}

pub fn real_home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

/// Load config from `<home>/.clawbox/config.json`. A missing file is the
/// only case that falls back to defaults; read/parse failures return Err.
///
/// Never silently default on a corrupt file: every write path is
/// load-modify-save on the whole Config, so a defaulted load followed by
/// any save would wipe the entire user config (data loss).
pub fn load_config(home: &Path) -> Result<Config, String> {
    let path = clawbox_config_path(home);
    if !path.exists() {
        return Ok(Config::default());
    }
    let content = fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read config file {}: {}", path.display(), e))?;
    serde_json::from_str(&content)
        .map_err(|e| format!("Config file {} is corrupt: {}", path.display(), e))
}

pub fn save_config(home: &Path, config: &Config) -> Result<(), String> {
    let path = clawbox_config_path(home);
    let dir = path.parent().unwrap();
    if !dir.exists() {
        fs::create_dir_all(dir).map_err(|e| format!("Failed to create config dir: {}", e))?;
    }
    let content = serde_json::to_string_pretty(config)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;
    fs::write(&path, content).map_err(|e| format!("Failed to write config: {}", e))
}

#[tauri::command]
pub async fn get_config() -> Result<Config, String> {
    load_config(&real_home())
}

#[tauri::command]
pub async fn set_config(path: String, value: serde_json::Value) -> Result<(), String> {
    let home = real_home();
    let mut config = load_config(&home)?;

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
        "providers" => {
            return Err(
                "providers is not editable via set_config; use config_providers_set".to_string(),
            )
        }
        _ => return Err(format!("Unknown config section: {}", parts[0])),
    }

    save_config(&home, &config)?;
    Ok(())
}

#[tauri::command]
pub async fn config_providers_get() -> Result<Vec<ProviderSpec>, String> {
    Ok(load_config(&real_home())?.providers)
}

/// Whole-table overwrite: the frontend always sends the full provider list.
#[tauri::command]
pub async fn config_providers_set(providers: Vec<ProviderSpec>) -> Result<(), String> {
    let home = real_home();
    let mut config = load_config(&home)?;
    config.providers = providers;
    save_config(&home, &config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::test_util::TempHome;

    fn spec(id: &str, name: &str) -> ProviderSpec {
        ProviderSpec {
            id: id.to_string(),
            name: name.to_string(),
            api_key: "sk-test".to_string(),
            base_url: "https://api.example.com/v1".to_string(),
            default_model: "model-x".to_string(),
            models: vec!["model-x".to_string(), "model-y".to_string()],
            enabled: true,
        }
    }

    #[test]
    fn providers_roundtrip() {
        let home = TempHome::new();
        let mut config = load_config(home.path()).unwrap();
        config.providers = vec![spec("a", "Alpha"), spec("b", "Beta")];
        save_config(home.path(), &config).unwrap();

        let loaded = load_config(home.path()).unwrap();
        assert_eq!(loaded.providers, vec![spec("a", "Alpha"), spec("b", "Beta")]);
    }

    #[test]
    fn providers_serialize_camel_case() {
        let json = serde_json::to_value(spec("a", "Alpha")).unwrap();
        assert!(json.get("apiKey").is_some());
        assert!(json.get("baseUrl").is_some());
        assert!(json.get("defaultModel").is_some());
        assert!(json.get("api_key").is_none());
    }

    #[test]
    fn legacy_config_without_providers_loads() {
        let home = TempHome::new();
        let path = clawbox_config_path(home.path());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        // Old-format config.json written before the providers field existed.
        fs::write(
            &path,
            r#"{"models":{},"channels":{},"agents":{},"skills":{}}"#,
        )
        .unwrap();

        let loaded = load_config(home.path()).unwrap();
        assert!(loaded.providers.is_empty());

        // Defaulted optional fields also deserialize from sparse entries.
        let sparse: ProviderSpec = serde_json::from_str(
            r#"{"id":"x","name":"X","apiKey":"k","baseUrl":"https://x"}"#,
        )
        .unwrap();
        assert_eq!(sparse.default_model, "");
        assert!(sparse.models.is_empty());
        assert!(sparse.enabled);
    }

    #[test]
    fn missing_config_file_loads_default() {
        let home = TempHome::new();
        let loaded = load_config(home.path()).unwrap();
        assert!(loaded.providers.is_empty());
        assert!(loaded.mcp_servers.is_empty());
    }

    #[test]
    fn corrupt_config_file_is_an_error_not_default() {
        let home = TempHome::new();
        let path = clawbox_config_path(home.path());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, r#"{"models": TRUNCATED"#).unwrap();

        let err = load_config(home.path()).unwrap_err();
        assert!(err.contains("corrupt"), "unexpected error: {}", err);
        assert!(
            err.contains(&path.display().to_string()),
            "error should name the file: {}",
            err
        );
    }
}
