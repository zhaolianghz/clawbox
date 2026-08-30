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
use time::{Date, Month};

/// 全局价表"核对日期": 2026-08-31。后续定期更新。
/// 真实数据核对日期(本仓库维护,人工核对各厂商官方页);
/// 90 天后 UI 会提示"可能已过期"但不阻止使用。
fn snapshot_date_const() -> Date {
    Date::from_calendar_date(2026, Month::August, 31)
        .expect("SNAPSHOT_DATE invalid")
}

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

/// 一个 model 的官方公开价 + 核对日期(USD per 1M tokens)。
///
/// `verified_at`: 本仓库人工核对官方价的日期。90 天后 UI banner 提示。
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PricedModel {
    pub price: ModelPrice,
    pub verified_at: Date,
}

impl PricedModel {
    pub fn new(price: ModelPrice) -> Self {
        Self { price, verified_at: snapshot_date_const() }
    }

    pub fn event_cost(
        &self,
        input: u64,
        cache_read: u64,
        cache_creation: u64,
        output: u64,
    ) -> f64 {
        self.price.event_cost(input, cache_read, cache_creation, output)
    }

    /// 是否超过 90 天未核对(UI banner 用)
    pub fn is_stale(&self, today: Date) -> bool {
        let age_days = (today - self.verified_at).whole_days();
        age_days > 90
    }

    /// 当前价表快照日期(对外公开)
    pub fn snapshot_date() -> Date {
        snapshot_date_const()
    }
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
        builtin_prices(canonical).map(|p| p.price)
    }
}

/// 全局入口:给定 model id 字符串(可能是 `claude-opus-4-1-20250805` 或
/// `gpt-5-mini` 这种大小写不敏感的任意形式),返回官方公开价 + 核对日期或 None。
pub fn builtin_prices(model: &str) -> Option<PricedModel> {
    let m = model.to_lowercase();
    if let Some(p) = claude_prices(&m) {
        return Some(PricedModel::new(p));
    }
    if let Some(p) = openai_prices(&m) {
        return Some(PricedModel::new(p));
    }
    if let Some(p) = gemini_prices(&m) {
        return Some(PricedModel::new(p));
    }
    if let Some(p) = deepseek_prices(&m) {
        return Some(PricedModel::new(p));
    }
    if let Some(p) = glm_prices(&m) {
        return Some(PricedModel::new(p));
    }
    if let Some(p) = kimi_prices(&m) {
        return Some(PricedModel::new(p));
    }
    if let Some(p) = minimax_prices(&m) {
        return Some(PricedModel::new(p));
    }
    // 阿里云百炼 DashScope (Qwen 系列)
    if let Some(p) = qwen_prices(&m) {
        return Some(PricedModel::new(p));
    }
    // 字节跳动豆包 Doubao / Seed (火山方舟 Volcano Ark)
    if let Some(p) = doubao_prices(&m) {
        return Some(PricedModel::new(p));
    }
    None
}

/// 当前价表快照日期(对外公开)
pub fn snapshot_date() -> Date {
    snapshot_date_const()
}

