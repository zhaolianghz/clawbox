use rayon::prelude::*;
use serde::Serialize;

use crate::backends::{self, BackendInfo, Backend, CronJob, NewCron};

#[derive(Serialize)]
pub struct TaggedCronJob {
    pub backend: String,
    pub job: CronJob,
}

#[derive(Serialize)]
pub struct BackendError {
    pub backend: String,
    pub message: String,
}

#[derive(Serialize)]
pub struct CronListAllResult {
    pub jobs: Vec<TaggedCronJob>,
    pub errors: Vec<BackendError>,
}

#[derive(Serialize)]
pub struct TaggedGatewayStatus {
    pub backend: String,
    pub status: crate::backends::GatewayStatus,
}

#[derive(Serialize)]
pub struct GatewayStatusAllResult {
    pub statuses: Vec<TaggedGatewayStatus>,
    pub errors: Vec<BackendError>,
}

use crate::backends::capabilities::{McpCapability, PluginsCapability, SkillsCapability};

#[derive(Serialize)]
pub struct TaggedItem<T> {
    pub backend: String,
    pub item: T,
}

#[derive(Serialize)]
pub struct TaggedListResult<T> {
    pub items: Vec<TaggedItem<T>>,
    pub errors: Vec<BackendError>,
}

fn collect_capability<T, F>(f: F) -> TaggedListResult<T>
where
    T: Send,
    F: Fn(&dyn SkillsCapability) -> Result<Vec<T>, String> + Sync + Send,
{
    use rayon::prelude::*;
    let results: Vec<_> = backends::entries().par_iter()
        .filter_map(|e| e.skills.map(|s| (e, s)))
        .map(|(e, s)| {
            if !e.backend.is_installed() {
                return (e.backend.id().to_string(), None, None);
            }
            match f(s) {
                Ok(v) => (e.backend.id().to_string(), Some(v), None),
                Err(err) => (e.backend.id().to_string(), None, Some(BackendError {
                    backend: e.backend.id().to_string(), message: err,
                })),
            }
        }).collect();
    let mut items = Vec::new();
    let mut errors = Vec::new();
    for (id, val, err) in results {
        if let Some(e) = err { errors.push(e); }
        if let Some(v) = val {
            for item in v {
                items.push(TaggedItem { backend: id.clone(), item });
            }
        }
    }
    TaggedListResult { items, errors }
}

fn collect_backends<F, T>(f: F) -> (Vec<(String, T)>, Vec<BackendError>)
where
    F: Fn(&dyn Backend) -> Result<T, String> + Sync + Send,
    T: Send,
{
    let results: Vec<_> = backends::backends().par_iter()
        .map(|b| {
            if !b.is_installed() {
                return (b.id().to_string(), None, None);
            }
            match f(b.as_ref()) {
                Ok(v) => (b.id().to_string(), Some(v), None),
                Err(e) => (b.id().to_string(), None, Some(BackendError {
                    backend: b.id().to_string(), message: e,
                })),
            }
        }).collect();

    let mut values = Vec::new();
    let mut errors = Vec::new();
    for (id, val, err) in results {
        if let Some(e) = err { errors.push(e); }
        if let Some(v) = val { values.push((id, v)); }
    }
    (values, errors)
}

#[tauri::command]
pub fn list_backends() -> Vec<BackendInfo> {
    backends::backends().iter().map(|b| BackendInfo {
        id: b.id().to_string(),
        display_name: b.display_name().to_string(),
        version: b.version(),
        installed: b.is_installed(),
    }).collect()
}

#[tauri::command]
pub fn gateway_status_all() -> GatewayStatusAllResult {
    let (pairs, errors) = collect_backends(|b| b.gateway_status());
    let statuses = pairs.into_iter().map(|(backend, status)| TaggedGatewayStatus {
        backend, status,
    }).collect();
    GatewayStatusAllResult { statuses, errors }
}

#[tauri::command]
pub fn gateway_start(backend: String) -> Result<String, String> {
    backends::find_backend(&backend)
        .ok_or_else(|| format!("Unknown backend: {}", backend))?
        .gateway_start()
}

#[tauri::command]
pub fn gateway_stop(backend: String) -> Result<String, String> {
    backends::find_backend(&backend)
        .ok_or_else(|| format!("Unknown backend: {}", backend))?
        .gateway_stop()
}

