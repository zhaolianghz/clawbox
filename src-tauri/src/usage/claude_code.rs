//! Claude Code adapter:解析 `~/.claude/projects/**/*.jsonl`。
//!
//! 形状提取(避免紧耦合 schema):每行 JSON,只看 `type == "assistant"`
//! 且 `message.role == "assistant"` 且 `message.usage` 非空。其它行一律
//! 跳过。模型字段从 `message.model` 取,空则归 "unknown"。
//!
//! 去重: 内存 HashSet 按 `(session_id, message.id)` 去重 — 同一 message
//! 在 sidechain 多次出现只算一次(借鉴 ccusage 成熟口径)。

/// Claude Code adapter 占位 — 真实实现在 Task 3 提交替换。
pub struct ClaudeCodeUsageProvider;

impl crate::usage::UsageProvider for ClaudeCodeUsageProvider {
    fn agent_id(&self) -> &'static str { "claude-code" }
    fn available(&self, _home: &std::path::Path) -> bool { false }
    fn scan(&self, _home: &std::path::Path) -> Result<crate::usage::UsageScan, crate::usage::UsageError> {
        Ok(crate::usage::UsageScan { agent_id: "claude-code".into(), ..Default::default() })
    }
}
