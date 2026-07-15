//! Tauri command layer for ACP adapters + multi-agent code review.

use crate::acp::adapters::{find_adapter, list_adapter_info, AdapterInfo};
use crate::acp::review::{
    list_reports, load_report, now_secs, run_review, save_report, ReviewReport, ReviewScope,
    ReviewStatus, ReviewTask, RoleAssignment,
};
use std::process::Command;

#[tauri::command(async)]
pub async fn acp_list_adapters() -> Vec<AdapterInfo> {
    // list_adapter_info shells out to probe adapter binaries — keep it off the
    // main thread so the UI never blocks.
    tauri::async_runtime::spawn_blocking(list_adapter_info)
        .await
        .unwrap_or_default()
}

#[tauri::command(async)]
pub async fn acp_install_adapter(id: String) -> Result<String, String> {
    // `npm install -g` can take minutes; run it on a blocking thread so the
    // main thread (and thus the whole UI) stays responsive.
    tauri::async_runtime::spawn_blocking(move || {
        let adapter = find_adapter(&id).ok_or_else(|| format!("unknown adapter: {}", id))?;
        // install_hint form: "npm install -g <pkg>" (possibly with --force)
        let parts: Vec<&str> = adapter.install_hint.split_whitespace().collect();
        if parts.is_empty() || parts[0] != "npm" {
            return Err(format!("unsupported install hint: {}", adapter.install_hint));
        }
        let out = Command::new(parts[0])
            .args(&parts[1..])
            .output()
            .map_err(|e| format!("failed to run npm: {}", e))?;
        if out.status.success() {
            Ok(format!("Installed {}", adapter.label))
        } else {
            Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn review_run(
    project_path: String,
    scope: ReviewScope,
    reviewers: Vec<RoleAssignment>,
    summarizer: RoleAssignment,
) -> Result<ReviewReport, String> {
    if reviewers.is_empty() {
        return Err("at least one reviewer is required".into());
    }
    let task = ReviewTask {
        id: format!("review_{}", now_secs()),
        project_path,
        scope,
        reviewers,
        summarizer,
        created_at: now_secs(),
    };
    let task_id = task.id.clone();
    match run_review(task).await {
        Ok(report) => Ok(report),
        Err(e) => {
            // run_review propagates errors without persisting anything; persist a
            // failed report (best-effort) so the failure shows up in review_list.
            let failed = ReviewReport {
                task_id,
                findings: vec![],
                summary: String::new(),
                status: ReviewStatus::Failed { message: e.clone() },
                created_at: now_secs(),
            };
            let _ = save_report(&failed);
            Err(e)
        }
    }
}

#[tauri::command]
pub fn review_list() -> Vec<ReviewReport> {
    list_reports()
}

#[tauri::command]
pub fn review_get(task_id: String) -> Result<ReviewReport, String> {
    load_report(&task_id)
}