/// 列出所有已知的 (model_prefix, verified_at) 元数据。
/// 给 UI 显示"我们已核对 N 个 model 价格"用。
pub fn known_models() -> Vec<&'static str> {
    // 静态列举 — 写价表时维护
    vec![
        // Claude
        "claude-fable-5", "claude-mythos-5", "claude-opus-5",
        "claude-opus-4-1", "claude-opus-4", "claude-sonnet-5",
        "claude-sonnet-4-6", "claude-sonnet-4-5", "claude-sonnet-4",
        "claude-haiku-4-5", "claude-haiku-3-5", "claude-3-opus",
        "claude-3-sonnet", "claude-3-haiku",
        // OpenAI
        "gpt-5.6", "gpt-5.5", "gpt-5.4-mini", "gpt-5.4-nano", "gpt-5.4",
        "gpt-5.2", "gpt-5.1", "gpt-5-mini", "gpt-5-nano", "gpt-5",
        "gpt-4.1-nano", "gpt-4.1-mini", "gpt-4.1", "gpt-4o-mini", "gpt-4o",
        "o4-mini", "o3-mini", "o3", "o1-mini", "o1",
        "gpt-4-turbo", "gpt-4", "gpt-3.5-turbo",
        // Gemini
        "gemini-3.7-flash", "gemini-3.5-flash", "gemini-3.1-pro",
        "gemini-3-flash", "gemini-2.5-pro", "gemini-2.5-flash",
        "gemini-2.5-flash-lite", "gemini-2.0-flash",
        "gemini-1.5-pro", "gemini-1.5-flash",
        // DeepSeek
        "deepseek-v4-pro", "deepseek-v4-flash", "deepseek-v3",
        "deepseek-r1",
        // GLM
        "glm-5.3", "glm-5.2", "glm-5.1", "glm-5-turbo", "glm-5",
        "glm-4.7-flashx", "glm-4.7-flash", "glm-4.7", "glm-4.6v",
        "glm-4.6", "glm-4.5x", "glm-4.5-air", "glm-4.5", "glm-4-plus",
        "glm-4-air", "glm-4-long", "glm-z1", "glm-4-flashx",
        // Kimi
        "kimi-k3", "kimi-k2.7-code", "kimi-k2.7", "kimi-k2.6",
        "kimi-k2.5", "kimi-k2", "moonshot-v1-128k", "moonshot-v1-32k",
        "moonshot-v1-8k",
        // MiniMax
        "MiniMax-M3", "MiniMax-M2.7-highspeed", "MiniMax-M2.7",
        "MiniMax-M2.5-highspeed", "MiniMax-M2.5", "MiniMax-M2.1-highspeed",
        "MiniMax-M2.1", "MiniMax-VL-01",
        // Qwen 百炼
        "qwen3.8-max", "qwen3.7-max", "qwen3.5-397b", "qwen3.5-omni-plus",
        "qwen3.5-omni-flash", "qwen3.7-plus", "qwen3-max", "qwen3.5-plus",
        "qwen3.5-flash", "qwen3.7-flash", "qwen3.5-coder",
        "qwen-long", "qwen2.5-max", "qwen2.5-plus", "qwen2.5-coder",
        "qwen-vl-max", "qwen-plus", "qwen-turbo",
        // Doubao 豆包
        "doubao-seed-2.0-pro", "doubao-seed-2.0-code", "doubao-seed-2.0-lite",
        "doubao-seed-2.0-mini", "seed-2.0-pro", "seed-2.0-code",
        "seed-2.0-lite", "seed-2.0-mini", "seed-1.6-thinking",
        "seed-1.6-flash", "doubao-1.5-pro", "doubao-1.5-vision-pro",
        "doubao-1.5-lite",
    ]
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

// ─────────────────────────────────────────────────────────────────────────
//  Qwen(阿里云百炼 DashScope)  —  https://help.aliyun.com/zh/model-studio/model-pricing
// ─────────────────────────────────────────────────────────────────────────
//
// 2026-05/08 价(¥ / 1M tokens,以下换算 USD 按 7.2:1)。
// 国际站(bailian.console.aliyun.com)价更稳定,统一以"国际站美元口径"记录:
//
// | Model             | Input   | Output  | Notes |
// | qwen3.8-max       | $1.67   | $3.75   | 最新旗舰 1M context,2026-08
// | qwen3.7-max       | $1.25   | $3.75   | 2026-05, 50% promo
// | qwen3.5-397b      | $0.60   | $3.60   | MoE 旗舰
// | qwen3.5-plus      | $0.40   | $2.40   | ≤256K;>256K $1.25/$5.00
// | qwen3.5-flash     | $0.10   | $0.40   | 1M 快
// | qwen3.7-plus      | $0.32   | $1.28   | ≤32K; 多模态
// | qwen3-max         | $1.20   | $6.00   | 262K, cache_read $0.12
// | qwen-plus         | $0.40   | $1.20   | 老旗舰
// | qwen-turbo        | $0.05   | $0.20   | 入门
// | qwen3.5-omni-plus | $0.97   | $7.36   | 多模态旗舰
// | qwen3.5-omni-flash| $0.31   | $2.50   | 多模态快
// | qwen3.5-coder     | $0.40   | $2.40   | 编程专用
// | qwen-long         | $0.14   | $0.56   | 1M 长文
//
// cache_read 字段:qwen3-max 给 0.12;qwen3.x 系列享受"上下文缓存折扣"
// 但具体折扣率未官方公开到 USD 单价,按 input * 0.1 估算。

