//! `doctor_run` 薄封装:加载 config(读锁)+ agent 状态,拼三组 checks。
//! 全部逻辑在 `crate::doctor`,命令层不做事。

use crate::commands::config::{load_config, real_home, CONFIG_LOCK};
use crate::doctor::{backend_checks, local_checks, network_checks, DoctorReport};

#[tauri::command]
pub async fn doctor_run() -> Result<DoctorReport, String> {
    // 读锁:与写路径互斥,加载期间 config 不会被改一半。
    let (config, statuses) = {
        let _guard = CONFIG_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let config = load_config(&real_home())?;
        let statuses = crate::agents::list_agent_status();
        (config, statuses)
    };
    let mut checks = local_checks(&real_home(), &config, &statuses);
    checks.extend(network_checks(&config).await);
    checks.extend(backend_checks());
    Ok(DoctorReport {
        checks,
        ran_at: time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_default(),
    })
}
