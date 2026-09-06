//! DeepSeek Harness (dsh) adapter — placeholder.
//!
//! dsh 的会话存到 `~/.dsh/sessions/<cwd>/<session_id>/session.jsonl.zstd`,
//! 但文件里只记执行轨迹(message/tool/permission 等事件),token 数字由
//! 订阅插件远程拉(ChatGPT/Claude/Grok 各家 usage API),本地不持久化。
//!
//! 因此 `available()` 永远返回 false,UI 上以"无本地数据"形式列出。

use crate::usage::{UsageError, UsageProvider, UsageScan};
use std::path::Path;

pub struct DshUsageProvider;

impl UsageProvider for DshUsageProvider {
    fn agent_id(&self) -> &'static str {
        "dsh"
    }

    fn available(&self, _home: &Path) -> bool {
        false
    }

    fn scan(&self, _home: &Path) -> Result<UsageScan, UsageError> {
        Ok(UsageScan {
            agent_id: "dsh".into(),
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
        assert!(!DshUsageProvider.available(&tmp));
        let scan = DshUsageProvider.scan(&tmp).unwrap();
        assert_eq!(scan.events.len(), 0);
    }
}
