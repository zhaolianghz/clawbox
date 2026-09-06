//! Cline adapter — placeholder.
//!
//! Cline 是 VS Code 扩展 + CLI 双形态。VS Code 扩展版在 UI 里显示 token
//! 但不落本地日志;CLI 版 (`cline --json`) 把每条消息写到 stdout,本
//! 地无持久化存储。要统计需要在 `~/.cline/data/` 加 hook 或注入中间件。
//!
//! 因此 `available()` 永远返回 false,UI 上以"无本地数据"形式列出。

use crate::usage::{UsageError, UsageProvider, UsageScan};
use std::path::Path;

pub struct ClineUsageProvider;

impl UsageProvider for ClineUsageProvider {
    fn agent_id(&self) -> &'static str {
        "cline"
    }

    fn available(&self, _home: &Path) -> bool {
        false
    }

    fn scan(&self, _home: &Path) -> Result<UsageScan, UsageError> {
        Ok(UsageScan {
            agent_id: "cline".into(),
            ..Default::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn always_unavailable() {
        let tmp = std::env::temp_dir();
        assert!(!ClineUsageProvider.available(&tmp));
        let scan = ClineUsageProvider.scan(&tmp).unwrap();
        assert_eq!(scan.events.len(), 0);
    }
}
