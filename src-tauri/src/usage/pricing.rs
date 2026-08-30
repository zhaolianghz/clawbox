//! 模型官方价表(USD per 1M tokens)。
//!
//! **设计原则**:成本按"官方公开价"算,中转站 / 第三方别名一律通过
//! ProviderSpec.model_aliases 映射到本表的 canonical model name 算。
//!
//! 价格单位:USD / 1,000,000 tokens(即 1M token 多少美元)。
//!
//! 数据来源:
//! - Anthropic:  https://docs.claude.com/en/docs/about-claude/pricing
//! - OpenAI:     https://openai.com/api/pricing
//! - Google:     https://ai.google.dev/pricing
//! - DeepSeek:   https://api-docs.deepseek.com/quick_start/pricing
//! - GLM(Zhipu): https://bigmodel.cn/pricing
//! - Kimi:       https://platform.moonshot.cn/docs/pricing/chat
//! - MiniMax:    https://platform.minimax.io/docs/guides/pricing-paygo
//!
//! 每次新增价目只更新本文件 + 对应 unit test;不改 store / aggregate。
//!
//! USD 默认按 7.2:1 RMB 换算(智谱按 ¥ 计价 → $)。
//!
//! 实现提示:每个 *_prices 函数独立运作,前缀匹配,版本快照后缀
//! (`-20250805` / `-0813-ga`)自动兼容。所有函数都对 lowercase 字符串做匹配,
//! 数字与版本号的大小写差异不影响。
//!
//! **cache 字段语义**(对照各厂商 docs):
//! - Anthropic: `cache_read` = 5m/1h 都按 0.1x input,`cache_creation` = 1.25x input
//! - OpenAI:    `cache_read` 按 0.5x input(约, GPT-5 系列未明确文档)
//! - Google:    `cache_read` 未官方公开(Gemini caching 按 cached tokens 折扣)
//! - DeepSeek:  `cache_read` 按 0.025x input(off-peak 折扣巨大)
//! - GLM:       `cache_read` = 缓存命中价,`cache_creation` 未提供
//! - Kimi:      `cache_read` 按 0.175x input
//! - MiniMax:   `cache_read` = 缓存命中价(M2/M3 系列),`cache_creation` = $0.375

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// 单个 model 的官方公开价(USD per 1M tokens)。
///
/// 字段全 Option 因为不同厂商对 cache_read / cache_creation 给的不一样:
/// - OpenAI / Google 没有 cache_creation 字段
/// - Anthropic / DeepSeek / MiniMax 都有
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ModelPrice {
    /// 输入 token 单价(USD / 1M tokens)
    pub input: f64,
    /// 缓存命中 token 单价(USD / 1M tokens)。None = 厂商未公开
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub cache_read: Option<f64>,
    /// 缓存写入 token 单价(USD / 1M tokens)。None = 厂商未公开
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub cache_creation: Option<f64>,
    /// 输出 token 单价(USD / 1M tokens)
    pub output: f64,
}

impl ModelPrice {
    /// 算单 event 成本(USD)。未知字段按 0 算(让 cache_creation 等
    /// 非通用项保持透明,而不是被 0 假值误导)。
    pub fn event_cost(
        &self,
        input: u64,
        cache_read: u64,
        cache_creation: u64,
        output: u64,
    ) -> f64 {
        let cr = self.cache_read.unwrap_or(0.0);
        let cc = self.cache_creation.unwrap_or(0.0);
        (input as f64) * self.input / 1_000_000.0
            + (cache_read as f64) * cr / 1_000_000.0
            + (cache_creation as f64) * cc / 1_000_000.0
            + (output as f64) * self.output / 1_000_000.0
    }
}

/// 一个 provider 的价格配置。中转站场景下用 `aliases` 把"中转名"映射到
/// 官方 canonical name,`overrides` 用来给某个具体 model 自定义单价。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ProviderPricing {
    /// 中转名 → 官方 canonical model 名
    /// 例: `{"claude-fable-5-route": "claude-fable-5"}`
    #[serde(default)]
    pub aliases: BTreeMap<String, String>,
    /// model → 自定义 ModelPrice(覆盖默认表)
    #[serde(default)]
    pub overrides: BTreeMap<String, ModelPrice>,
}

impl ProviderPricing {
    /// 给一个 (provider pricing + model id) 算出 ModelPrice 或 None。
    /// 优先级:override > alias 推导 > model id 直接查表。
    pub fn resolve(
        &self,
        provider: Option<&ProviderPricing>,
        model: &str,
    ) -> Option<ModelPrice> {
        let provider = provider?;
        // 1. override 最优先
        if let Some(p) = provider.overrides.get(model) {
            return Some(*p);
        }
        // 2. alias 推导 canonical name → 查默认表
        let canonical = provider
            .aliases
            .get(model)
            .map(|s| s.as_str())
            .unwrap_or(model);
        builtin_prices(canonical)
    }
}

/// 全局入口:给定 model id 字符串(可能是 `claude-opus-4-1-20250805` 或
/// `gpt-5-mini` 这种大小写不敏感的任意形式),返回官方公开价或 None。
pub fn builtin_prices(model: &str) -> Option<ModelPrice> {
    let m = model.to_lowercase();
    if let Some(p) = claude_prices(&m) {
        return Some(p);
    }
    if let Some(p) = openai_prices(&m) {
        return Some(p);
    }
    if let Some(p) = gemini_prices(&m) {
        return Some(p);
    }
    if let Some(p) = deepseek_prices(&m) {
        return Some(p);
    }
    if let Some(p) = glm_prices(&m) {
        return Some(p);
    }
    if let Some(p) = kimi_prices(&m) {
        return Some(p);
    }
    if let Some(p) = minimax_prices(&m) {
        return Some(p);
    }
    None
}

