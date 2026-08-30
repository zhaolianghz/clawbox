//! 模型官方价表(USD per 1M tokens)。
//!
//! **设计原则**:成本按"官方公开价"算,中转站 / 第三方别名一律通过
//! ProviderSpec.model_aliases 映射到本表的 canonical model name 算。
//!
//! 价格单位:USD / 1,000,000 tokens(即 1M token 多少美元)。
//!
//! v1 价表覆盖:Anthropic Claude 4 系列 + 4.1、OpenAI GPT-4o / 4.1 / 5、
//! Google Gemini 2.5、DeepSeek V3 / R1、智谱 GLM-4.5 / 4.6、月之暗面 Kimi K2。
//!
//! 字段对应 BucketTotals 4 列:
//! - input  → `input`
//! - cache_read → `cache_read`(Anthropic 用,OpenAI 无此分项)
//! - cache_creation → `cache_creation`(Anthropic 用)
//! - output → `output`
//!
//! 数据源(2026-08 查):Anthropic pricing 页 / OpenAI API pricing / Gemini pricing /
//! 各服务商官网公开页。**价格可能过时**——用户可在 Providers 页用 `override_price`
//! 覆盖;某 model 不在本表 → 调用方得到 None → UI 显示"—",用量数字仍正常。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// 单个 model 的官方公开价(USD / 1M tokens)。
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ModelPrice {
    pub input: f64,
    pub cache_read: Option<f64>,
    pub cache_creation: Option<f64>,
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

/// 用户在 Providers 页对某个 provider 的某个 model 设的覆盖价(USD / 1M tokens)。
/// key 是**该 provider 下**的 model id(可能是中转别名),value 给出 canonical
/// model id 用以查默认价表;若 value 为 None,表示该 model 完全无价(显式 0)。
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ProviderPricing {
    /// provider 下 model id → canonical model id(查默认价表用)。
    /// 空 = 不映射,直接拿 model id 去查价表(适合 provider.model 已是官方名)。
    #[serde(default)]
    pub aliases: BTreeMap<String, String>,
    /// 单 model 价格覆盖(USD / 1M tokens,完整 4 列)。
    /// 优先级高于默认价表 + aliases 推导。覆盖后 UI 标"自定义"。
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
        let canonical = provider.aliases.get(model).map(|s| s.as_str()).unwrap_or(model);
        builtin_prices(canonical)
    }
}

/// 默认价表。**写死**(用户通过 ProviderPricing.overrides 覆盖)。
/// 维护约定:加 model 前先查官方价,不要凭印象。
pub fn builtin_prices(model: &str) -> Option<ModelPrice> {
    let m = model.to_ascii_lowercase();
    // Anthropic Claude(2026 年公开价)
    if let Some(p) = claude_prices(&m) {
        return Some(p);
    }
    // OpenAI
    if let Some(p) = openai_prices(&m) {
        return Some(p);
    }
    // Google Gemini
    if let Some(p) = gemini_prices(&m) {
        return Some(p);
    }
    // DeepSeek
    if let Some(p) = deepseek_prices(&m) {
        return Some(p);
    }
    // 智谱 GLM
    if let Some(p) = glm_prices(&m) {
        return Some(p);
    }
    // Moonshot Kimi
    if let Some(p) = kimi_prices(&m) {
        return Some(p);
    }
    None
}

