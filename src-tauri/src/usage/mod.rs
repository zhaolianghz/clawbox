//! Token 用量统计 — 本机各 agent CLI 的真实 token 消耗。
//!
//! 设计：`UsageProvider` trait 每 agent 一个 adapter(Claude Code / Codex),
//! 扫描各自本地 JSONL/会话日志,按天按 agent 按模型聚合,落 ClawBox 自有
//! 存储(`~/.clawbox/usage/`),与原始格式解耦,抗 30 天会话清理。
//!
//! Spec: `docs/superpowers/specs/2026-08-29-token-usage-design.md`
//! 数据源容错策略: 逐行解析 + 形状提取 + 容错 + 故障隔离(各 adapter 独立
//! try-catch),matched_ratio 低于 0.8 → UI 黄条提示。

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub mod claude_code;
pub mod codex;
pub mod store;
pub mod aggregate;

/// 单个 token 用量事件(已按 provider 计费口径拆分,前端按 4 列展示)。
///
/// `input_tokens` = 新输入(不计 cache)
/// `cache_read_tokens` = 缓存命中读(Anthropic 计费按 cache_read 单价)
/// `cache_creation_tokens` = 缓存写入(Anthropic 计费按 cache_write 单价;
///                              Codex 无此分项,记 0)
/// `output_tokens` = 模型输出(Codex 已含 reasoning_output_tokens)
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageEvent {
    /// 事件时间(RFC3339 字符串,跟项目其它模块统一,不引入 chrono)。
    pub ts: String,
    /// 跨文件去重键。
    pub session_id: String,
    /// 事件 id(Claude: message.id; Codex: 自合成的 turn 序号)。
    pub event_id: String,
    /// 模型 id,未识别归 "unknown"。
    pub model: String,
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub cache_read_tokens: u64,
    #[serde(default)]
    pub cache_creation_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
}

/// 解析过程的统计 — UI matched_ratio 低于 0.8 时黄条提示「格式可能已变更」。
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParseStats {
    pub files_scanned: usize,
    pub files_skipped: usize,
    pub lines_total: usize,
    pub lines_matched: usize,
    pub lines_skipped: usize,
}

impl ParseStats {
    pub fn matched_ratio(&self) -> f64 {
        if self.lines_total == 0 {
            return 1.0; // 空文件 = 完美匹配(无可挑剔)
        }
        self.lines_matched as f64 / self.lines_total as f64
    }

    /// 把另一个 stats 累加进来(多文件扫描时)。
    pub fn add(&mut self, other: &ParseStats) {
        self.files_scanned += other.files_scanned;
        self.files_skipped += other.files_skipped;
        self.lines_total += other.lines_total;
        self.lines_matched += other.lines_matched;
        self.lines_skipped += other.lines_skipped;
    }
}

/// 单个 adapter 一次扫描的全部产物。`events` 可能为空(没匹配 / 文件被删),
/// 失败永远记 `stats`,不抛错(逐行容错)。
#[derive(Clone, Debug, Default)]
pub struct UsageScan {
    pub agent_id: String,
    pub events: Vec<UsageEvent>,
    pub stats: ParseStats,
}

/// 扫描过程的不可恢复错误(目录权限、IO 整体失败等)。逐行解析失败不算
/// 错误,只记 stats,符合「降级可见而非静默丢失」原则。
#[derive(Debug)]
pub struct UsageError {
    pub agent_id: String,
    pub kind: String,
    pub message: String,
}

impl std::fmt::Display for UsageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}: {}", self.agent_id, self.kind, self.message)
    }
}

impl std::error::Error for UsageError {}

impl UsageError {
    pub fn new(agent_id: &str, kind: &str, message: impl Into<String>) -> Self {
        Self {
            agent_id: agent_id.to_string(),
            kind: kind.to_string(),
            message: message.into(),
        }
    }
}