#[tauri::command]
pub fn cron_list_all() -> CronListAllResult {
    let (pairs, errors) = collect_backends(|b| b.cron_list());
    let jobs = pairs.into_iter()
        .flat_map(|(id, js)| js.into_iter().map(move |j| TaggedCronJob {
            backend: id.clone(), job: j,
        }))
        .collect();
    CronListAllResult { jobs, errors }
}

#[tauri::command]
pub fn cron_create(backend: String, params: NewCron) -> Result<String, String> {
    backends::find_backend(&backend)
        .ok_or_else(|| format!("Unknown backend: {}", backend))?
        .cron_create(params)
}

#[tauri::command]
pub fn cron_remove(backend: String, id: String) -> Result<String, String> {
    backends::find_backend(&backend)
        .ok_or_else(|| format!("Unknown backend: {}", backend))?
        .cron_remove(&id)
}

#[tauri::command]
pub fn cron_set_enabled(backend: String, id: String, enabled: bool) -> Result<String, String> {
    backends::find_backend(&backend)
        .ok_or_else(|| format!("Unknown backend: {}", backend))?
        .cron_set_enabled(&id, enabled)
}

#[tauri::command]
pub fn cron_run(backend: String, id: String) -> Result<String, String> {
    backends::find_backend(&backend)
        .ok_or_else(|| format!("Unknown backend: {}", backend))?
        .cron_run(&id)
}

#[tauri::command]
pub fn skills_list_all() -> TaggedListResult<crate::backends::capabilities::Skill> {
    collect_capability(|s| s.skills_list())
}

#[tauri::command]
pub fn skills_install(backend: String, id: String) -> Result<String, String> {
    let entry = backends::find_entry(&backend)
        .ok_or_else(|| format!("Unknown backend: {}", backend))?;
    let skills = entry.skills
        .ok_or_else(|| format!("{} does not support skills", backend))?;
    skills.skills_install(&id)
}

#[tauri::command]
pub fn skills_uninstall(backend: String, id: String) -> Result<String, String> {
    let entry = backends::find_entry(&backend)
        .ok_or_else(|| format!("Unknown backend: {}", backend))?;
    let skills = entry.skills
        .ok_or_else(|| format!("{} does not support skills", backend))?;
    skills.skills_uninstall(&id)
}

#[tauri::command]
pub fn skills_set_enabled(backend: String, id: String, enabled: bool) -> Result<String, String> {
    let entry = backends::find_entry(&backend)
        .ok_or_else(|| format!("Unknown backend: {}", backend))?;
    let skills = entry.skills
        .ok_or_else(|| format!("{} does not support skills", backend))?;
    skills.skills_set_enabled(&id, enabled)
}

fn collect_capability_mcp<T, F>(f: F) -> TaggedListResult<T>
where
    T: Send,
    F: Fn(&dyn McpCapability) -> Result<Vec<T>, String> + Sync + Send,
{
    use rayon::prelude::*;
    let results: Vec<_> = backends::entries().par_iter()
        .filter_map(|e| e.mcp.map(|m| (e, m)))
        .map(|(e, m)| {
            if !e.backend.is_installed() {
                return (e.backend.id().to_string(), None, None);
            }
            match f(m) {
                Ok(v) => (e.backend.id().to_string(), Some(v), None),
                Err(err) => (e.backend.id().to_string(), None, Some(BackendError {
                    backend: e.backend.id().to_string(), message: err,
                })),
            }
        }).collect();
    let mut items = Vec::new();
    let mut errors = Vec::new();
    for (id, val, err) in results {
        if let Some(e) = err { errors.push(e); }
        if let Some(v) = val {
            for item in v {
                items.push(TaggedItem { backend: id.clone(), item });
            }
        }
    }
    TaggedListResult { items, errors }
}

#[tauri::command]
pub fn mcp_list_all() -> TaggedListResult<crate::backends::capabilities::McpServer> {
    collect_capability_mcp(|m| m.mcp_list())
}

#[tauri::command]
pub fn mcp_add(backend: String, name: String, config_json: String) -> Result<String, String> {
    let entry = backends::find_entry(&backend)
        .ok_or_else(|| format!("Unknown backend: {}", backend))?;
    let mcp = entry.mcp
        .ok_or_else(|| format!("{} does not support mcp", backend))?;
    mcp.mcp_add(&name, &config_json)
}