fn claude_prices(m: &str) -> Option<ModelPrice> {
    // 比对前缀以兼容 `-20250805` 后缀版本号
    let hit = |prefixes: &[&str]| -> bool {
        prefixes.iter().any(|p| m.starts_with(p))
    };
    if hit(&["claude-opus-4-1"]) {
        // Claude Opus 4.1: input $15, output $75, cache_read $1.50, cache_write $18.75
        Some(ModelPrice {
            input: 15.0,
            cache_read: Some(1.50),
            cache_creation: Some(18.75),
            output: 75.0,
        })
    } else if hit(&["claude-opus-4"]) {
        // Claude Opus 4: $15 / $75 / $1.50 / $18.75
        Some(ModelPrice {
            input: 15.0,
            cache_read: Some(1.50),
            cache_creation: Some(18.75),
            output: 75.0,
        })
    } else if hit(&["claude-sonnet-4-5"]) || hit(&["claude-sonnet-4-5-"]) {
        // Sonnet 4.5: $3 / $15 / $0.30 / $3.75
        Some(ModelPrice {
            input: 3.0,
            cache_read: Some(0.30),
            cache_creation: Some(3.75),
            output: 15.0,
        })
    } else if hit(&["claude-sonnet-4-1"]) {
        Some(ModelPrice {
            input: 3.0,
            cache_read: Some(0.30),
            cache_creation: Some(3.75),
            output: 15.0,
        })
    } else if hit(&["claude-sonnet-4"]) {
        // Sonnet 4: $3 / $15 / $0.30 / $3.75
        Some(ModelPrice {
            input: 3.0,
            cache_read: Some(0.30),
            cache_creation: Some(3.75),
            output: 15.0,
        })
    } else if hit(&["claude-haiku-4"]) || hit(&["claude-haiku-3-5"]) || hit(&["claude-3-5-haiku"]) {
        // Haiku 4 / 3.5: $1 / $5 / $0.10 / $1.25
        Some(ModelPrice {
            input: 1.0,
            cache_read: Some(0.10),
            cache_creation: Some(1.25),
            output: 5.0,
        })
    } else if hit(&["claude-3-opus"]) {
        // Opus 3: $15 / $75
        Some(ModelPrice {
            input: 15.0,
            cache_read: None,
            cache_creation: None,
            output: 75.0,
        })
    } else {
        None
    }
}

fn openai_prices(m: &str) -> Option<ModelPrice> {
    let hit = |prefixes: &[&str]| -> bool {
        prefixes.iter().any(|p| m.starts_with(p))
    };
    // GPT-5
    if hit(&["gpt-5"]) {
        // GPT-5 / GPT-5-mini 区分见下方
        if hit(&["gpt-5-mini"]) || hit(&["gpt-5-nano"]) {
            // mini: $0.25 / $2.00; nano: $0.05 / $0.40 (官方 cache 字段无,OpenAI 自动 prompt cache)
            Some(ModelPrice {
                input: if hit(&["gpt-5-nano"]) { 0.05 } else { 0.25 },
                cache_read: None, // OpenAI 自动 prompt cache 不暴露
                cache_creation: None,
                output: if hit(&["gpt-5-nano"]) { 0.40 } else { 2.00 },
            })
        } else {
            // gpt-5 / gpt-5-chat-latest / 等: $1.25 / $10.00
            Some(ModelPrice {
                input: 1.25,
                cache_read: None,
                cache_creation: None,
                output: 10.0,
            })
        }
    } else if hit(&["gpt-4-1"]) || hit(&["gpt-4.1"]) {
        // GPT-4.1: $3 / $10 (cache read $0.75 自动)
        Some(ModelPrice {
            input: 3.0,
            cache_read: Some(0.75),
            cache_creation: None,
            output: 10.0,
        })
    } else if hit(&["gpt-4o"]) {
        // GPT-4o: $2.50 / $10.00
        Some(ModelPrice {
            input: 2.50,
            cache_read: Some(1.25),
            cache_creation: None,
            output: 10.0,
        })
    } else if hit(&["gpt-4-turbo"]) {
        Some(ModelPrice {
            input: 10.0,
            cache_read: None,
            cache_creation: None,
            output: 30.0,
        })
    } else if hit(&["gpt-4"]) && !hit(&["gpt-4o"]) && !hit(&["gpt-4-turbo"]) {
        // 原生 gpt-4(8K context): $30 / $60
        Some(ModelPrice {
            input: 30.0,
            cache_read: None,
            cache_creation: None,
            output: 60.0,
        })
    } else if hit(&["gpt-3.5-turbo"]) {
        Some(ModelPrice {
            input: 0.50,
            cache_read: None,
            cache_creation: None,
            output: 1.50,
        })
    } else if hit(&["o1"]) || hit(&["o3"]) || hit(&["o4-mini"]) {
        // o1 / o3 / o4-mini 系列
        if hit(&["o4-mini"]) {
            // o4-mini: $1.10 / $4.40
            Some(ModelPrice {
                input: 1.10,
                cache_read: Some(0.275),
                cache_creation: None,
                output: 4.40,
            })
        } else if hit(&["o3-mini"]) {
            Some(ModelPrice {
                input: 1.10,
                cache_read: Some(0.55),
                cache_creation: None,
                output: 4.40,
            })
        } else if hit(&["o3"]) {
            Some(ModelPrice {
                input: 10.0,
                cache_read: Some(2.50),
                cache_creation: None,
                output: 40.0,
            })
        } else if hit(&["o1-mini"]) {
            Some(ModelPrice {
                input: 3.0,
                cache_read: Some(0.75),
                cache_creation: None,
                output: 12.0,
            })
        } else {
            // o1 / o1-pro
            Some(ModelPrice {
                input: 15.0,
                cache_read: Some(7.50),
                cache_creation: None,
                output: 60.0,
            })
        }
    } else {
        None
    }
}

