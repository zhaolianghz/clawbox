use serde::Serialize;

// Skills
#[derive(Serialize, Clone, Debug)]
pub struct Skill {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub enabled: bool,
    pub raw: serde_json::Value,
}
pub trait SkillsCapability: Send + Sync {
    fn skills_list(&self) -> Result<Vec<Skill>, String>;
    fn skills_install(&self, id: &str) -> Result<String, String>;
    fn skills_uninstall(&self, id: &str) -> Result<String, String>;
    fn skills_set_enabled(&self, id: &str, enabled: bool) -> Result<String, String>;
}

// MCP
#[derive(Serialize, Clone, Debug)]
pub struct McpServer {
    pub name: String,
    pub transport: String,
    pub status: String,
    pub raw: serde_json::Value,
}
pub trait McpCapability: Send + Sync {
    fn mcp_list(&self) -> Result<Vec<McpServer>, String>;
    fn mcp_add(&self, name: &str, config_json: &str) -> Result<String, String>;
    fn mcp_remove(&self, name: &str) -> Result<String, String>;
}

// Memory
#[derive(Serialize, Clone, Debug)]
pub struct MemoryStatus {
    pub provider: String,
    pub builtin_active: bool,
    pub raw: serde_json::Value,
}
pub trait MemoryCapability: Send + Sync {
    fn memory_status(&self) -> Result<MemoryStatus, String>;
    fn memory_index(&self) -> Result<String, String>;
    fn memory_reset(&self) -> Result<String, String>;
}

// Plugins
#[derive(Serialize, Clone, Debug)]
pub struct Plugin {
    pub id: String,
    pub name: String,
    pub version: String,
    pub enabled: bool,
    pub raw: serde_json::Value,
}
pub trait PluginsCapability: Send + Sync {
    fn plugins_list(&self) -> Result<Vec<Plugin>, String>;
    fn plugins_install(&self, source: &str) -> Result<String, String>;
    fn plugins_remove(&self, id: &str) -> Result<String, String>;
    fn plugins_set_enabled(&self, id: &str, enabled: bool) -> Result<String, String>;
}

// Tools
#[derive(Serialize, Clone, Debug)]
pub struct Tool {
    pub id: String,
    pub enabled: bool,
    pub raw: serde_json::Value,
}
pub trait ToolsCapability: Send + Sync {
    fn tools_list(&self) -> Result<Vec<Tool>, String>;
    fn tools_set_enabled(&self, id: &str, enabled: bool) -> Result<String, String>;
}

// Hooks
#[derive(Serialize, Clone, Debug)]
pub struct Hook {
    pub id: String,
    pub name: String,
    pub event: String,
    pub enabled: bool,
    pub raw: serde_json::Value,
}
pub trait HooksCapability: Send + Sync {
    fn hooks_list(&self) -> Result<Vec<Hook>, String>;
    fn hooks_set_enabled(&self, id: &str, enabled: bool) -> Result<String, String>;
}
