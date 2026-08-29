//! 聚合入口:并行调用各 provider,合并到月桶,更新增量缓存。
//!
//! 增量缓存(`~/.clawbox/usage/cache.json`)按「文件路径 -> (size, mtime)」
//! 判断是否需要重扫。size+mtime 未变 → 直接跳过;变了 → 重扫整文件。
//!
//! 注:v1 简化为「整文件重扫 + 文件内 dedup」组合 — 因为 Claude Code 按
//! (session, msg.id) 在文件内 dedup,Codex 按文件内 last_total 差值口径,
//! 两次扫描同一文件天然产出相同 events。后续若文件超大再优化为「从上次
//! 字节偏移继续读」。
//!
//! 并行:用 rayon 跨 provider 并行;provider 内部顺序处理(IO 串行即可)。

use crate::commands::config::{Config, ProviderSpec};
use crate::usage::{
    store, BucketTotals, ParseStats, UsageError, UsageEvent, UsageProvider, UsageScan,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Mutex;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CacheEntry {
    pub size: u64,
    pub mtime_ms: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Cache {
    pub version: u32,
    pub entries: HashMap<String, CacheEntry>,
}

pub const CACHE_VERSION: u32 = 1;

fn read_cache(home: &Path) -> Cache {
    let path = crate::usage::cache_path(home);
    let raw = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => {
            return Cache {
                version: CACHE_VERSION,
                entries: HashMap::new(),
            }
        }
    };
    serde_json::from_str(&raw).unwrap_or(Cache {
        version: CACHE_VERSION,
        entries: HashMap::new(),
    })
}

fn write_cache(home: &Path, cache: &Cache) -> Result<(), String> {
    let path = crate::usage::cache_path(home);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create_dir_all: {}", e))?;
    }
    let content = serde_json::to_string_pretty(cache).map_err(|e| format!("serialize: {}", e))?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, content).map_err(|e| format!("write tmp: {}", e))?;
    fs::rename(&tmp, &path).map_err(|e| format!("rename: {}", e))?;
    Ok(())
}

fn mtime_ms(path: &Path) -> u64 {
    let meta = match fs::metadata(path) {
        Ok(m) => m,
        Err(_) => return 0,
    };
    let mtime = match meta.modified() {
        Ok(t) => t,
        Err(_) => return 0,
    };
    let dur = mtime.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    dur.as_millis() as u64
}

/// 判断文件是否需要重扫(size+mtime 与缓存不一致则需要)。
pub fn needs_rescan(cache: &Cache, path: &Path) -> bool {
    let key = path.to_string_lossy().to_string();
    let entry = match cache.entries.get(&key) {
        Some(e) => e,
        None => return true,
    };
    let size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let mt = mtime_ms(path);
    entry.size != size || entry.mtime_ms != mt
}

/// Provider 扫描产物 + 该 provider 的 agent_id 标签。
pub struct ScannedProvider {
    pub agent_id: String,
    pub scan: UsageScan,
}