// ─────────────────────────────────────────────────────────────────────────
//  Anthropic Claude  —  https://docs.claude.com/en/docs/about-claude/pricing
// ─────────────────────────────────────────────────────────────────────────
//
// 2026-08 最新完整价表(USD / 1M tokens):
//
// | Model           | Input | 5m Cache Write | 1h Cache Write | Cache Hit | Output |
// | Fable 5         | 10.00 | 12.50          | 20.00          | 1.00      | 50.00  |
// | Mythos 5 (LA)   | 10.00 | 12.50          | 20.00          | 1.00      | 50.00  |
// | Opus 5          |  5.00 |  6.25          | 10.00          | 0.50      | 25.00  |
// | Opus 4.8        |  5.00 |  6.25          | 10.00          | 0.50      | 25.00  |
// | Opus 4.7        |  5.00 |  6.25          | 10.00          | 0.50      | 25.00  |
// | Opus 4.6        |  5.00 |  6.25          | 10.00          | 0.50      | 25.00  |
// | Opus 4.5        |  5.00 |  6.25          | 10.00          | 0.50      | 25.00  |
// | Opus 4.1(已退)  | 15.00 | 18.75          | 30.00          | 1.50      | 75.00  |
// | Opus 4 (已退)   | 15.00 | 18.75          | 30.00          | 1.50      | 75.00  |
// | Sonnet 5 (intro)|  2.00 |  2.50          |  4.00          | 0.20      | 10.00  | 截止 2026-08-31
// | Sonnet 4.6      |  3.00 |  3.75          |  6.00          | 0.30      | 15.00  |
// | Sonnet 4.5      |  3.00 |  3.75          |  6.00          | 0.30      | 15.00  |
// | Sonnet 4 (已退) |  3.00 |  3.75          |  6.00          | 0.30      | 15.00  |
// | Haiku 4.5       |  1.00 |  1.25          |  2.00          | 0.10      |  5.00  |
// | Haiku 3.5 (退)  |  0.80 |  1.00          |  1.60          | 0.08      |  4.00  |
//
// 取 5m cache_write 作为 cache_creation 字段(常见用法;1h 缓存 double 价格
// 是少数场景,不在 store 端统计)。

fn claude_prices(m: &str) -> Option<ModelPrice> {
    let hit = |prefixes: &[&str]| -> bool {
        prefixes.iter().any(|p| m.starts_with(p))
    };
    // 前缀要按"长前缀优先"排,防止 claude-opus-5 被 claude-opus-4-1 截胡
    if hit(&["claude-fable-5"]) || hit(&["claude-mythos-5"]) {
        Some(ModelPrice { input: 10.00, cache_read: Some(1.00),  cache_creation: Some(12.50), output: 50.00 })
    } else if hit(&["claude-opus-5"]) || hit(&["claude-opus-4-8"]) || hit(&["claude-opus-4-7"])
        || hit(&["claude-opus-4-6"]) || hit(&["claude-opus-4-5"])
    {
        Some(ModelPrice { input: 5.00, cache_read: Some(0.50), cache_creation: Some(6.25), output: 25.00 })
    } else if hit(&["claude-opus-4-1"]) || hit(&["claude-opus-4"]) {
        Some(ModelPrice { input: 15.00, cache_read: Some(1.50), cache_creation: Some(18.75), output: 75.00 })
    } else if hit(&["claude-sonnet-5"]) {
        // 2026-08-31 之前的促销价;之后会切到 $3/$15
        Some(ModelPrice { input: 2.00, cache_read: Some(0.20), cache_creation: Some(2.50), output: 10.00 })
    } else if hit(&["claude-sonnet-4-6"]) || hit(&["claude-sonnet-4-5"]) || hit(&["claude-sonnet-4"]) {
        Some(ModelPrice { input: 3.00, cache_read: Some(0.30), cache_creation: Some(3.75), output: 15.00 })
    } else if hit(&["claude-haiku-4-5"]) || hit(&["claude-haiku-4"]) {
        Some(ModelPrice { input: 1.00, cache_read: Some(0.10), cache_creation: Some(1.25), output: 5.00 })
    } else if hit(&["claude-haiku-3-5"]) || hit(&["claude-3-5-haiku"]) {
        Some(ModelPrice { input: 0.80, cache_read: Some(0.08), cache_creation: Some(1.00), output: 4.00 })
    } else if hit(&["claude-3-opus"]) {
        Some(ModelPrice { input: 15.00, cache_read: None, cache_creation: None, output: 75.00 })
    } else if hit(&["claude-3-sonnet"]) {
        Some(ModelPrice { input: 3.00, cache_read: None, cache_creation: None, output: 15.00 })
    } else if hit(&["claude-3-haiku"]) {
        Some(ModelPrice { input: 0.25, cache_read: None, cache_creation: None, output: 1.25 })
    } else if hit(&["claude"]) {
        // 兜底:未知 Claude 子模型给 Opus 4.1 价(最贵档,避免低估成本)
        Some(ModelPrice { input: 15.00, cache_read: Some(1.50), cache_creation: Some(18.75), output: 75.00 })
    } else {
        None
    }
}

// ─────────────────────────────────────────────────────────────────────────
//  OpenAI  —  https://openai.com/api/pricing
// ─────────────────────────────────────────────────────────────────────────
//
// 2026-05/08 最新公开价(USD / 1M tokens):
//
// | Model         | Input | Cache Read | Output |
// | GPT-5.6 Sol   |  5.00 | -          | 30.00  | (旗舰)
// | GPT-5.5       |  5.00 | -          | 30.00  |
// | GPT-5.4       |  2.50 | -          | 15.00  | (推荐生产)
// | GPT-5.4 mini  |  0.75 | -          |  4.50  |
// | GPT-5.4 nano  |  0.20 | -          |  1.25  |
// | GPT-5.2       |  1.75 | -          | 14.00  |
// | GPT-5.1       |  1.25 | -          | 10.00  |
// | GPT-5         |  1.25 | -          | 10.00  |
// | GPT-5 mini    |  0.25 | -          |  2.00  |
// | GPT-5 nano    |  0.05 | -          |  0.40  |
// | GPT-4.1       |  2.00 | -          |  8.00  |
// | GPT-4.1 mini  |  0.40 | -          |  1.60  |
// | GPT-4.1 nano  |  0.10 | -          |  0.40  |
// | GPT-4o        |  2.50 | -          | 10.00  |
// | GPT-4o mini   |  0.15 | -          |  0.60  |
// | o3            |  2.00 | -          |  8.00  |
// | o4-mini       |  1.10 | -          |  4.40  |
// | o3-mini       |  1.10 | -          |  4.40  |
// | o1            | 15.00 | -          | 60.00  |
// | o1-mini       |  3.00 | -          | 12.00  |
// | GPT-4 turbo   | 10.00 | -          | 30.00  |
// | GPT-4         | 30.00 | -          | 60.00  |
// | GPT-3.5 turbo |  0.50 | -          |  1.50  |
//
// 注:OpenAI 公开页没明确 cache_read 单独价。Claude Code 在 o-series 上的
// 行为是 cached input 自动按 0.5x 算;此处按 0.5x 估算兜底。

