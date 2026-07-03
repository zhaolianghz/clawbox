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
pub fn gateway_status_all() -> Vec<crate::backends::GatewayStatus> {
    let (pairs, _errors) = collect_backends(|b| b.gateway_status());
    pairs.into_iter().map(|(_, s)| s).collect()
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