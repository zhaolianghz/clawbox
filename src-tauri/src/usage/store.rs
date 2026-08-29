//! Append-only 月桶 `~/.clawbox/usage/usage-YYYY-MM.json`。
//!
//! 核心:与原始 JSONL 格式解耦,抗 30 天会话清理。
//!
//! `agent_to_provider_at_scan` 在每次扫描时按当时 `Config.agent_providers`
//! 快照落库 — 后续 binding 变化不影响历史桶。

//! Store 占位 — 真实实现在 Task 5 提交。