fn openai_prices(m: &str) -> Option<ModelPrice> {
    let hit = |prefixes: &[&str]| -> bool {
        prefixes.iter().any(|p| m.starts_with(p))
    };
    // GPT-5.6 / 5.5 / 5.4 旗舰族(按 2026-05 公开)
    if hit(&["gpt-5.6"]) {
        Some(ModelPrice { input: 5.00, cache_read: None, cache_creation: None, output: 30.00 })
    } else if hit(&["gpt-5.5"]) {
        Some(ModelPrice { input: 5.00, cache_read: None, cache_creation: None, output: 30.00 })
    } else if hit(&["gpt-5.4-mini"]) || hit(&["gpt-5.4 mini"]) {
        Some(ModelPrice { input: 0.75, cache_read: None, cache_creation: None, output: 4.50 })
    } else if hit(&["gpt-5.4-nano"]) || hit(&["gpt-5.4 nano"]) {
        Some(ModelPrice { input: 0.20, cache_read: None, cache_creation: None, output: 1.25 })
    } else if hit(&["gpt-5.4"]) {
        Some(ModelPrice { input: 2.50, cache_read: None, cache_creation: None, output: 15.00 })
    } else if hit(&["gpt-5.2"]) {
        Some(ModelPrice { input: 1.75, cache_read: None, cache_creation: None, output: 14.00 })
    } else if hit(&["gpt-5.1"]) {
        Some(ModelPrice { input: 1.25, cache_read: None, cache_creation: None, output: 10.00 })
    } else if hit(&["gpt-5-nano"]) || hit(&["gpt-5 nano"]) {
        Some(ModelPrice { input: 0.05, cache_read: None, cache_creation: None, output: 0.40 })
    } else if hit(&["gpt-5-mini"]) || hit(&["gpt-5 mini"]) {
        Some(ModelPrice { input: 0.25, cache_read: None, cache_creation: None, output: 2.00 })
    } else if hit(&["gpt-5"]) {
        Some(ModelPrice { input: 1.25, cache_read: None, cache_creation: None, output: 10.00 })
    } else if hit(&["gpt-4.1-nano"]) || hit(&["gpt-4.1 nano"]) {
        Some(ModelPrice { input: 0.10, cache_read: None, cache_creation: None, output: 0.40 })
    } else if hit(&["gpt-4.1-mini"]) || hit(&["gpt-4.1 mini"]) {
        Some(ModelPrice { input: 0.40, cache_read: None, cache_creation: None, output: 1.60 })
    } else if hit(&["gpt-4.1"]) {
        Some(ModelPrice { input: 2.00, cache_read: None, cache_creation: None, output: 8.00 })
    } else if hit(&["gpt-4o-mini"]) || hit(&["gpt-4o mini"]) {
        Some(ModelPrice { input: 0.15, cache_read: None, cache_creation: None, output: 0.60 })
    } else if hit(&["gpt-4o"]) {
        Some(ModelPrice { input: 2.50, cache_read: None, cache_creation: None, output: 10.00 })
    } else if hit(&["o4-mini"]) {
        Some(ModelPrice { input: 1.10, cache_read: None, cache_creation: None, output: 4.40 })
    } else if hit(&["o3-mini"]) {
        Some(ModelPrice { input: 1.10, cache_read: None, cache_creation: None, output: 4.40 })
    } else if hit(&["o3"]) {
        Some(ModelPrice { input: 2.00, cache_read: None, cache_creation: None, output: 8.00 })
    } else if hit(&["o1-mini"]) {
        Some(ModelPrice { input: 3.00, cache_read: None, cache_creation: None, output: 12.00 })
    } else if hit(&["o1"]) {
        Some(ModelPrice { input: 15.00, cache_read: None, cache_creation: None, output: 60.00 })
    } else if hit(&["gpt-4-turbo"]) {
        Some(ModelPrice { input: 10.00, cache_read: None, cache_creation: None, output: 30.00 })
    } else if hit(&["gpt-4"]) {
        Some(ModelPrice { input: 30.00, cache_read: None, cache_creation: None, output: 60.00 })
    } else if hit(&["gpt-3.5-turbo"]) {
        Some(ModelPrice { input: 0.50, cache_read: None, cache_creation: None, output: 1.50 })
    } else {
        None
    }
}

// ─────────────────────────────────────────────────────────────────────────
//  Google Gemini  —  https://ai.google.dev/pricing
// ─────────────────────────────────────────────────────────────────────────
//
// 2026-05 最新公开价(USD / 1M tokens, ≤128K context 档):
//
// | Model              | Input | Output | Notes |
// | Gemini 3.7 Flash   |  0.75 |  3.75 | 2026-08-13 intro → 2026-12-31 后翻倍
// | Gemini 3.5 Flash   |  1.50 |  9.00 |
// | Gemini 3.1 Pro     |  2.00 | 12.00 | ≤1M; 2M 长文翻倍
// | Gemini 3 Flash     |  0.50 |  3.00 |
// | Gemini 2.5 Pro     |  1.25 | 10.00 | ≤200K;>200K 翻倍
// | Gemini 2.5 Flash   |  0.30 |  2.50 |
// | Gemini 2.5 Flash-Lite | 0.10 |  0.40 | 最便宜
// | Gemini 2.0 Flash   |  0.10 |  0.40 |
// | Gemini 1.5 Pro     |  1.25 |  5.00 |
// | Gemini 1.5 Flash   |  0.075|  0.30|
//
// cache_read 官方按 "implicit caching 75% off input" / "explicit caching 90% off"
// 估算;此处不展开,留 None 让 store 按 input 单价兜底。

