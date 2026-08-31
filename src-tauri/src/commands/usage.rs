//! Token 用量统计 — Tauri 命令层。
//!
//! 三个命令:
//! - `usage_summary(window_days)` — 读月桶聚合返回最近 N 天的按 agent/model 汇总
//! - `usage_refresh()` — 触发 aggregate::refresh 扫描落盘
//! - `usage_provider_summary()` — 按 agent_to_provider_at_scan 快照汇总到 provider 名下
//!
//! 命令层是薄封装:读锁 → 调 logic → 序列化返回。所有真实计算在
//! `crate::usage` 模块,这里不复制。

use crate::commands::config::{load_config, real_home, CONFIG_LOCK};
use crate::usage::pricing;
use crate::usage::{aggregate, store};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use time::{Date, Month, OffsetDateTime};

/// 一行汇总(前端展示)。
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UsageTotals {
    pub input: u64,
    pub cache_read: u64,
    pub cache_creation: u64,
    pub output: u64,
    pub events: u64,
    /// 折算成本(USD)。None = model 不在价表里;Some(0.0) = 官方免费 model。
    pub cost_usd: Option<f64>,
}

impl UsageTotals {
    pub fn add(&mut self, other: &UsageTotals) {
        self.input += other.input;
        self.cache_read += other.cache_read;
        self.cache_creation += other.cache_creation;
        self.output += other.output;
        self.events += other.events;
        // cost_usd merge:Some+None=Some(已写优先),None+None=None,Some+Some=累加
        match (self.cost_usd, other.cost_usd) {
            (Some(a), Some(b)) => self.cost_usd = Some(a + b),
            (None, Some(b)) => self.cost_usd = Some(b),
            (Some(a), None) => self.cost_usd = Some(a),
            (None, None) => self.cost_usd = None,
        }
    }
}

/// 价格表快照元信息,前端展示 stale banner 用。
#[derive(Clone, Debug, Serialize)]
pub struct PricingMeta {
    /// 价表快照日期(YYYY-MM-DD)
    pub snapshot_date: String,
    /// 当前距快照多少天(基于 today)
    pub age_days: i64,
    /// 距 stale 还有多少天(可负,负数表示已 stale)
    pub days_until_stale: i64,
    /// 当前是否已 stale
    pub is_stale: bool,
    /// 覆盖 model 数(在价表里有官方价)
    pub covered_models: u32,
}

impl PricingMeta {
    pub fn snapshot(today: Date) -> Self {
        let snap = pricing::PricedModel::snapshot_date();
        let age = (today - snap).whole_days();
        let sample = pricing::PricedModel::new(crate::usage::pricing::ModelPrice::default());
        let days_until = sample.days_until_stale(today);
        let covered = pricing::known_models().len() as u32;
        PricingMeta {
            snapshot_date: format!(
                "{:04}-{:02}-{:02}",
                snap.year(),
                snap.month() as u8,
                snap.day()
            ),
            age_days: age,
            days_until_stale: days_until,
            is_stale: age > days_until,
            covered_models: covered,
        }
    }
}

