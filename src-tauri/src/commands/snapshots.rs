//! 配置快照命令。薄封装:逻辑在 `crate::sync::snapshots`(home 参数化
//! 可测)。restore 会写 ClawBox config(清记账),持 CONFIG_LOCK 与用户
//! 同时触发的 config 命令互斥。

use crate::commands::config::{real_home, CONFIG_LOCK};
use crate::sync::snapshots::{self, RestoreResult, SnapshotInfo};

/// 快照列表。agent_id=None 列全部 agent,按时间倒序。
#[tauri::command]
pub async fn snapshots_list(agent_id: Option<String>) -> Result<Vec<SnapshotInfo>, String> {
    let home = real_home();
    Ok(snapshots::list(&home, agent_id.as_deref()))
}

/// 恢复到指定快照时刻:还原文件 + 清对应维度托管记账(见 snapshots::restore)。
#[tauri::command]
pub async fn snapshots_restore(
    agent_id: String,
    snapshot_id: String,
) -> Result<RestoreResult, String> {
    let _guard = CONFIG_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let home = real_home();
    snapshots::restore(&home, &agent_id, &snapshot_id)
}