/// 各 agent 的本地会话根目录,默认按 macOS/Linux 的 $HOME/.{claude,codex}。
/// home 参数化便于 TempHome 测试。
pub trait UsageProvider: Send + Sync {
    /// 稳定标识(对应 AgentStatus.id,如 "claude-code"、"codex")。
    fn agent_id(&self) -> &'static str;

    /// 是否在本机可用(检查该 agent 的 home 目录是否存在)。不可用
    /// 的 provider 在 `all_providers()` 中仍会被列出,扫描时直接返回
    /// 空 events + 0 stats(调用方不必过滤)。
    fn available(&self, home: &Path) -> bool;

    /// 扫描所有相关文件 → 一组 UsageEvent。
    ///
    /// 容错契约:
    /// - 单文件不存在 → 跳过,记 stats.files_skipped
    /// - 单行解析失败 → 跳过,记 stats.lines_skipped
    /// - 整目录不可访问 / 致命错误 → 返回 Err(UsageError)
    fn scan(&self, home: &Path) -> Result<UsageScan, UsageError>;
}

/// 所有 provider 列表(v1: Claude Code + Codex;后续 agent 留位)。
pub fn all_providers() -> Vec<Box<dyn UsageProvider>> {
    vec![
        Box::new(claude_code::ClaudeCodeUsageProvider),
        Box::new(codex::CodexUsageProvider),
    ]
}

/// home 下的 ClawBox 用量目录 `~/.clawbox/usage/`。与 snapshots/ 同级,
/// 遵循项目既有的 home 相对布局约定(见 `clawbox_config_path`)。
pub fn usage_dir(home: &Path) -> PathBuf {
    home.join(".clawbox").join("usage")
}

/// 单个月桶文件路径 `usage-YYYY-MM.json`。append-only 桶。
pub fn month_bucket_path(home: &Path, year: i32, month: u8) -> PathBuf {
    usage_dir(home).join(format!("usage-{:04}-{:02}.json", year, month))
}

/// 增量缓存路径 `cache.json`。key = 绝对路径,value = (size, mtime_ms, last_event_id, consumed_total)。
pub fn cache_path(home: &Path) -> PathBuf {
    usage_dir(home).join("cache.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matched_ratio_handles_zero_lines() {
        let mut s = ParseStats::default();
        assert_eq!(s.matched_ratio(), 1.0);
        s.lines_total = 10;
        s.lines_matched = 8;
        assert!((s.matched_ratio() - 0.8).abs() < 1e-9);
    }

    #[test]
    fn stats_add_accumulates() {
        let mut a = ParseStats {
            files_scanned: 1,
            files_skipped: 0,
            lines_total: 100,
            lines_matched: 80,
            lines_skipped: 20,
        };
        let b = ParseStats {
            files_scanned: 2,
            files_skipped: 1,
            lines_total: 50,
            lines_matched: 40,
            lines_skipped: 10,
        };
        a.add(&b);
        assert_eq!(a.files_scanned, 3);
        assert_eq!(a.files_skipped, 1);
        assert_eq!(a.lines_total, 150);
        assert_eq!(a.lines_matched, 120);
        assert_eq!(a.lines_skipped, 30);
    }

    #[test]
    fn usage_error_display_contains_context() {
        let e = UsageError::new("claude-code", "io", "permission denied");
        let s = format!("{}", e);
        assert!(s.contains("claude-code"));
        assert!(s.contains("io"));
        assert!(s.contains("permission denied"));
    }

    #[test]
    fn usage_dir_layout_matches_snapshots() {
        let home = Path::new("/tmp/x");
        assert_eq!(
            usage_dir(home),
            PathBuf::from("/tmp/x/.clawbox/usage")
        );
        assert_eq!(
            month_bucket_path(home, 2026, 8),
            PathBuf::from("/tmp/x/.clawbox/usage/usage-2026-08.json")
        );
        assert_eq!(
            cache_path(home),
            PathBuf::from("/tmp/x/.clawbox/usage/cache.json")
        );
    }
}
