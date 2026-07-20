use serde::Serialize;

mod hermes;
pub mod openclaw;
pub mod capabilities;

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

use capabilities::{McpCapability, MemoryCapability};

pub struct BackendEntry {
    pub backend: &'static dyn Backend,
    pub mcp: Option<&'static dyn McpCapability>,
    pub memory: Option<&'static dyn MemoryCapability>,
}

pub fn entries() -> &'static [BackendEntry] {
    static INSTANCES: std::sync::OnceLock<Vec<BackendEntry>> = std::sync::OnceLock::new();
    INSTANCES.get_or_init(|| {
        vec![
            BackendEntry {
                backend: &openclaw::OpenClawBackend,
                mcp:     Some(&openclaw::OpenClawBackend),
                memory:  Some(&openclaw::OpenClawBackend),
            },
            BackendEntry {
                backend: &hermes::HermesBackend,
                mcp:     Some(&hermes::HermesBackend),
                memory:  Some(&hermes::HermesBackend),
            },
        ]
    }).as_slice()
}

pub fn find_entry(id: &str) -> Option<&'static BackendEntry> {
    entries().iter().find(|e| e.backend.id() == id)
}