#[tauri::command]
pub fn mcp_remove(backend: String, name: String) -> Result<String, String> {
    let entry = backends::find_entry(&backend)
        .ok_or_else(|| format!("Unknown backend: {}", backend))?;
    let mcp = entry.mcp
        .ok_or_else(|| format!("{} does not support mcp", backend))?;
    mcp.mcp_remove(&name)
}

fn collect_capability_plugins<T, F>(f: F) -> TaggedListResult<T>
where
    T: Send,
    F: Fn(&dyn PluginsCapability) -> Result<Vec<T>, String> + Sync + Send,
{
    use rayon::prelude::*;
    let results: Vec<_> = backends::entries().par_iter()
        .filter_map(|e| e.plugins.map(|p| (e, p)))
        .map(|(e, p)| {
            if !e.backend.is_installed() {
                return (e.backend.id().to_string(), None, None);
            }
            match f(p) {
                Ok(v) => (e.backend.id().to_string(), Some(v), None),
                Err(err) => (e.backend.id().to_string(), None, Some(BackendError {
                    backend: e.backend.id().to_string(), message: err,
                })),
            }
        }).collect();
    let mut items = Vec::new();
    let mut errors = Vec::new();
    for (id, val, err) in results {
        if let Some(e) = err { errors.push(e); }
        if let Some(v) = val {
            for item in v {
                items.push(TaggedItem { backend: id.clone(), item });
            }
        }
    }
    TaggedListResult { items, errors }
}

#[tauri::command]
pub fn plugins_list_all() -> TaggedListResult<crate::backends::capabilities::Plugin> {
    collect_capability_plugins(|p| p.plugins_list())
}

#[tauri::command]
pub fn plugins_install(backend: String, source: String) -> Result<String, String> {
    let entry = backends::find_entry(&backend)
        .ok_or_else(|| format!("Unknown backend: {}", backend))?;
    let plugins = entry.plugins
        .ok_or_else(|| format!("{} does not support plugins", backend))?;
    plugins.plugins_install(&source)
}

#[tauri::command]
pub fn plugins_remove(backend: String, id: String) -> Result<String, String> {
    let entry = backends::find_entry(&backend)
        .ok_or_else(|| format!("Unknown backend: {}", backend))?;
    let plugins = entry.plugins
        .ok_or_else(|| format!("{} does not support plugins", backend))?;
    plugins.plugins_remove(&id)
}

#[tauri::command]
pub fn plugins_set_enabled(backend: String, id: String, enabled: bool) -> Result<String, String> {
    let entry = backends::find_entry(&backend)
        .ok_or_else(|| format!("Unknown backend: {}", backend))?;
    let plugins = entry.plugins
        .ok_or_else(|| format!("{} does not support plugins", backend))?;
    plugins.plugins_set_enabled(&id, enabled)
}

use crate::backends::capabilities::{MemoryCapability, ToolsCapability};

fn collect_capability_tools<T, F>(f: F) -> TaggedListResult<T>
where
    T: Send,
    F: Fn(&dyn ToolsCapability) -> Result<Vec<T>, String> + Sync + Send,
{
    use rayon::prelude::*;
    let results: Vec<_> = backends::entries().par_iter()
        .filter_map(|e| e.tools.map(|t| (e, t)))
        .map(|(e, t)| {
            if !e.backend.is_installed() {
                return (e.backend.id().to_string(), None, None);
            }
            match f(t) {
                Ok(v) => (e.backend.id().to_string(), Some(v), None),
                Err(err) => (e.backend.id().to_string(), None, Some(BackendError {
                    backend: e.backend.id().to_string(), message: err,
                })),
            }
        }).collect();
    let mut items = Vec::new();
    let mut errors = Vec::new();
    for (id, val, err) in results {
        if let Some(e) = err { errors.push(e); }
        if let Some(v) = val {
            for item in v {
                items.push(TaggedItem { backend: id.clone(), item });
            }
        }
    }
    TaggedListResult { items, errors }
}

#[tauri::command]
pub fn tools_list_all() -> TaggedListResult<crate::backends::capabilities::Tool> {
    collect_capability_tools(|t| t.tools_list())
}

#[tauri::command]
pub fn tools_set_enabled(backend: String, id: String, enabled: bool) -> Result<String, String> {
    let entry = backends::find_entry(&backend)
        .ok_or_else(|| format!("Unknown backend: {}", backend))?;
    let tools = entry.tools
        .ok_or_else(|| format!("{} does not support tools", backend))?;
    tools.tools_set_enabled(&id, enabled)
}