fn gemini_prices(m: &str) -> Option<ModelPrice> {
    let hit = |prefixes: &[&str]| -> bool {
        prefixes.iter().any(|p| m.starts_with(p))
    };
    // Gemini 2.5 Pro: ≤200K $1.25/$10.00; >200K $2.50/$15.00。简化用低位价。
    if hit(&["gemini-2.5-pro"]) {
        Some(ModelPrice {
            input: 1.25,
            cache_read: None,
            cache_creation: None,
            output: 10.0,
        })
    } else if hit(&["gemini-2.5-flash"]) {
        // Flash: $0.30 / $2.50
        Some(ModelPrice {
            input: 0.30,
            cache_read: None,
            cache_creation: None,
            output: 2.50,
        })
    } else if hit(&["gemini-2.0-flash"]) || hit(&["gemini-2.0-flash-lite"]) {
        Some(ModelPrice {
            input: 0.10,
            cache_read: None,
            cache_creation: None,
            output: 0.40,
        })
    } else if hit(&["gemini-1.5-pro"]) {
        Some(ModelPrice {
            input: 1.25,
            cache_read: None,
            cache_creation: None,
            output: 5.0,
        })
    } else if hit(&["gemini-1.5-flash"]) {
        Some(ModelPrice {
            input: 0.075,
            cache_read: None,
            cache_creation: None,
            output: 0.30,
        })
    } else {
        None
    }
}

