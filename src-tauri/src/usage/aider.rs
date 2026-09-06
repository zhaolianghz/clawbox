//! Aider adapter — placeholder.
//!
//! Aider 默认不落本地 token 日志,除非用户在启动时加 `--llm-history-file`
//! 才会把每条 LLM request/response 写到 raw JSON 文件里(路径用
//! `AIDER_LLM_HISTORY_FILE` 环境变量自定义)。ClawBox 检测该文件存在时
//! 仍能解析(参考 ccusage / token-monitor 文档),但默认配置下不可用。
//!
//! 因此 `available()` 永远返回 false,UI 上以"无本地数据"形式列出。

use crate::usage::{UsageError, UsageProvider, UsageScan};
use std::path::Path;

pub struct AiderUsageProvider;

impl UsageProvider for AiderUsageProvider {
    fn agent_id(&self) -> &'static str {
        "aider"
    }

    fn available(&self, _home: &Path) -> bool {
        // 默认 false:aider 需主动加 --llm-history-file 才有数据。
        // 后续可探测 AIDER_LLM_HISTORY_FILE 环境变量或项目根 .aider.llm.history
        // 文件,这里先按"无本地日志"处理。
        false
    }

    fn scan(&self, _home: &Path) -> Result<UsageScan, UsageError> {
        Ok(UsageScan {
            agent_id: "aider".into(),
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
        assert!(!AiderUsageProvider.available(&tmp));
        let scan = AiderUsageProvider.scan(&tmp).unwrap();
        assert_eq!(scan.events.len(), 0);
    }
}