fn qwen_prices(m: &str) -> Option<ModelPrice> {
    let yuan = |y: f64| y / 7.2;
    if m.starts_with("qwen3.8-max") || m.starts_with("qwen3-8-max") {
        Some(ModelPrice { input: 1.67, cache_read: Some(0.17), cache_creation: None, output: 3.75 })
    } else if m.starts_with("qwen3.7-max") || m.starts_with("qwen3-7-max") {
        Some(ModelPrice { input: 1.25, cache_read: Some(0.13), cache_creation: None, output: 3.75 })
    } else if m.starts_with("qwen3.5-397b") || m.starts_with("qwen3-5-397b") || m.starts_with("qwen3.5-max") {
        Some(ModelPrice { input: 0.60, cache_read: Some(0.06), cache_creation: None, output: 3.60 })
    } else if m.starts_with("qwen3.5-omni-plus") || m.starts_with("qwen3-5-omni-plus") {
        Some(ModelPrice { input: 0.97, cache_read: Some(0.10), cache_creation: None, output: 7.36 })
    } else if m.starts_with("qwen3.5-omni-flash") || m.starts_with("qwen3-5-omni-flash") {
        Some(ModelPrice { input: 0.31, cache_read: Some(0.03), cache_creation: None, output: 2.50 })
    } else if m.starts_with("qwen3.7-plus") || m.starts_with("qwen3-7-plus") {
        // ≤32K 档;>32K 翻 3 倍
        Some(ModelPrice { input: 0.32, cache_read: Some(0.03), cache_creation: None, output: 1.28 })
    } else if m.starts_with("qwen3-max") || m.starts_with("qwen3.6-plus") {
        Some(ModelPrice { input: 1.20, cache_read: Some(0.12), cache_creation: None, output: 6.00 })
    } else if m.starts_with("qwen3.5-plus") || m.starts_with("qwen3-5-plus") {
        Some(ModelPrice { input: 0.40, cache_read: Some(0.04), cache_creation: None, output: 2.40 })
    } else if m.starts_with("qwen3.5-flash") || m.starts_with("qwen3-5-flash") {
        Some(ModelPrice { input: 0.10, cache_read: Some(0.01), cache_creation: None, output: 0.40 })
    } else if m.starts_with("qwen3.7-flash") || m.starts_with("qwen3-7-flash") {
        // 2026 新品,极便宜
        Some(ModelPrice { input: 0.03, cache_read: None, cache_creation: None, output: 0.13 })
    } else if m.starts_with("qwen3.5-coder") || m.starts_with("qwen3-5-coder") || m.starts_with("qwen3-coder") {
        Some(ModelPrice { input: 0.40, cache_read: None, cache_creation: None, output: 2.40 })
    } else if m.starts_with("qwen3") {
        // 兜底:未知 qwen3 子模型按 qwen3.5-flash 价
        Some(ModelPrice { input: 0.10, cache_read: Some(0.01), cache_creation: None, output: 0.40 })
    } else if m.starts_with("qwen-long") || m.starts_with("qwen_long") {
        Some(ModelPrice { input: 0.14, cache_read: None, cache_creation: None, output: 0.56 })
    } else if m.starts_with("qwen2.5-max") || m.starts_with("qwen2-5-max") {
        Some(ModelPrice { input: 0.40, cache_read: None, cache_creation: None, output: 1.20 })
    } else if m.starts_with("qwen2.5-plus") || m.starts_with("qwen2-5-plus") {
        Some(ModelPrice { input: 0.07, cache_read: None, cache_creation: None, output: 0.28 })
    } else if m.starts_with("qwen2.5-coder") || m.starts_with("qwen2-5-coder") || m.starts_with("qwen-coder") {
        Some(ModelPrice { input: 0.07, cache_read: None, cache_creation: None, output: 0.28 })
    } else if m.starts_with("qwen2.5") || m.starts_with("qwen2-5") {
        Some(ModelPrice { input: 0.04, cache_read: None, cache_creation: None, output: 0.40 })
    } else if m.starts_with("qwen-vl-max") || m.starts_with("qwen-vl-plus") {
        Some(ModelPrice { input: 0.21, cache_read: None, cache_creation: None, output: 0.63 })
    } else if m.starts_with("qwen-vl") {
        Some(ModelPrice { input: 0.07, cache_read: None, cache_creation: None, output: 0.07 })
    } else if m.starts_with("qwen-plus") {
        Some(ModelPrice { input: 0.40, cache_read: None, cache_creation: None, output: 1.20 })
    } else if m.starts_with("qwen-turbo") {
        Some(ModelPrice { input: 0.05, cache_read: None, cache_creation: None, output: 0.20 })
    } else if m.starts_with("qwen") {
        // 兜底:未知 qwen 子模型按 qwen-turbo 价(便宜档)
        Some(ModelPrice { input: 0.05, cache_read: None, cache_creation: None, output: 0.20 })
    } else {
        None
    }
}