/// 顶层返回结构。前端 TypeScript 类型对齐 snake_case。
#[derive(Clone, Debug, Default, Serialize)]
pub struct UsageSummary {
    pub total: UsageTotals,
    pub by_day: Vec<DayUsage>,
    pub by_agent: Vec<AgentUsage>,
    pub parse_health: aggregate::ParseHealth,
    pub window_days: u32,
    /// 价表快照元信息(banner 用)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pricing_meta: Option<PricingMeta>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct DayUsage {
    pub date: String,
    pub totals: UsageTotals,
    pub by_agent: Vec<AgentUsage>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct AgentUsage {
    pub agent_id: String,
    pub totals: UsageTotals,
    pub by_model: Vec<ModelUsage>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct ModelUsage {
    pub model: String,
    pub totals: UsageTotals,
    pub events: u64,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct ProviderUsage {
    pub provider_name: String,
    pub totals: UsageTotals,
}

/// `usage_summary(window_days)`:聚合最近 N 天(默认 30)。
#[tauri::command]
pub fn usage_summary(window_days: u32) -> Result<UsageSummary, String> {
    let home = real_home();
    let _guard = CONFIG_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let config = load_config(&home)?;

    let days = if window_days == 0 { 30 } else { window_days };
    let now = OffsetDateTime::now_utc();
    let mut summary = UsageSummary {
        window_days: days,
        ..Default::default()
    };

    // 收集涉及的月份桶 + 每个月的快照(用于 provider 归属)
    let months = store::read_all(&home);
    if months.is_empty() {
        summary.parse_health = last_scan_health(&home);
        return Ok(summary);
    }

    // agent -> provider_name 映射(用所有月份桶的最近一次,先读旧月,
    // 覆盖式 → 越新越靠后 → 最终结果用最新月份的值)
    let mut agent_to_provider: BTreeMap<String, String> = BTreeMap::new();
    for m in &months {
        for (k, v) in &m.agent_to_provider_at_scan {
            agent_to_provider.insert(k.clone(), v.clone());
        }
    }

    // 收集所有日期(YYYY-MM-DD),过滤最近 N 天
    let cutoff = date_minus_days(now, days as i64);

    // 临时聚合:day -> agent -> model -> totals
    let mut day_agg: BTreeMap<String, BTreeMap<String, BTreeMap<String, UsageTotals>>> =
        BTreeMap::new();
    // agent -> model -> totals(全窗口)
    let mut agent_model_agg: BTreeMap<String, BTreeMap<String, UsageTotals>> = BTreeMap::new();

    for m in months {
        for (date, day_buckets) in &m.buckets {
            if date.as_str() < cutoff.as_str() {
                continue;
            }
            let day_entry = day_agg.entry(date.clone()).or_default();
            for (agent_model_key, totals) in day_buckets {
                let (agent_id, model) = match agent_model_key.split_once(':') {
                    Some((a, mm)) => (a.to_string(), mm.to_string()),
                    None => continue,
                };
                let event_cost = pricing::builtin_prices(&model)
                    .map(|p| {
                        p.event_cost(
                            totals.input,
                            totals.cache_read,
                            totals.cache_creation,
                            totals.output,
                        )
                    });
                let t = UsageTotals {
                    input: totals.input,
                    cache_read: totals.cache_read,
                    cache_creation: totals.cache_creation,
                    output: totals.output,
                    events: totals.events,
                    cost_usd: event_cost,
                };
                summary.total.add(&t);
                day_entry.entry(agent_id.clone()).or_default().entry(model.clone()).and_modify(|cur| cur.add(&t)).or_insert(t.clone());
                agent_model_agg
                    .entry(agent_id.clone())
                    .or_default()
                    .entry(model.clone())
                    .and_modify(|cur| cur.add(&t))
                    .or_insert(t);
            }
        }
    }

    // 装 by_day(按日期正序)
    for (date, per_agent) in day_agg {
        let mut day_usage = DayUsage {
            date: date.clone(),
            ..Default::default()
        };
        for (agent_id, per_model) in per_agent {
            let mut agent_usage = AgentUsage {
                agent_id: agent_id.clone(),
                ..Default::default()
            };
            for (model, totals) in per_model {
                agent_usage.totals.add(&totals);
                agent_usage.by_model.push(ModelUsage {
                    model,
                    totals: totals.clone(),
                    events: totals.events,
                });
            }
            agent_usage.by_model.sort_by(|a, b| b.totals.input.cmp(&a.totals.input));
            day_usage.totals.add(&agent_usage.totals);
            day_usage.by_agent.push(agent_usage);
        }
        day_usage.by_agent.sort_by(|a, b| b.totals.input.cmp(&a.totals.input));
        summary.by_day.push(day_usage);
    }

    // 装 by_agent
    for (agent_id, per_model) in agent_model_agg {
        let mut agent_usage = AgentUsage {
            agent_id: agent_id.clone(),
            ..Default::default()
        };
        for (model, totals) in per_model {
            agent_usage.totals.add(&totals);
            agent_usage.by_model.push(ModelUsage {
                model,
                totals: totals.clone(),
                events: totals.events,
            });
        }
        agent_usage.by_model.sort_by(|a, b| b.totals.input.cmp(&a.totals.input));
        summary.by_agent.push(agent_usage);
    }
    summary.by_agent.sort_by(|a, b| b.totals.input.cmp(&a.totals.input));

    // parse_health 用最近一个月的 last_scan_at(已有数据则有,否则默认)
    summary.parse_health = last_scan_health(&home);
    // pricing meta — banner / stale 提示
    let today = OffsetDateTime::now_utc().date();
    summary.pricing_meta = Some(PricingMeta::snapshot(today));
    // 让前端可以按 agent 名查到 provider
    let _ = agent_to_provider; // 当前 by_agent 不内嵌 provider;前端可另查 usage_provider_summary

    // 抑制 unused warning(同时避免完全删变量引起的死代码)
    let _ = &config;

    Ok(summary)
}

/// `usage_refresh()`:跑一次扫描,落月桶 + 增量缓存。
#[tauri::command]
pub fn usage_refresh() -> Result<aggregate::RefreshReport, String> {
    let home = real_home();
    let _guard = CONFIG_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let config = load_config(&home)?;
    let providers_meta = aggregate::providers_meta_from_config(&config);
    aggregate::refresh(&home, &config, &providers_meta).map_err(|e| e.to_string())
}

/// `usage_provider_summary()`:按 provider 名汇总所有月份桶里的消耗。
///
/// 注意:同一 provider 被多个 agent 共享时,「provider 总消耗」= 所有 agent
/// 在当时绑定该 provider 期间产生的消耗之和。绑定变化后,旧月份的桶仍
/// 保留当时的绑定快照,不会被新绑定覆盖。
#[tauri::command]
pub fn usage_provider_summary() -> Result<Vec<ProviderUsage>, String> {
    let home = real_home();
    let _guard = CONFIG_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let months = store::read_all(&home);
    // agent -> provider_name(用最新月份的快照为准)
    let mut agent_to_provider: BTreeMap<String, String> = BTreeMap::new();
    for m in &months {
        for (k, v) in &m.agent_to_provider_at_scan {
            agent_to_provider.insert(k.clone(), v.clone());
        }
    }

    // provider -> totals
    let mut provider_agg: HashMap<String, UsageTotals> = HashMap::new();

    for m in &months {
        // 每月桶用一个「当时」agent→provider 映射(优先用桶内,fallback 到最新)
        let local_map: BTreeMap<String, String> = if !m.agent_to_provider_at_scan.is_empty() {
            m.agent_to_provider_at_scan.clone()
        } else {
            agent_to_provider.clone()
        };
        for (_date, day_buckets) in &m.buckets {
            for (agent_model_key, totals) in day_buckets {
                let (agent_id, _model) = match agent_model_key.split_once(':') {
                    Some((a, mm)) => (a.to_string(), mm.to_string()),
                    None => continue,
                };
                let provider_name = local_map
                    .get(&agent_id)
                    .cloned()
                    .unwrap_or_else(|| format!("(未绑定:{})", agent_id));
                let entry = provider_agg.entry(provider_name).or_default();
                entry.input += totals.input;
                entry.cache_read += totals.cache_read;
                entry.cache_creation += totals.cache_creation;
                entry.output += totals.output;
                entry.events += totals.events;
            }
        }
    }

    let mut out: Vec<ProviderUsage> = provider_agg
        .into_iter()
        .map(|(provider_name, totals)| ProviderUsage {
            provider_name,
            totals,
        })
        .collect();
    out.sort_by(|a, b| b.totals.input.cmp(&a.totals.input));
    Ok(out)
}

/// 从最近一个月的桶读 last_scan_at,构建 ParseHealth(只读侧)。
fn last_scan_health(home: &Path) -> aggregate::ParseHealth {
    let months = store::read_all(home);
    if let Some(last) = months.last() {
        aggregate::ParseHealth {
            matched_ratio: 1.0,
            per_agent: vec![],
            errors: vec![],
            last_scan_at: if last.last_scan_at.is_empty() {
                None
            } else {
                Some(last.last_scan_at.clone())
            },
            added_events_deduped: 0,
        }
    } else {
        aggregate::ParseHealth::default()
    }
}

/// 从今天往回数 N 天的日期字符串(`YYYY-MM-DD`)。用 time crate 简化。
fn date_minus_days(now: OffsetDateTime, days: i64) -> String {
    let target = now - time::Duration::days(days);
    format!(
        "{:04}-{:02}-{:02}",
        target.year(),
        u8::from(target.month()),
        target.day()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::test_util::TempHome;

    #[test]
    fn date_minus_days_basic() {
        let now = OffsetDateTime::from_unix_timestamp(1724860800).unwrap(); // 2024-08-28 UTC
        assert_eq!(date_minus_days(now, 0), "2024-08-28");
        assert_eq!(date_minus_days(now, 7), "2024-08-21");
    }

    #[test]
    fn usage_totals_add_accumulates() {
        let mut a = UsageTotals {
            input: 1,
            cache_read: 2,
            cache_creation: 3,
            output: 4,
            events: 5,
            cost_usd: Some(0.10),
        };
        let b = UsageTotals {
            input: 10,
            cache_read: 20,
            cache_creation: 30,
            output: 40,
            events: 50,
            cost_usd: Some(0.20),
        };
        a.add(&b);
        assert_eq!(a.input, 11);
        assert_eq!(a.cache_read, 22);
        assert_eq!(a.cache_creation, 33);
        assert_eq!(a.output, 44);
        assert_eq!(a.events, 55);
        // cost_usd 也累加(0.10 + 0.20 = 0.30,f64 浮点误差 ~1e-16)
        let c = a.cost_usd.expect("cost_usd 应该 Some");
        assert!(
            (c - 0.30).abs() < 1e-9,
            "expected ~0.30, got {}",
            c
        );
    }

    #[test]
    fn pricing_meta_snapshot_is_well_formed() {
        use time::OffsetDateTime;
        let today = OffsetDateTime::now_utc().date();
        let meta = PricingMeta::snapshot(today);
        assert!(!meta.snapshot_date.is_empty(), "snapshot_date 非空");
        assert!(meta.covered_models >= 80, "至少 80 个 model,实际 {}", meta.covered_models);
        assert!(meta.age_days >= 0);
        // is_stale 在当前 SNAPSHOT_DATE + 30 天窗口内应为 false
        assert!(!meta.is_stale || meta.age_days > 30);
    }

    #[test]
    fn empty_home_returns_empty_summary() {
        let tmp = TempHome::new();
        // 没有 buckets,但 last_scan_health 应是 default
        // (直接调 last_scan_health 验证)
        let h = last_scan_health(tmp.path());
        assert!(h.last_scan_at.is_none());
        assert_eq!(h.matched_ratio, 0.0); // default = 0(无数据时不假装是完美匹配)
    }
}