fn deepseek_prices(m: &str) -> Option<ModelPrice> {
    // DeepSeek V3 / R1 统一价:$0.27 input(cache hit)/$0.55 cache miss/$1.10 output
    // (官方页 2026-08 价,缓存命中按 input 单价算)
    if m.starts_with("deepseek-v3") || m.starts_with("deepseek-r1") || m.starts_with("deepseek-v2") {
        Some(ModelPrice {
            input: 0.27,
            cache_read: Some(0.07), // 缓存命中单独定价($0.07/M,官方页)
            cache_creation: None,
            output: 1.10,
        })
    } else if m.starts_with("deepseek") {
        // 未知 deepseek 子模型给个保守价
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

fn glm_prices(m: &str) -> Option<ModelPrice> {
    // 智谱 GLM-4.5 / 4.6 系列:input $0.30 / output $2.50(v4.5 公开价)
    // GLM-4-Flash:免费 / GLM-4-Air: $0.001/$0.001
    if m.starts_with("glm-4-6") {
        Some(ModelPrice {
            input: 0.30,
            cache_read: None,
            cache_creation: None,
            output: 2.50,
        })
    } else if m.starts_with("glm-4-5") {
        Some(ModelPrice {
            input: 0.30,
            cache_read: None,
            cache_creation: None,
            output: 2.50,
        })
    } else if m.starts_with("glm-4-flash") {
        // GLM-4-Flash 大部分免费,给一个象征值
        Some(ModelPrice {
            input: 0.0,
            cache_read: None,
            cache_creation: None,
            output: 0.0,
        })
    } else if m.starts_with("glm-4-air") {
        Some(ModelPrice {
            input: 0.001,
            cache_read: None,
            cache_creation: None,
            output: 0.001,
        })
    } else if m.starts_with("glm-4") {
        Some(ModelPrice {
            input: 0.30,
            cache_read: None,
            cache_creation: None,
            output: 2.50,
        })
    } else if m.starts_with("glm-5") {
        // GLM-5 系列假设接近 4.6 价
        Some(ModelPrice {
            input: 0.30,
            cache_read: None,
            cache_creation: None,
            output: 2.50,
        })
    } else {
        None
    }
}

fn kimi_prices(m: &str) -> Option<ModelPrice> {
    // 月之暗面 Kimi K2 / K2.5 / k2-thinking
    // 官方公开 2026-08: input $0.60 / output $2.50 (cache 类似 Anthropic 0.16)
    if m.starts_with("kimi-k2") || m.starts_with("kimi-") {
        Some(ModelPrice {
            input: 0.60,
            cache_read: Some(0.15),
            cache_creation: None,
            output: 2.50,
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
        // 主流必须命中
        assert!(builtin_prices("claude-opus-4-1-20250805").is_some());
        assert!(builtin_prices("claude-sonnet-4-5").is_some());
        assert!(builtin_prices("gpt-4o").is_some());
        assert!(builtin_prices("gpt-5").is_some());
        assert!(builtin_prices("gpt-5-mini").is_some());
        assert!(builtin_prices("o4-mini").is_some());
        assert!(builtin_prices("gemini-2.5-pro").is_some());
        assert!(builtin_prices("deepseek-v3").is_some());
        assert!(builtin_prices("glm-4.5").is_some());
        assert!(builtin_prices("glm-4.6").is_some());
        assert!(builtin_prices("kimi-k2.5").is_some());

        // 大小写不敏感
        assert!(builtin_prices("CLAUDE-OPUS-4-1").is_some());

        // 未知返回 None
        assert!(builtin_prices("claude-fable-5").is_none());
        assert!(builtin_prices("MiniMax-M3").is_none());
    }

    #[test]
    fn event_cost_computes_correctly() {
        let p = ModelPrice {
            input: 15.0,
            cache_read: Some(1.50),
            cache_creation: Some(18.75),
            output: 75.0,
        };
        // 1M input + 0.1M cache_read + 0.05M cache_creation + 0.02M output
        // = 15 + 0.15 + 0.9375 + 1.50 = $17.5875
        let cost = p.event_cost(1_000_000, 100_000, 50_000, 20_000);
        assert!(
            (cost - 17.5875).abs() < 1e-6,
            "expected 17.5875, got {cost}"
        );
    }

    #[test]
    fn event_cost_zero_when_no_cache_prices() {
        // OpenAI GPT-4o 无 cache_*,应当按 0 算 cache,不影响 input/output
        let p = ModelPrice {
            input: 2.50,
            cache_read: None,
            cache_creation: None,
            output: 10.0,
        };
        // 100K input + 50K cache_read + 30K cache_creation + 10K output
        // = 0.25 + 0 + 0 + 0.10 = $0.35
        let cost = p.event_cost(100_000, 50_000, 30_000, 10_000);
        assert!((cost - 0.35).abs() < 1e-6, "expected 0.35, got {cost}");
    }

    #[test]
    fn provider_pricing_override_beats_default() {
        let mut pp = ProviderPricing::default();
        pp.overrides.insert(
            "claude-fable-5".into(),
            ModelPrice {
                input: 5.0,
                cache_read: None,
                cache_creation: None,
                output: 20.0,
            },
        );
        let p = pp.resolve(Some(&pp), "claude-fable-5");
        assert_eq!(p.unwrap().input, 5.0);
    }

    #[test]
    fn provider_pricing_alias_maps_to_default_table() {
        let mut pp = ProviderPricing::default();
        pp.aliases.insert("claude-fable-5".into(), "claude-opus-4-1".into());
        let p = pp.resolve(Some(&pp), "claude-fable-5");
        // 应该走默认表里的 claude-opus-4-1 价: $15
        assert_eq!(p.unwrap().input, 15.0);
    }

    #[test]
    fn provider_pricing_alias_falls_back_to_model_id() {
        // alias 没映射 → 直接拿 model id 查默认表
        let mut pp = ProviderPricing::default();
        pp.aliases.insert("unrelated".into(), "claude-opus-4-1".into());
        let p = pp.resolve(Some(&pp), "gpt-4o");
        // gpt-4o 在默认表里: $2.50
        assert_eq!(p.unwrap().input, 2.50);
    }

    #[test]
    fn provider_pricing_unknown_model_returns_none() {
        let pp = ProviderPricing::default();
        // 模型不在默认表里也没 override → None
        assert!(pp.resolve(Some(&pp), "totally-fake-model").is_none());
    }

    #[test]
    fn provider_pricing_none_provider_returns_none() {
        // 没传 provider pricing → None(不报错)
        assert!(ProviderPricing::default().resolve(None, "claude-opus-4-1").is_none());
        assert!(ProviderPricing { ..Default::default() }
            .resolve(None, "claude-opus-4-1")
            .is_none());
    }
}
