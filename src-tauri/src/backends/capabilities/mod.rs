use serde::Serialize;

// MCP —— sync::cli 的 CliMcpAdapter(hermes/openclaw 的 MCP 统一下发)走
// 这里的 mcp_list/add/remove,绝不可删。
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

// Memory —— 记忆页原生状态折叠区(memory_status_all/index/reset)仍在用。
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
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