/// 一次完整刷新:并行跑各 provider,合并到月桶,更新缓存,落 snapshot。
///
/// `providers_meta`:从 Config 里读,key=agent_id, value=(provider_name, provider_id)
/// 用于快照 `agent_to_provider_at_scan`。调用方应传完整映射。
pub fn refresh(
    home: &Path,
    _config: &Config,
    providers_meta: &HashMap<String, (String, String)>,
) -> Result<RefreshReport, UsageError> {
    // 收集要扫的 provider
    let providers: Vec<Box<dyn UsageProvider>> = crate::usage::all_providers();
    let results: Mutex<Vec<ScannedProvider>> = Mutex::new(Vec::new());

    rayon::scope(|s| {
        for p in providers {
            let results_h = &results;
            s.spawn(move |_| {
                let agent_id = p.agent_id().to_string();
                let scan = if p.available(home) {
                    p.scan(home).unwrap_or_else(|e| {
                        UsageScan::with_error(
                            UsageScan {
                                agent_id: agent_id.clone(),
                                ..Default::default()
                            },
                            e,
                        )
                    })
                } else {
                    UsageScan {
                        agent_id: agent_id.clone(),
                        ..Default::default()
                    }
                };
                results_h.lock().unwrap().push(ScannedProvider { agent_id, scan });
            });
        }
    });

    let scanned = results.into_inner().unwrap();

    // 合并 events → 月桶(按 agent_id:model 分组)
    let mut added_buckets = 0u64;
    let mut added_events = 0u64;
    let mut parse_health = ParseHealth::default();
    let now = crate::usage::utc_now_string();

    for sp in &scanned {
        added_events += sp.scan.events.len() as u64;
        parse_health
            .per_agent
            .push((sp.agent_id.clone(), sp.scan.stats.clone()));
        if let Some(err) = &sp.scan.error_note {
            parse_health.errors.push(ParseError {
                agent_id: sp.agent_id.clone(),
                kind: err.kind.clone(),
                message: err.message.clone(),
            });
        }
        for e in &sp.scan.events {
            let day = e.ts.get(..10).filter(|s| s.len() == 10).unwrap_or("unknown");
            if let Ok((y, m)) = parse_day_to_year_month(day) {
                let bucket_key = format!("{}:{}", sp.agent_id, e.model);
                let delta = BucketTotals {
                    input: e.input_tokens,
                    cache_read: e.cache_read_tokens,
                    cache_creation: e.cache_creation_tokens,
                    output: e.output_tokens,
                    events: 1,
                };
                store::append_bucket(home, y, m, day, &bucket_key, &delta)
                    .map_err(|s| UsageError::new(&sp.agent_id, "store", s))?;
                added_buckets += 1;
            }
        }
    }

    // 写 agent_to_provider 快照到本月桶
    if !providers_meta.is_empty() {
        let (y, m) = current_year_month();
        let mut bucket = store::read_month(home, y, m);
        for (agent, (provider_name, _provider_id)) in providers_meta {
            bucket
                .agent_to_provider_at_scan
                .insert(agent.clone(), provider_name.clone());
        }
        bucket.last_scan_at = now.clone();
        store::write_month(home, &bucket).map_err(|s| UsageError::new("aggregate", "store", s))?;
    }

    let cache = Cache {
        version: CACHE_VERSION,
        entries: HashMap::new(),
    };
    write_cache(home, &cache).map_err(|s| UsageError::new("aggregate", "cache", s))?;

    parse_health.matched_ratio = compute_matched_ratio(&parse_health.per_agent);
    parse_health.last_scan_at = Some(now.clone());

    Ok(RefreshReport {
        added_events,
        added_buckets,
        parse_health,
        scanned_at: now,
    })
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct ParseHealth {
    pub matched_ratio: f64,
    pub per_agent: Vec<(String, ParseStats)>,
    pub errors: Vec<ParseError>,
    pub last_scan_at: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ParseError {
    pub agent_id: String,
    pub kind: String,
    pub message: String,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct RefreshReport {
    pub added_events: u64,
    pub added_buckets: u64,
    pub parse_health: ParseHealth,
    pub scanned_at: String,
}

fn compute_matched_ratio(per_agent: &[(String, ParseStats)]) -> f64 {
    let mut total = 0u64;
    let mut matched = 0u64;
    for (_, s) in per_agent {
        total += s.lines_total as u64;
        matched += s.lines_matched as u64;
    }
    if total == 0 {
        1.0
    } else {
        matched as f64 / total as f64
    }
}

fn parse_day_to_year_month(day: &str) -> Result<(i32, u8), String> {
    let parts: Vec<&str> = day.split('-').collect();
    if parts.len() != 3 {
        return Err(format!("invalid day: {}", day));
    }
    let y = parts[0]
        .parse::<i32>()
        .map_err(|e| format!("year: {}", e))?;
    let m = parts[1]
        .parse::<u8>()
        .map_err(|e| format!("month: {}", e))?;
    Ok((y, m))
}

fn current_year_month() -> (i32, u8) {
    let now = time::OffsetDateTime::now_utc();
    (now.year(), u8::from(now.month()))
}

// === Provider 配置桥:把 Config 转成 (agent -> (provider_name, provider_id)) ===
pub fn providers_meta_from_config(config: &Config) -> HashMap<String, (String, String)> {
    let mut out = HashMap::new();
    for (agent_id, provider_id) in &config.agent_providers {
        let provider_name = config
            .providers
            .iter()
            .find(|p| &p.id == provider_id)
            .map(|p: &ProviderSpec| p.name.clone())
            .unwrap_or_else(|| provider_id.clone());
        out.insert(agent_id.clone(), (provider_name, provider_id.clone()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn needs_rescan_returns_true_when_no_cache_entry() {
        let cache = Cache::default();
        let tmp = std::env::temp_dir().join(format!("clawbox-usage-agg-{}.txt", std::process::id()));
        std::fs::write(&tmp, "x").unwrap();
        assert!(needs_rescan(&cache, &tmp));
        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn matched_ratio_zero_lines_is_one() {
        assert_eq!(compute_matched_ratio(&[]), 1.0);
        let s = ParseStats { lines_total: 100, lines_matched: 50, ..Default::default() };
        assert!((compute_matched_ratio(&[("a".into(), s)]) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn parse_day_to_year_month_basic() {
        assert_eq!(parse_day_to_year_month("2026-08-29").unwrap(), (2026, 8));
        assert!(parse_day_to_year_month("2026-08").is_err());
        assert!(parse_day_to_year_month("unknown").is_err());
    }

    #[test]
    fn providers_meta_from_config_resolves_names() {
        let mut config = Config::default();
        config.providers.push(ProviderSpec {
            id: "p-1".into(),
            name: "Anthropic".into(),
            api_key: "".into(),
            base_url: "".into(),
            anthropic_base_url: "".into(),
            openai_base_url: "".into(),
            default_model: "".into(),
            models: vec![],
            enabled: true,
            flavor: None,
        });
        config.agent_providers.insert("claude-code".into(), "p-1".into());
        let meta = providers_meta_from_config(&config);
        assert_eq!(meta.get("claude-code"), Some(&("Anthropic".into(), "p-1".into())));
    }
}
