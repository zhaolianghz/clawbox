use rayon::prelude::*;
use serde::Serialize;

use crate::backends::{self, Backend, BackendInfo};
use crate::backends::capabilities::MemoryCapability;

#[derive(Serialize)]
pub struct BackendError {
    pub backend: String,
    pub message: String,
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

fn collect_backends<F, T>(f: F) -> (Vec<(String, T)>, Vec<BackendError>)
where
    F: Fn(&dyn Backend) -> Result<T, String> + Sync + Send,
    T: Send,
{
    let results: Vec<_> = backends::backends().par_iter()
        .map(|b| {
            let installed = b.is_installed();
            let id = b.id().to_string();
            if !installed { return (id, None, None); }
            match f(b.as_ref()) {
                Ok(v) => (id, Some(v), None),
                Err(e) => (id.clone(), None, Some(BackendError {
                    backend: id, message: e,
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
pub async fn list_backends() -> Vec<BackendInfo> {
    backends::backends().iter().map(|b| BackendInfo {
        id: b.id().to_string(),
        display_name: b.display_name().to_string(),
        version: b.version(),
        installed: b.is_installed(),
    }).collect()
}

#[tauri::command]
pub async fn gateway_status_all() -> GatewayStatusAllResult {
    let (pairs, errors) = collect_backends(|b| b.gateway_status());
    let statuses = pairs.into_iter().map(|(backend, status)| TaggedGatewayStatus {
        backend, status,
    }).collect();
    GatewayStatusAllResult { statuses, errors }
}

fn collect_capability_memory<T, F>(f: F) -> TaggedListResult<T>
where
    T: Send,
    F: Fn(&dyn MemoryCapability) -> Result<Vec<T>, String> + Sync + Send,
{
    let results: Vec<_> = backends::entries().par_iter()
        .filter_map(|e| e.memory.map(|m| (e, m)))
        .map(|(e, m)| {
            let installed = e.backend.is_installed();
            let id = e.backend.id().to_string();
            if !installed { return (id, None, None); }
            match f(m) {
                Ok(v) => (id, Some(v), None),
                Err(err) => (id.clone(), None, Some(BackendError {
                    backend: id, message: err,
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
pub async fn memory_status_all() -> TaggedListResult<crate::backends::capabilities::MemoryStatus> {
    collect_capability_memory(|m| m.memory_status().map(|s| vec![s]))
}

#[tauri::command]
pub async fn memory_index(backend: String) -> Result<String, String> {
    let entry = backends::find_entry(&backend)
        .ok_or_else(|| format!("Unknown backend: {}", backend))?;
    let mem = entry.memory
        .ok_or_else(|| format!("{} does not support memory", backend))?;
    mem.memory_index()
}

#[tauri::command]
pub async fn memory_reset(backend: String) -> Result<String, String> {
    let entry = backends::find_entry(&backend)
        .ok_or_else(|| format!("Unknown backend: {}", backend))?;
    let mem = entry.memory
        .ok_or_else(|| format!("{} does not support memory", backend))?;
    mem.memory_reset()
}