use crate::backends::capabilities::HooksCapability;

fn collect_capability_hooks<T, F>(f: F) -> TaggedListResult<T>
where
    T: Send,
    F: Fn(&dyn HooksCapability) -> Result<Vec<T>, String> + Sync + Send,
{
    use rayon::prelude::*;
    let results: Vec<_> = backends::entries().par_iter()
        .filter_map(|e| e.hooks.map(|h| (e, h)))
        .map(|(e, h)| {
            if !e.backend.is_installed() {
                return (e.backend.id().to_string(), None, None);
            }
            match f(h) {
                Ok(v) => (e.backend.id().to_string(), Some(v), None),
                Err(err) => (e.backend.id().to_string(), None, Some(BackendError {
                    backend: e.backend.id().to_string(), message: err,
                })),
            }
        }).collect();
    let mut items = Vec::new();
    let mut errors = Vec::new();
    for (id, val, err) in results {
        if let Some(e) = err { errors.push(e); }
        if let Some(v) = val {
            for item in v {
                items.push(TaggedItem { backend: id.clone(), item });
            }
        }
    }
    TaggedListResult { items, errors }
}

#[tauri::command]
pub fn hooks_list_all() -> TaggedListResult<crate::backends::capabilities::Hook> {
    collect_capability_hooks(|h| h.hooks_list())
}

#[tauri::command]
pub fn hooks_set_enabled(backend: String, id: String, enabled: bool) -> Result<String, String> {
    let entry = backends::find_entry(&backend)
        .ok_or_else(|| format!("Unknown backend: {}", backend))?;
    let hooks = entry.hooks
        .ok_or_else(|| format!("{} does not support hooks", backend))?;
    hooks.hooks_set_enabled(&id, enabled)
}

fn collect_capability_memory<T, F>(f: F) -> TaggedListResult<T>
where
    T: Send,
    F: Fn(&dyn MemoryCapability) -> Result<Vec<T>, String> + Sync + Send,
{
    use rayon::prelude::*;
    let results: Vec<_> = backends::entries().par_iter()
        .filter_map(|e| e.memory.map(|m| (e, m)))
        .map(|(e, m)| {
            if !e.backend.is_installed() {
                return (e.backend.id().to_string(), None, None);
            }
            match f(m) {
                Ok(v) => (e.backend.id().to_string(), Some(v), None),
                Err(err) => (e.backend.id().to_string(), None, Some(BackendError {
                    backend: e.backend.id().to_string(), message: err,
                })),
            }
        }).collect();
    let mut items = Vec::new();
    let mut errors = Vec::new();
    for (id, val, err) in results {
        if let Some(e) = err { errors.push(e); }
        if let Some(v) = val {
            for item in v {
                items.push(TaggedItem { backend: id.clone(), item });
            }
        }
    }
    TaggedListResult { items, errors }
}

#[tauri::command]
pub fn memory_status_all() -> TaggedListResult<crate::backends::capabilities::MemoryStatus> {
    collect_capability_memory(|m| m.memory_status().map(|s| vec![s]))
}

#[tauri::command]
pub fn memory_index(backend: String) -> Result<String, String> {
    let entry = backends::find_entry(&backend)
        .ok_or_else(|| format!("Unknown backend: {}", backend))?;
    let mem = entry.memory
        .ok_or_else(|| format!("{} does not support memory", backend))?;
    mem.memory_index()
}

#[tauri::command]
pub fn memory_reset(backend: String) -> Result<String, String> {
    let entry = backends::find_entry(&backend)
        .ok_or_else(|| format!("Unknown backend: {}", backend))?;
    let mem = entry.memory
        .ok_or_else(|| format!("{} does not support memory", backend))?;
    mem.memory_reset()
}

#[derive(Serialize)]
pub struct Stats {
    pub gateway_running: bool,
    pub usage: Option<serde_json::Value>,
    pub health: Option<serde_json::Value>,
}

#[tauri::command]
pub fn get_stats(days: Option<u32>) -> Stats {
    let days_str = days.unwrap_or(30).to_string();
    let usage = backends::openclaw::openclaw_json(
        &["gateway", "usage-cost", "--days", &days_str, "--json"],
    ).ok();
    let health = backends::openclaw::openclaw_json(&["health", "--json"]).ok();
    Stats {
        gateway_running: health.is_some() || usage.is_some(),
        usage, health,
    }
}