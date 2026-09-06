//! Trae Agent adapter — placeholder.
//!
//! trae-agent (bytedance/trae-agent) 是源码安装(uv sync),不发布 npm
//! 二进制。运行时只记会话执行轨迹,不持久化每 turn 的 token 数字
//! (token 由用户在 yaml 配置里提供上限,实际计数由模型 provider 决定,
//! trae 这边不聚合)。
//!
//! 因此 `available()` 永远返回 false,UI 上以"无本地数据"形式列出。

use crate::usage::{UsageError, UsageProvider, UsageScan};
use std::path::Path;

pub struct TraeAgentUsageProvider;

impl UsageProvider for TraeAgentUsageProvider {
    fn agent_id(&self) -> &'static str {
        "trae-agent"
    }

    fn available(&self, _home: &Path) -> bool {
        false
    }

    fn scan(&self, _home: &Path) -> Result<UsageScan, UsageError> {
        Ok(UsageScan {
            agent_id: "trae-agent".into(),
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
        assert!(!TraeAgentUsageProvider.available(&tmp));
        let scan = TraeAgentUsageProvider.scan(&tmp).unwrap();
        assert_eq!(scan.events.len(), 0);
    }
}