// ─────────────────────────────────────────────────────────────────────────
//  Doubao / Seed(字节跳动火山方舟 Volcano Ark)  —  https://www.volcengine.com/docs/82379
// ─────────────────────────────────────────────────────────────────────────
//
// 2026 价(¥ / 1M tokens,以下换算 USD 按 7.2:1):
//
// | Model                  | Input | Output | Notes |
// | doubao-seed-2.0-pro    | ¥3.2  | ¥16-48 | 0-32K 档(¥16)→ 256K 档(¥48)
// | doubao-seed-2.0-lite   | ¥0.6  | ¥3.6   | 高吞吐
// | doubao-seed-2.0-mini   | ¥0.2  | ¥2.0   | 入门
// | doubao-seed-2.0-code   | ¥3.2  | ¥16-48 | 编程专用
// | doubao-1.5-pro         | ¥0.7  | ¥1.75  | 老旗舰
// | doubao-1.5-vision-pro  | ¥2.62 | ¥7.86  | 多模态
// | doubao-1.5-lite        | ¥0.18 | ¥0.72  | 老款快
// | seed-1.6-flash         | ¥0.124| ¥1.31  | 极便宜
// | seed-1.6-thinking      | ¥0.7  | ¥7.0   | 推理
//
// cache_read:豆包官方"缓存命中价 = 输入价的 20%"。

