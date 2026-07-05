use serde::{Serialize, Deserialize};

mod hermes;
pub mod openclaw;
pub mod capabilities;

#[derive(Serialize, Clone, Debug)]
pub struct CronJob {
    pub id: String,
    pub name: String,
    pub schedule: String,
    pub enabled: bool,
    pub last_run: Option<String>,
    pub next_run: Option<String>,
    pub agent: Option<String>,
    pub message: Option<String>,
    pub raw: serde_json::Value,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct NewCron {
    pub name: String,
    pub schedule: String,
    pub message: Option<String>,
    pub agent: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct GatewayStatus {
    pub status: String,        // "running" | "stopped"
    pub version: String,
    pub pid: Option<i32>,
}

#[derive(Serialize, Clone, Debug)]
pub struct BackendInfo {
    pub id: String,
    pub display_name: String,
    pub version: String,
    pub installed: bool,
}

pub trait Backend: Send + Sync {
    fn id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    fn version(&self) -> String;
    fn is_installed(&self) -> bool;

    fn gateway_status(&self) -> Result<GatewayStatus, String>;
    fn gateway_start(&self) -> Result<String, String>;
    fn gateway_stop(&self) -> Result<String, String>;

    fn cron_list(&self) -> Result<Vec<CronJob>, String>;
    fn cron_create(&self, params: NewCron) -> Result<String, String>;
    fn cron_remove(&self, id: &str) -> Result<String, String>;
    fn cron_set_enabled(&self, id: &str, enabled: bool) -> Result<String, String>;
    fn cron_run(&self, id: &str) -> Result<String, String>;
}

pub fn backends() -> &'static [Box<dyn Backend>] {
    static INSTANCES: std::sync::OnceLock<Vec<Box<dyn Backend>>> = std::sync::OnceLock::new();
    INSTANCES.get_or_init(|| {
        vec![
            Box::new(openclaw::OpenClawBackend),
            Box::new(hermes::HermesBackend),
        ]
    }).as_slice()
}

pub fn find_backend(id: &str) -> Option<&'static dyn Backend> {
    backends().iter().find(|b| b.id() == id).map(|b| b.as_ref())
}

use capabilities::{HooksCapability, McpCapability, MemoryCapability, PluginsCapability, SkillsCapability, ToolsCapability};

pub struct BackendEntry {
    pub backend: &'static dyn Backend,
    pub skills: Option<&'static dyn SkillsCapability>,
    pub mcp: Option<&'static dyn McpCapability>,
    pub memory: Option<&'static dyn MemoryCapability>,
    pub plugins: Option<&'static dyn PluginsCapability>,
    pub tools: Option<&'static dyn ToolsCapability>,
    pub hooks: Option<&'static dyn HooksCapability>,
}

pub fn entries() -> &'static [BackendEntry] {
    static INSTANCES: std::sync::OnceLock<Vec<BackendEntry>> = std::sync::OnceLock::new();
    INSTANCES.get_or_init(|| {
        vec![
            BackendEntry {
                backend: &openclaw::OpenClawBackend,
                skills:  Some(&openclaw::OpenClawBackend),
                mcp:     Some(&openclaw::OpenClawBackend),
                memory:  Some(&openclaw::OpenClawBackend),
                plugins: None,
                tools:   None,
                hooks:   None,
            },
            BackendEntry {
                backend: &hermes::HermesBackend,
                skills:  Some(&hermes::HermesBackend),
                mcp:     Some(&hermes::HermesBackend),
                memory:  Some(&hermes::HermesBackend),
                plugins: None,
                tools:   None,
                hooks:   None,
            },
        ]
    }).as_slice()
}

pub fn find_entry(id: &str) -> Option<&'static BackendEntry> {
    entries().iter().find(|e| e.backend.id() == id)
}