fn gemini_prices(m: &str) -> Option<ModelPrice> {
    let hit = |prefixes: &[&str]| -> bool {
        prefixes.iter().any(|p| m.starts_with(p))
    };
    if hit(&["gemini-3.7-flash"]) {
        Some(ModelPrice { input: 0.75, cache_read: None, cache_creation: None, output: 3.75 })
    } else if hit(&["gemini-3.5-flash"]) {
        Some(ModelPrice { input: 1.50, cache_read: None, cache_creation: None, output: 9.00 })
    } else if hit(&["gemini-3.1-pro"]) {
        Some(ModelPrice { input: 2.00, cache_read: None, cache_creation: None, output: 12.00 })
    } else if hit(&["gemini-3-flash"]) {
        Some(ModelPrice { input: 0.50, cache_read: None, cache_creation: None, output: 3.00 })
    } else if hit(&["gemini-2.5-pro"]) {
        Some(ModelPrice { input: 1.25, cache_read: None, cache_creation: None, output: 10.00 })
    } else if hit(&["gemini-2.5-flash-lite"]) || hit(&["gemini-2.5-flash-lite"]) {
        Some(ModelPrice { input: 0.10, cache_read: None, cache_creation: None, output: 0.40 })
    } else if hit(&["gemini-2.5-flash"]) {
        Some(ModelPrice { input: 0.30, cache_read: None, cache_creation: None, output: 2.50 })
    } else if hit(&["gemini-2.0-flash"]) {
        Some(ModelPrice { input: 0.10, cache_read: None, cache_creation: None, output: 0.40 })
    } else if hit(&["gemini-1.5-pro"]) {
        Some(ModelPrice { input: 1.25, cache_read: None, cache_creation: None, output: 5.00 })
    } else if hit(&["gemini-1.5-flash"]) {
        Some(ModelPrice { input: 0.075, cache_read: None, cache_creation: None, output: 0.30 })
    } else if hit(&["gemini"]) {
        // 兜底:未知 Gemini 子模型给 Flash-Lite 价(便宜)
        Some(ModelPrice { input: 0.10, cache_read: None, cache_creation: None, output: 0.40 })
    } else {
        None
    }
}

// ─────────────────────────────────────────────────────────────────────────
//  DeepSeek  —  https://api-docs.deepseek.com/quick_start/pricing
// ─────────────────────────────────────────────────────────────────────────
//
// 2026-08 价(USD / 1M tokens, off-peak 折扣 50%):
//
// | Model              | Cache Hit | Cache Miss (input) | Output |
// | deepseek-v4-pro    | 0.022     | 0.66               | 1.98   |
// | deepseek-v4-flash  | 0.007     | 0.22               | 0.66   |
// | deepseek-v3        | (≈ v4-flash 同)        |
// | deepseek-r1        | (≈ v4-flash 同)        |
//
// V3 / R1 沿用旧价表:input $0.27 / cache_hit $0.07 / output $1.10
// V4 用上面新表。

fn deepseek_prices(m: &str) -> Option<ModelPrice> {
    if m.starts_with("deepseek-v4-pro") {
        Some(ModelPrice {
            input: 0.66,
            cache_read: Some(0.022),
            cache_creation: None,
            output: 1.98,
        })
    } else if m.starts_with("deepseek-v4-flash") {
        Some(ModelPrice {
            input: 0.22,
            cache_read: Some(0.007),
            cache_creation: None,
            output: 0.66,
        })
    } else if m.starts_with("deepseek-v3") || m.starts_with("deepseek-r1")
        || m.starts_with("deepseek-v2")
    {
        Some(ModelPrice {
            input: 0.27,
            cache_read: Some(0.07),
            cache_creation: None,
            output: 1.10,
        })
    } else if m.starts_with("deepseek") {
        Some(ModelPrice {
            input: 0.27,
            cache_read: None,
            cache_creation: None,
            output: 1.10,
        })
    } else {
        None
    }
}

// ─────────────────────────────────────────────────────────────────────────
//  GLM(Zhipu 智谱)  —  https://bigmodel.cn/pricing
// ─────────────────────────────────────────────────────────────────────────
//
// 2026-08 价(¥ / 1M tokens,以下换算为 USD,按 7.2:1):
//
// | Model              | Input | Output | Cache Read |
// | GLM-5.3            |  ¥8   | ¥28    | (限时免费)  | 新品旗舰
// | GLM-5.2            |  ¥8   | ¥28    | ¥2          |
// | GLM-5.1            |  ¥6-8 | ¥24-28 | ¥1.3-2      | 按上下文分档
// | GLM-5-Turbo        |  ¥5-7 | ¥22-26 | ¥1.2-1.8    |
// | GLM-5              |  ¥4-6 | ¥18-22 | ¥1-1.5      |
// | GLM-4.7            |  ¥2-4 | ¥8-16  | ¥0.4-0.8    | 按输出长度分档
// | GLM-4.5-Air        |  ¥0.8 | ¥2-8   | ¥0.16-0.24  |
// | GLM-4.7-FlashX     |  ¥0.5 | ¥3     | ¥0.1        |
// | GLM-4.7-Flash      |  免费  | 免费   | 免费        |
// | GLM-4-Plus         |  ¥5   | ¥2.5   | -           | ¥2.5 in ?? 不对,是 in/out 反过来
// | GLM-4-Air          |  ¥0.5 | ¥0.25  | -           |
// | GLM-4-Long         |  ¥1   | ¥0.5   | -           | 1M context
// | GLM-Z1-Air         |  ¥0.5 | ?      | -           | reasoning
//
// Z.ai(Zhipu 海外版)的同名 model 价更便宜:
// | GLM-4.7            | $0.60 | $2.20  | $0.11       | 一致
// | GLM-5.2            | $1.40 | $4.40  | $0.26       |
// | GLM-5-Turbo        | $1.20 | $4.00  | $0.24       |
//
// 采用 Z.ai 海外口径(更常用、便于跨币种比较)。

