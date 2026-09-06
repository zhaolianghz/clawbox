//! Cursor Agent adapter — placeholder.
//!
//! Cursor CLI / Cursor IDE 的 token 数据不在本地(走 Cursor 自身的 usage
//! dashboard,需要 admin API 才能批量拉取)。本地 `~/.cursor/projects/<cwd>/`
//! 下的 agent-transcripts 只有 `{role, message}`,无 token 字段。
//!
//! 因此 `available()` 永远返回 false,UI 上以"无本地数据"形式列出。
//! 后续可考虑接 Cursor Admin Usage Events API(需要 admin token)。

use crate::usage::{UsageError, UsageProvider, UsageScan};
use std::path::Path;

pub struct CursorAgentUsageProvider;

impl UsageProvider for CursorAgentUsageProvider {
    fn agent_id(&self) -> &'static str {
        "cursor-agent"
    }

    fn available(&self, _home: &Path) -> bool {
        false
    }

    fn scan(&self, _home: &Path) -> Result<UsageScan, UsageError> {
        Ok(UsageScan {
            agent_id: "cursor-agent".into(),
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
        assert!(!CursorAgentUsageProvider.available(&tmp));
        let scan = CursorAgentUsageProvider.scan(&tmp).unwrap();
        assert_eq!(scan.events.len(), 0);
    }
}
