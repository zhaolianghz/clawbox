//! Codex adapter:解析 `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl`。
//!
//! **差值口径**: Codex 的 `event_msg.type == "token_count"` 给的是
//! 累积 `total_token_usage`,而非单次 turn 增量。维护文件内 last_total
//! 快照,差值即本次 turn 增量;首个事件 last_total = 0,差值 = total。
//!
//! 模型:从 `turn_context.payload.model` 取,缺失则归 "unknown"。
//! 跨 turn 共享同一个 turn_context,后续 token_count 都用该 model。

/// Codex adapter 占位 — 真实实现在 Task 4 提交替换。
pub struct CodexUsageProvider;

impl crate::usage::UsageProvider for CodexUsageProvider {
    fn agent_id(&self) -> &'static str { "codex" }
    fn available(&self, _home: &std::path::Path) -> bool { false }
    fn scan(&self, _home: &std::path::Path) -> Result<crate::usage::UsageScan, crate::usage::UsageError> {
        Ok(crate::usage::UsageScan { agent_id: "codex".into(), ..Default::default() })
    }
}