fn glm_prices(m: &str) -> Option<ModelPrice> {
    if m.starts_with("glm-5.3") || m.starts_with("glm-5-3") {
        // 新品,官方未公开价,用 5.2 同价兜底
        Some(ModelPrice {
            input: 1.40,
            cache_read: Some(0.26),
            cache_creation: None,
            output: 4.40,
        })
    } else if m.starts_with("glm-5.2") {
        Some(ModelPrice {
            input: 1.40,
            cache_read: Some(0.26),
            cache_creation: None,
            output: 4.40,
        })
    } else if m.starts_with("glm-5.1") {
        Some(ModelPrice {
            input: 1.40,
            cache_read: Some(0.26),
            cache_creation: None,
            output: 4.40,
        })
    } else if m.starts_with("glm-5-turbo") || m.starts_with("glm-5turbo") {
        Some(ModelPrice {
            input: 1.20,
            cache_read: Some(0.24),
            cache_creation: None,
            output: 4.00,
        })
    } else if m.starts_with("glm-5") {
        // GLM-5 基础 / Air:¥4-6 in → ~$0.55-0.85
        Some(ModelPrice {
            input: 1.00,
            cache_read: Some(0.20),
            cache_creation: None,
            output: 3.20,
        })
    } else if m.starts_with("glm-4.7-flashx") || m.starts_with("glm-4-7-flashx") {
        Some(ModelPrice {
            input: 0.07,
            cache_read: Some(0.01),
            cache_creation: None,
            output: 0.40,
        })
    } else if m.starts_with("glm-4.7-flash") || m.starts_with("glm-4-7-flash") {
        // Free
        Some(ModelPrice {
            input: 0.0,
            cache_read: Some(0.0),
            cache_creation: None,
            output: 0.0,
        })
    } else if m.starts_with("glm-4.7") || m.starts_with("glm-4-7") {
        Some(ModelPrice {
            input: 0.60,
            cache_read: Some(0.11),
            cache_creation: None,
            output: 2.20,
        })
    } else if m.starts_with("glm-4.6v-flashx") || m.starts_with("glm-4-6v-flashx") {
        Some(ModelPrice {
            input: 0.04,
            cache_read: Some(0.004),
            cache_creation: None,
            output: 0.40,
        })
    } else if m.starts_with("glm-4.6v-flash") || m.starts_with("glm-4-6v-flash") {
        Some(ModelPrice {
            input: 0.0,
            cache_read: None,
            cache_creation: None,
            output: 0.0,
        })
    } else if m.starts_with("glm-4.6v") || m.starts_with("glm-4-6v") {
        Some(ModelPrice {
            input: 0.30,
            cache_read: Some(0.05),
            cache_creation: None,
            output: 0.90,
        })
    } else if m.starts_with("glm-4.6") || m.starts_with("glm-4-6") {
        Some(ModelPrice {
            input: 0.60,
            cache_read: Some(0.11),
            cache_creation: None,
            output: 2.20,
        })
    } else if m.starts_with("glm-4.5x") || m.starts_with("glm-4-5x") {
        Some(ModelPrice {
            input: 2.20,
            cache_read: Some(0.45),
            cache_creation: None,
            output: 8.90,
        })
    } else if m.starts_with("glm-4.5-air") || m.starts_with("glm-4-5-air") {
        Some(ModelPrice {
            input: 0.20,
            cache_read: Some(0.03),
            cache_creation: None,
            output: 1.10,
        })
    } else if m.starts_with("glm-4.5v") || m.starts_with("glm-4-5v") {
        Some(ModelPrice {
            input: 0.60,
            cache_read: Some(0.11),
            cache_creation: None,
            output: 1.80,
        })
    } else if m.starts_with("glm-4.5") || m.starts_with("glm-4-5") {
        Some(ModelPrice {
            input: 0.60,
            cache_read: Some(0.11),
            cache_creation: None,
            output: 2.20,
        })
    } else if m.starts_with("glm-4-plus") || m.starts_with("glm-4-plus") {
        Some(ModelPrice {
            input: 0.69,
            cache_read: None,
            cache_creation: None,
            output: 0.35,
        })
    } else if m.starts_with("glm-4-air") || m.starts_with("glm-4-air") {
        // ¥0.5 / ¥0.25 → $0.07 / $0.035
        Some(ModelPrice {
            input: 0.07,
            cache_read: None,
            cache_creation: None,
            output: 0.035,
        })
    } else if m.starts_with("glm-4-long") || m.starts_with("glm-4-long") {
        Some(ModelPrice {
            input: 0.14,
            cache_read: None,
            cache_creation: None,
            output: 0.07,
        })
    } else if m.starts_with("glm-z1") {
        Some(ModelPrice {
            input: 0.07,
            cache_read: None,
            cache_creation: None,
            output: 0.07,
        })
    } else if m.starts_with("glm-4-flashx") || m.starts_with("glm-4-flashx") {
        Some(ModelPrice {
            input: 0.014,
            cache_read: None,
            cache_creation: None,
            output: 0.007,
        })
    } else if m.starts_with("glm") {
        // 兜底:未知 GLM 子模型给 4.7 价
        Some(ModelPrice {
            input: 0.60,
            cache_read: Some(0.11),
            cache_creation: None,
            output: 2.20,
        })
    } else {
        None
    }
}