fn doubao_prices(m: &str) -> Option<ModelPrice> {
    let yuan = |y: f64| y / 7.2;
    if m.starts_with("doubao-seed-2.0-pro") || m.starts_with("doubao-seed-2-pro") {
        // 默认 ≤32K 档(¥3.2/¥16);>32K 用比例自动升档(此处只给下限)
        Some(ModelPrice { input: yuan(3.2), cache_read: Some(yuan(0.64)), cache_creation: None, output: yuan(16.0) })
    } else if m.starts_with("doubao-seed-2.0-code") || m.starts_with("doubao-seed-2-code") {
        Some(ModelPrice { input: yuan(3.2), cache_read: Some(yuan(0.64)), cache_creation: None, output: yuan(16.0) })
    } else if m.starts_with("doubao-seed-2.0-lite") || m.starts_with("doubao-seed-2-lite") {
        Some(ModelPrice { input: yuan(0.6), cache_read: Some(yuan(0.12)), cache_creation: None, output: yuan(3.6) })
    } else if m.starts_with("doubao-seed-2.0-mini") || m.starts_with("doubao-seed-2-mini") {
        Some(ModelPrice { input: yuan(0.2), cache_read: Some(yuan(0.04)), cache_creation: None, output: yuan(2.0) })
    } else if m.starts_with("seed-2.0-pro") || m.starts_with("seed-2-pro") {
        Some(ModelPrice { input: yuan(3.2), cache_read: Some(yuan(0.64)), cache_creation: None, output: yuan(16.0) })
    } else if m.starts_with("seed-2.0-code") || m.starts_with("seed-2-code") {
        Some(ModelPrice { input: yuan(3.2), cache_read: Some(yuan(0.64)), cache_creation: None, output: yuan(16.0) })
    } else if m.starts_with("seed-2.0-lite") || m.starts_with("seed-2-lite") {
        Some(ModelPrice { input: yuan(0.6), cache_read: Some(yuan(0.12)), cache_creation: None, output: yuan(3.6) })
    } else if m.starts_with("seed-2.0-mini") || m.starts_with("seed-2-mini") {
        Some(ModelPrice { input: yuan(0.2), cache_read: Some(yuan(0.04)), cache_creation: None, output: yuan(2.0) })
    } else if m.starts_with("seed-1.6-thinking") || m.starts_with("seed-1-6-thinking") {
        Some(ModelPrice { input: yuan(0.7), cache_read: Some(yuan(0.14)), cache_creation: None, output: yuan(7.0) })
    } else if m.starts_with("seed-1.6-flash") || m.starts_with("seed-1-6-flash") {
        Some(ModelPrice { input: yuan(0.124), cache_read: Some(yuan(0.025)), cache_creation: None, output: yuan(1.31) })
    } else if m.starts_with("doubao-1.5-vision-pro") || m.starts_with("doubao-1-5-vision-pro") {
        Some(ModelPrice { input: yuan(2.62), cache_read: None, cache_creation: None, output: yuan(7.86) })
    } else if m.starts_with("doubao-1.5-pro") || m.starts_with("doubao-1-5-pro") {
        Some(ModelPrice { input: yuan(0.7), cache_read: None, cache_creation: None, output: yuan(1.75) })
    } else if m.starts_with("doubao-1.5-lite") || m.starts_with("doubao-1-5-lite") {
        Some(ModelPrice { input: yuan(0.18), cache_read: None, cache_creation: None, output: yuan(0.72) })
    } else if m.starts_with("doubao-pro") {
        // 兜底:未知 doubao-pro 子模型按 1.5-pro 价
        Some(ModelPrice { input: yuan(0.7), cache_read: None, cache_creation: None, output: yuan(1.75) })
    } else if m.starts_with("doubao-lite") {
        Some(ModelPrice { input: yuan(0.18), cache_read: None, cache_creation: None, output: yuan(0.72) })
    } else if m.starts_with("seed") {
        // 兜底:未知 seed 子模型按 1.6-flash 价
        Some(ModelPrice { input: yuan(0.124), cache_read: Some(yuan(0.025)), cache_creation: None, output: yuan(1.31) })
    } else if m.starts_with("doubao") {
        // 兜底:未知 doubao 子模型按 lite 价
        Some(ModelPrice { input: yuan(0.18), cache_read: None, cache_creation: None, output: yuan(0.72) })
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

        // Qwen(百炼) 全系
        assert!(builtin_prices("qwen3.8-max").is_some());
        assert!(builtin_prices("qwen3.7-max").is_some());
        assert!(builtin_prices("qwen3.5-397b").is_some());
        assert!(builtin_prices("qwen3.5-omni-plus").is_some());
        assert!(builtin_prices("qwen3.5-omni-flash").is_some());
        assert!(builtin_prices("qwen3.7-plus").is_some());
        assert!(builtin_prices("qwen3-max").is_some());
        assert!(builtin_prices("qwen3.5-plus").is_some());
        assert!(builtin_prices("qwen3.5-flash").is_some());
        assert!(builtin_prices("qwen3.7-flash").is_some());
        assert!(builtin_prices("qwen3.5-coder").is_some());
        assert!(builtin_prices("qwen-long").is_some());
        assert!(builtin_prices("qwen2.5-max").is_some());
        assert!(builtin_prices("qwen2.5-plus").is_some());
        assert!(builtin_prices("qwen2.5-coder").is_some());
        assert!(builtin_prices("qwen-vl-max").is_some());
        assert!(builtin_prices("qwen-plus").is_some());
        assert!(builtin_prices("qwen-turbo").is_some());

        // Doubao(豆包/Seed) 全系
        assert!(builtin_prices("doubao-seed-2.0-pro").is_some());
        assert!(builtin_prices("doubao-seed-2.0-code").is_some());
        assert!(builtin_prices("doubao-seed-2.0-lite").is_some());
        assert!(builtin_prices("doubao-seed-2.0-mini").is_some());
        assert!(builtin_prices("seed-2.0-pro").is_some());
        assert!(builtin_prices("seed-1.6-flash").is_some());
        assert!(builtin_prices("seed-1.6-thinking").is_some());
        assert!(builtin_prices("doubao-1.5-pro").is_some());
        assert!(builtin_prices("doubao-1.5-vision-pro").is_some());
        assert!(builtin_prices("doubao-1.5-lite").is_some());

        // 大小写不敏感
        assert!(builtin_prices("CLAUDE-OPUS-4-1").is_some());

        // 未知返回 None
        assert!(builtin_prices("some-totally-fake-model-xyz").is_none());
    }

    #[test]
    fn claude_fable_5_pricing_doubles_opus() {
        // Fable 5 是 Opus 4.x 的 2 倍(input $10 vs $5, output $50 vs $25)
        let fable = builtin_prices("claude-fable-5").unwrap();
        assert_eq!(fable.price.input, 10.00);
        assert_eq!(fable.price.output, 50.00);
        // Fable 有 cache 字段(Anthropic 公开了)
        assert_eq!(fable.price.cache_read, Some(1.00));
        assert_eq!(fable.price.cache_creation, Some(12.50));

        let opus5 = builtin_prices("claude-opus-5").unwrap();
        assert_eq!(opus5.price.input, 5.00);
        assert_eq!(opus5.price.output, 25.00);
        assert_eq!(opus5.price.cache_read, Some(0.50));
    }

    #[test]
    fn claude_opus_4_5_to_4_8_share_pricing() {
        // 4.5/4.6/4.7/4.8 价格统一(Anthropic 公开价)
        for v in &["claude-opus-4-5", "claude-opus-4-6", "claude-opus-4-7", "claude-opus-4-8"] {
            let p = builtin_prices(v).unwrap();
            assert_eq!(p.price.input, 5.00, "{} input", v);
            assert_eq!(p.price.output, 25.00, "{} output", v);
        }
    }

    #[test]
    fn gemini_pricing_context_tiers() {
        // 2.5 Pro ≤200K $1.25;2.5 Flash $0.30;Flash-Lite $0.10
        let pro = builtin_prices("gemini-2.5-pro").unwrap();
        assert_eq!(pro.price.input, 1.25);
        assert_eq!(pro.price.output, 10.00);

        let flash = builtin_prices("gemini-2.5-flash").unwrap();
        assert_eq!(flash.price.input, 0.30);

        let lite = builtin_prices("gemini-2.5-flash-lite").unwrap();
        assert_eq!(lite.price.input, 0.10);
    }

    #[test]
    fn deepseek_v4_pricing_distinguishes_pro_vs_flash() {
        let pro = builtin_prices("deepseek-v4-pro").unwrap();
        let flash = builtin_prices("deepseek-v4-flash").unwrap();
        // Pro 是 Flash 的 3 倍 input
        assert!(pro.price.input > flash.price.input);
        assert_eq!(pro.price.input, 0.66);
        assert_eq!(flash.price.input, 0.22);
        // 4-pro-0813-ga 都能命中(版本快照后缀)
        assert!(builtin_prices("deepseek-v4-pro-0813-ga").is_some());
    }

    #[test]
    fn glm_4_7_pricing_matches_zhipu_official() {
        let m = builtin_prices("glm-4.7").unwrap();
        assert_eq!(m.price.input, 0.60);
        assert_eq!(m.price.output, 2.20);
        assert_eq!(m.price.cache_read, Some(0.11));
    }

    #[test]
    fn glm_flash_free_returns_zero_pricing() {
        let p = builtin_prices("glm-4.7-flash").unwrap();
        assert_eq!(p.price.input, 0.0);
        assert_eq!(p.price.output, 0.0);
    }

    #[test]
    fn minimax_m3_pricing_matches_official_page() {
        let m3 = builtin_prices("MiniMax-M3").unwrap();
        assert_eq!(m3.price.input, 0.30);
        assert_eq!(m3.price.output, 1.20);
        assert_eq!(m3.price.cache_read, Some(0.06));
        // 没有 cache_creation
        assert_eq!(m3.price.cache_creation, None);
    }

    #[test]
    fn kimi_k3_is_most_expensive_in_family() {
        let k3 = builtin_prices("kimi-k3").unwrap();
        let k2_6 = builtin_prices("kimi-k2.6").unwrap();
        assert!(k3.price.input > k2_6.price.input);
        assert!(k3.price.output > k2_6.price.output);
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
    fn qwen_pricing_matches_bailian_intl_pricing() {
        // qwen3.7-max 50% promo: $1.25/$3.75
        let q37max = builtin_prices("qwen3.7-max").unwrap();
        assert_eq!(q37max.price.input, 1.25);
        assert_eq!(q37max.price.output, 3.75);

        // qwen3.5-397b: $0.60/$3.60
        let q397b = builtin_prices("qwen3.5-397b").unwrap();
        assert_eq!(q397b.price.input, 0.60);
        assert_eq!(q397b.price.output, 3.60);

        // qwen3.5-flash: $0.10/$0.40
        let qflash = builtin_prices("qwen3.5-flash").unwrap();
        assert_eq!(qflash.price.input, 0.10);
        assert_eq!(qflash.price.output, 0.40);

        // qwen-turbo: $0.05/$0.20 (入门最便宜)
        let turbo = builtin_prices("qwen-turbo").unwrap();
        assert_eq!(turbo.price.input, 0.05);
        assert_eq!(turbo.price.output, 0.20);
    }

    #[test]
    fn doubao_pricing_matches_volcano_ark_pricing() {
        // doubao-seed-2.0-pro 0-32K 档: ¥3.2/¥16 → ~$0.44/$2.22
        let pro = builtin_prices("doubao-seed-2.0-pro").unwrap();
        assert!((pro.price.input - 3.2 / 7.2).abs() < 0.01);
        assert!((pro.price.output - 16.0 / 7.2).abs() < 0.01);

        // doubao-1.5-pro: ¥0.7/¥1.75 → ~$0.10/$0.24
        let p15 = builtin_prices("doubao-1.5-pro").unwrap();
        assert!((p15.price.input - 0.7 / 7.2).abs() < 0.01);
        assert!((p15.price.output - 1.75 / 7.2).abs() < 0.01);

        // seed-1.6-flash: ¥0.124/¥1.31
        let s16f = builtin_prices("seed-1.6-flash").unwrap();
        assert!(s16f.price.input < 0.03, "seed-1.6-flash input 应该是最便宜的");
        assert!(s16f.price.output < 0.20, "seed-1.6-flash output 应该是最便宜的");
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

    #[test]
    fn stale_price_over_90_days() {
        use time::Month;
        // 当前 SNAPSHOT_DATE 是 2026-08-31
        // 加 91 天后 = 2026-12-01,应该 stale
        let today = time::Date::from_calendar_date(2026, Month::December, 1).unwrap();
        let p = builtin_prices("claude-fable-5").unwrap();
        assert!(p.is_stale(today), "91 天后应该标记为 stale");
    }

    #[test]
    fn fresh_price_under_90_days() {
        use time::Month;
        // SNAPSHOT_DATE + 30 天应新鲜
        let today = time::Date::from_calendar_date(2026, Month::September, 30).unwrap();
        let p = builtin_prices("claude-fable-5").unwrap();
        assert!(!p.is_stale(today), "30 天后应仍新鲜");
    }

    #[test]
    fn snapshot_date_is_2026_08_31() {
        use time::Month;
        let d = PricedModel::snapshot_date();
        assert_eq!(d.year(), 2026);
        assert_eq!(d.month(), Month::August);
        assert_eq!(d.day(), 31);
    }

    #[test]
    fn known_models_lists_nine_vendors() {
        let models = known_models();
        // 至少包含 9 家厂商的代表性 model
        assert!(models.contains(&"claude-fable-5"));
        assert!(models.contains(&"gpt-5"));
        assert!(models.contains(&"gemini-2.5-pro"));
        assert!(models.contains(&"deepseek-v4-pro"));
        assert!(models.contains(&"glm-4.7"));
        assert!(models.contains(&"kimi-k2.6"));
        assert!(models.contains(&"MiniMax-M3"));
        assert!(models.contains(&"qwen3.7-max"));
        assert!(models.contains(&"doubao-seed-2.0-pro"));
        // 至少 80 个
        assert!(models.len() >= 80, "只有 {} 个 model", models.len());
    }

}