//! Tauri command layer for ACP adapters + multi-agent code review.

use crate::acp::adapters::{list_adapter_info, AdapterInfo};
use crate::acp::review::{
    list_reports, load_report, now_secs, run_review, save_report, ReviewReport, ReviewScope,
    ReviewStatus, ReviewTask, RoleAssignment,
};

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
    // `npm install -g` can take minutes; keep it off the main thread.
    tauri::async_runtime::spawn_blocking(move || {
        let def = crate::agents::find_agent(&id)
            .filter(|a| a.kind == crate::agents::AgentKind::AcpBridge)
            .ok_or_else(|| format!("unknown adapter: {}", id))?;
        crate::agents::install::run_install(def)
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