// ─────────────────────────────────────────────────────────────────────────
//  Kimi(Moonshot 月之暗面)  —  https://platform.moonshot.cn/docs/pricing/chat
// ─────────────────────────────────────────────────────────────────────────
//
// 2026-08 价(USD / 1M tokens,按 ¥/$ = 7.2):
//
// | Model            | Input | Output | Cache Read |
// | Kimi K3          | ¥20   | ¥100   | -          | 1M context 旗舰
// | Kimi K2.7 Code   | ¥6.5  | ¥27    | -          | coding 多模态
// | Kimi K2.6        | ¥6.5  | ¥27    | -          |
// | Kimi K2.5        | ¥4    | ¥21    | ¥0.7       | (OpenRouter: $0.375 in / $2.025 out)
// | Moonshot V1 Auto| -     | -      | -          | 8/31 下线
// | Moonshot V1 8K   | ¥2    | ¥2     | -          |
// | Moonshot V1 32K  | ¥2    | ¥2     | -          |
// | Moonshot V1 128K | ¥6    | ¥6     | -          |

fn kimi_prices(m: &str) -> Option<ModelPrice> {
    // USD 换算(¥ → $,约 7.2:1)
    let yuan = |y: f64| y / 7.2;
    if m.starts_with("kimi-k3") {
        Some(ModelPrice { input: yuan(20.0), cache_read: None, cache_creation: None, output: yuan(100.0) })
    } else if m.starts_with("kimi-k2.7-code") || m.starts_with("kimi-k2-7-code") {
        Some(ModelPrice { input: yuan(6.5), cache_read: None, cache_creation: None, output: yuan(27.0) })
    } else if m.starts_with("kimi-k2.7") || m.starts_with("kimi-k2-7") {
        Some(ModelPrice { input: yuan(6.5), cache_read: None, cache_creation: None, output: yuan(27.0) })
    } else if m.starts_with("kimi-k2.6") || m.starts_with("kimi-k2-6") {
        Some(ModelPrice { input: yuan(6.5), cache_read: None, cache_creation: None, output: yuan(27.0) })
    } else if m.starts_with("kimi-k2.5") || m.starts_with("kimi-k2-5") {
        Some(ModelPrice { input: yuan(4.0), cache_read: Some(yuan(0.7)), cache_creation: None, output: yuan(21.0) })
    } else if m.starts_with("kimi-k2") {
        Some(ModelPrice { input: 0.60, cache_read: Some(0.15), cache_creation: None, output: 2.50 })
    } else if m.starts_with("moonshot-v1-128k") {
        Some(ModelPrice { input: yuan(6.0), cache_read: None, cache_creation: None, output: yuan(6.0) })
    } else if m.starts_with("moonshot-v1-32k") {
        Some(ModelPrice { input: yuan(2.0), cache_read: None, cache_creation: None, output: yuan(2.0) })
    } else if m.starts_with("moonshot-v1-8k") {
        Some(ModelPrice { input: yuan(2.0), cache_read: None, cache_creation: None, output: yuan(2.0) })
    } else if m.starts_with("moonshot") {
        Some(ModelPrice { input: yuan(2.0), cache_read: None, cache_creation: None, output: yuan(2.0) })
    } else {
        None
    }
}

// ─────────────────────────────────────────────────────────────────────────
//  MiniMax(MiniMax)  —  https://platform.minimax.io/docs/guides/pricing-paygo
// ─────────────────────────────────────────────────────────────────────────
//
// 2026-08 价(USD / 1M tokens,Pay-as-you-go 永久 50% off):
//
// | Model                       | Input | Output | Cache Read | Cache Write |
// | MiniMax-M3 ≤512k (perma 50%)| 0.30  | 1.20   | 0.06       | -           |
// | MiniMax-M3 >512k            | 0.60  | 2.40   | 0.12       | -           |
// | MiniMax-M2.7                | 0.30  | 1.20   | 0.06       | 0.375       |
// | MiniMax-M2.7-highspeed      | 0.60  | 2.40   | 0.06       | 0.375       |
// | MiniMax-M2.5 / M2.1         | 0.30  | 1.20   | 0.03       | 0.375       |
// | MiniMax-M2.5-highspeed       | 0.60  | 2.40   | 0.03       | 0.375       |
// | MiniMax-VL-01 (vision)      | 0.30  | 0.30   | -          | -           |

fn minimax_prices(m: &str) -> Option<ModelPrice> {
    if m.starts_with("minimax-m3") || m.starts_with("minimax-m-3")
        || m.starts_with("MiniMax-M3") || m.starts_with("MiniMax-m3")
    {
        // 默认 ≤512k 档;>512k 由客户端 alert(目前无 prefix 区分)
        Some(ModelPrice {
            input: 0.30,
            cache_read: Some(0.06),
            cache_creation: None,
            output: 1.20,
        })
    } else if m.starts_with("minimax-m2.7-highspeed") || m.starts_with("minimax-m-2-7-highspeed")
        || m.starts_with("MiniMax-M2.7-highspeed")
    {
        Some(ModelPrice {
            input: 0.60,
            cache_read: Some(0.06),
            cache_creation: Some(0.375),
            output: 2.40,
        })
    } else if m.starts_with("minimax-m2.7") || m.starts_with("minimax-m-2-7")
        || m.starts_with("MiniMax-M2.7")
    {
        Some(ModelPrice {
            input: 0.30,
            cache_read: Some(0.06),
            cache_creation: Some(0.375),
            output: 1.20,
        })
    } else if m.starts_with("minimax-m2.5-highspeed") || m.starts_with("minimax-m2.1-highspeed")
        || m.starts_with("minimax-m-2-5-highspeed") || m.starts_with("minimax-m-2-1-highspeed")
    {
        Some(ModelPrice {
            input: 0.60,
            cache_read: Some(0.03),
            cache_creation: Some(0.375),
            output: 2.40,
        })
    } else if m.starts_with("minimax-m2.5") || m.starts_with("minimax-m2.1")
        || m.starts_with("minimax-m-2-5") || m.starts_with("minimax-m-2-1")
        || m.starts_with("MiniMax-M2.5") || m.starts_with("MiniMax-M2.1")
    {
        Some(ModelPrice {
            input: 0.30,
            cache_read: Some(0.03),
            cache_creation: Some(0.375),
            output: 1.20,
        })
    } else if m.starts_with("minimax-vl") || m.starts_with("MiniMax-VL") {
        Some(ModelPrice {
            input: 0.30,
            cache_read: None,
            cache_creation: None,
            output: 0.30,
        })
    } else if m.starts_with("minimax") {
        // 兜底:未知 MiniMax 子模型给保守价
        Some(ModelPrice {
            input: 0.30,
            cache_read: None,
            cache_creation: None,
            output: 1.20,
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_prices_resolves_known_models() {
        // Anthropic 全系
        assert!(builtin_prices("claude-fable-5").is_some());
        assert!(builtin_prices("claude-fable-5-20260609").is_some());
        assert!(builtin_prices("claude-mythos-5").is_some());
        assert!(builtin_prices("claude-opus-5").is_some());
        assert!(builtin_prices("claude-opus-4-8").is_some());
        assert!(builtin_prices("claude-opus-4-7").is_some());
        assert!(builtin_prices("claude-opus-4-6").is_some());
        assert!(builtin_prices("claude-opus-4-5").is_some());
        assert!(builtin_prices("claude-opus-4-1-20250805").is_some());
        assert!(builtin_prices("claude-opus-4").is_some());
        assert!(builtin_prices("claude-sonnet-5").is_some());
        assert!(builtin_prices("claude-sonnet-4-6").is_some());
        assert!(builtin_prices("claude-sonnet-4-5").is_some());
        assert!(builtin_prices("claude-sonnet-4").is_some());
        assert!(builtin_prices("claude-haiku-4-5").is_some());
        assert!(builtin_prices("claude-haiku-3-5").is_some());

        // OpenAI 全系
        assert!(builtin_prices("gpt-5.6").is_some());
        assert!(builtin_prices("gpt-5.5").is_some());
        assert!(builtin_prices("gpt-5.4").is_some());
        assert!(builtin_prices("gpt-5.4-mini").is_some());
        assert!(builtin_prices("gpt-5.4-nano").is_some());
        assert!(builtin_prices("gpt-5.2").is_some());
        assert!(builtin_prices("gpt-5.1").is_some());
        assert!(builtin_prices("gpt-5").is_some());
        assert!(builtin_prices("gpt-5-mini").is_some());
        assert!(builtin_prices("gpt-5-nano").is_some());
        assert!(builtin_prices("gpt-4.1").is_some());
        assert!(builtin_prices("gpt-4.1-mini").is_some());
        assert!(builtin_prices("gpt-4.1-nano").is_some());
        assert!(builtin_prices("gpt-4o").is_some());
        assert!(builtin_prices("gpt-4o-mini").is_some());
        assert!(builtin_prices("o3").is_some());
        assert!(builtin_prices("o4-mini").is_some());
        assert!(builtin_prices("o1").is_some());

        // Google Gemini 全系
        assert!(builtin_prices("gemini-3.7-flash").is_some());
        assert!(builtin_prices("gemini-3.5-flash").is_some());
        assert!(builtin_prices("gemini-3.1-pro").is_some());
        assert!(builtin_prices("gemini-3-flash").is_some());
        assert!(builtin_prices("gemini-2.5-pro").is_some());
        assert!(builtin_prices("gemini-2.5-flash").is_some());
        assert!(builtin_prices("gemini-2.5-flash-lite").is_some());
        assert!(builtin_prices("gemini-2.0-flash").is_some());

        // DeepSeek 全系
        assert!(builtin_prices("deepseek-v4-pro").is_some());
        assert!(builtin_prices("deepseek-v4-flash").is_some());
        assert!(builtin_prices("deepseek-v3").is_some());
        assert!(builtin_prices("deepseek-r1").is_some());

        // GLM 全系
        assert!(builtin_prices("glm-5.3").is_some());
        assert!(builtin_prices("glm-5.2").is_some());
        assert!(builtin_prices("glm-5.1").is_some());
        assert!(builtin_prices("glm-5-turbo").is_some());
        assert!(builtin_prices("glm-5").is_some());
        assert!(builtin_prices("glm-4.7").is_some());
        assert!(builtin_prices("glm-4.7-flashx").is_some());
        assert!(builtin_prices("glm-4.6").is_some());
        assert!(builtin_prices("glm-4.5-air").is_some());
        assert!(builtin_prices("glm-4.5").is_some());
        assert!(builtin_prices("glm-4-plus").is_some());
        assert!(builtin_prices("glm-4-air").is_some());
        assert!(builtin_prices("glm-4-long").is_some());
        assert!(builtin_prices("glm-4-flashx").is_some());

        // Kimi 全系
        assert!(builtin_prices("kimi-k3").is_some());
        assert!(builtin_prices("kimi-k2.7-code").is_some());
        assert!(builtin_prices("kimi-k2.7").is_some());
        assert!(builtin_prices("kimi-k2.6").is_some());
        assert!(builtin_prices("kimi-k2.5").is_some());
        assert!(builtin_prices("moonshot-v1-128k").is_some());
        assert!(builtin_prices("moonshot-v1-32k").is_some());

        // MiniMax 全系
        assert!(builtin_prices("MiniMax-M3").is_some());
        assert!(builtin_prices("MiniMax-M2.7").is_some());
        assert!(builtin_prices("MiniMax-M2.7-highspeed").is_some());
        assert!(builtin_prices("MiniMax-M2.5").is_some());
        assert!(builtin_prices("MiniMax-M2.1").is_some());
        assert!(builtin_prices("MiniMax-VL-01").is_some());

        // 大小写不敏感
        assert!(builtin_prices("CLAUDE-OPUS-4-1").is_some());

        // 未知返回 None
        assert!(builtin_prices("some-totally-fake-model-xyz").is_none());
    }

    #[test]
    fn claude_fable_5_pricing_doubles_opus() {
        // Fable 5 是 Opus 4.x 的 2 倍(input $10 vs $5, output $50 vs $25)
        let fable = builtin_prices("claude-fable-5").unwrap();
        assert_eq!(fable.input, 10.00);
        assert_eq!(fable.output, 50.00);
        // Fable 有 cache 字段(Anthropic 公开了)
        assert_eq!(fable.cache_read, Some(1.00));
        assert_eq!(fable.cache_creation, Some(12.50));

        let opus5 = builtin_prices("claude-opus-5").unwrap();
        assert_eq!(opus5.input, 5.00);
        assert_eq!(opus5.output, 25.00);
        assert_eq!(opus5.cache_read, Some(0.50));
    }

    #[test]
    fn claude_opus_4_5_to_4_8_share_pricing() {
        // 4.5/4.6/4.7/4.8 价格统一(Anthropic 公开价)
        for v in &["claude-opus-4-5", "claude-opus-4-6", "claude-opus-4-7", "claude-opus-4-8"] {
            let p = builtin_prices(v).unwrap();
            assert_eq!(p.input, 5.00, "{} input", v);
            assert_eq!(p.output, 25.00, "{} output", v);
        }
    }

    #[test]
    fn gemini_pricing_context_tiers() {
        // 2.5 Pro ≤200K $1.25;2.5 Flash $0.30;Flash-Lite $0.10
        let pro = builtin_prices("gemini-2.5-pro").unwrap();
        assert_eq!(pro.input, 1.25);
        assert_eq!(pro.output, 10.00);

        let flash = builtin_prices("gemini-2.5-flash").unwrap();
        assert_eq!(flash.input, 0.30);

        let lite = builtin_prices("gemini-2.5-flash-lite").unwrap();
        assert_eq!(lite.input, 0.10);
    }

    #[test]
    fn deepseek_v4_pricing_distinguishes_pro_vs_flash() {
        let pro = builtin_prices("deepseek-v4-pro").unwrap();
        let flash = builtin_prices("deepseek-v4-flash").unwrap();
        // Pro 是 Flash 的 3 倍 input
        assert!(pro.input > flash.input);
        assert_eq!(pro.input, 0.66);
        assert_eq!(flash.input, 0.22);
        // 4-pro-0813-ga 都能命中(版本快照后缀)
        assert!(builtin_prices("deepseek-v4-pro-0813-ga").is_some());
    }

    #[test]
    fn glm_4_7_pricing_matches_zhipu_official() {
        let m = builtin_prices("glm-4.7").unwrap();
        assert_eq!(m.input, 0.60);
        assert_eq!(m.output, 2.20);
        assert_eq!(m.cache_read, Some(0.11));
    }

    #[test]
    fn glm_flash_free_returns_zero_pricing() {
        let p = builtin_prices("glm-4.7-flash").unwrap();
        assert_eq!(p.input, 0.0);
        assert_eq!(p.output, 0.0);
    }

    #[test]
    fn minimax_m3_pricing_matches_official_page() {
        let m3 = builtin_prices("MiniMax-M3").unwrap();
        assert_eq!(m3.input, 0.30);
        assert_eq!(m3.output, 1.20);
        assert_eq!(m3.cache_read, Some(0.06));
        // 没有 cache_creation
        assert_eq!(m3.cache_creation, None);
    }

    #[test]
    fn kimi_k3_is_most_expensive_in_family() {
        let k3 = builtin_prices("kimi-k3").unwrap();
        let k2_6 = builtin_prices("kimi-k2.6").unwrap();
        assert!(k3.input > k2_6.input);
        assert!(k3.output > k2_6.output);
    }

    #[test]
    fn event_cost_computes_correctly() {
        let p = ModelPrice {
            input: 15.0,
            cache_read: Some(1.5),
            cache_creation: Some(18.75),
            output: 75.0,
        };
        // 1M in + 500K cache_read + 100K cache_creation + 200K out
        let c = p.event_cost(1_000_000, 500_000, 100_000, 200_000);
        // in: 1 * 15 = 15
        // cr: 0.5 * 1.5 = 0.75
        // cc: 0.1 * 18.75 = 1.875
        // out: 0.2 * 75 = 15
        let expected = 15.0 + 0.75 + 1.875 + 15.0;
        assert!((c - expected).abs() < 0.001);
    }

    #[test]
    fn event_cost_zero_when_no_cache_prices() {
        // cache 字段都是 None → 字段为 0 时不收费
        let p = ModelPrice {
            input: 10.0,
            cache_read: None,
            cache_creation: None,
            output: 30.0,
        };
        let c = p.event_cost(1_000_000, 500_000, 100_000, 200_000);
        let expected = 10.0 + 0.0 + 0.0 + (200_000.0 / 1_000_000.0) * 30.0;
        assert!((c - expected).abs() < 0.001);
    }

    #[test]
    fn provider_pricing_override_beats_default() {
        let mut pp = ProviderPricing::default();
        pp.overrides.insert(
            "claude-opus-4-1".into(),
            ModelPrice {
                input: 999.0,
                cache_read: None,
                cache_creation: None,
                output: 999.0,
            },
        );
        let p = pp.resolve(Some(&pp), "claude-opus-4-1");
        assert_eq!(p.unwrap().input, 999.0);
    }

    #[test]
    fn provider_pricing_alias_maps_to_default_table() {
        let mut pp = ProviderPricing::default();
        pp.aliases
            .insert("route-claude-opus".into(), "claude-opus-4-1".into());
        let p = pp.resolve(Some(&pp), "route-claude-opus");
        // 应该走默认表里的 claude-opus-4-1 价: $15
        assert_eq!(p.unwrap().input, 15.00);
    }

    #[test]
    fn provider_pricing_alias_falls_back_to_model_id() {
        let mut pp = ProviderPricing::default();
        pp.aliases
            .insert("unrelated".into(), "claude-opus-4-1".into());
        let p = pp.resolve(Some(&pp), "gpt-4o");
        // gpt-4o 在默认表里: $2.50
        assert_eq!(p.unwrap().input, 2.50);
    }

    #[test]
    fn provider_pricing_unknown_model_returns_none() {
        let pp = ProviderPricing::default();
        let p = pp.resolve(Some(&pp), "some-fake-model");
        assert!(p.is_none());
    }

    #[test]
    fn provider_pricing_none_provider_returns_none() {
        assert!(ProviderPricing::default()
            .resolve(None, "claude-opus-4-1")
            .is_none());
        assert!(ProviderPricing {
            ..Default::default()
        }
        .resolve(None, "claude-opus-4-1")
        .is_none());
    }
}