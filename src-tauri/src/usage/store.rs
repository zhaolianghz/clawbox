//! Append-only 月桶 `~/.clawbox/usage/usage-YYYY-MM.json`。
//!
//! 核心:与原始 JSONL 格式解耦,抗 30 天会话清理。
//!
//! `agent_to_provider_at_scan` 在每次扫描时按当时 `Config.agent_providers`
//! 快照落库 — 后续 binding 变化不影响历史桶。

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::Path;

/// 月桶 key = `agent_id:model`(BTreeMap 保证稳定遍历序)。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BucketTotals {
    #[serde(default)]
    pub input: u64,
    #[serde(default)]
    pub cache_read: u64,
    #[serde(default)]
    pub cache_creation: u64,
    #[serde(default)]
    pub output: u64,
    #[serde(default)]
    pub events: u64,
    /// 本桶内累计成本(USD)。**写入时**按 model id 直接查 builtin_prices 算,
    /// 不感知 provider 的 alias/override(那俩是用户态,store 不依赖)。
    /// 用户修改 alias 后,旧桶的 cost_usd 不会自动重算(v1 简化) —
    /// 前端如需最新值,可在 UI 提供「按当前 alias 重算」按钮单独触发。
    /// None = 写入时尚未知价(model 不在默认表里),前端显示"—"。
    #[serde(default)]
    pub cost_usd: Option<f64>,
}

impl BucketTotals {
    pub fn add(&mut self, other: &BucketTotals) {
        self.input += other.input;
        self.cache_read += other.cache_read;
        self.cache_creation += other.cache_creation;
        self.output += other.output;
        self.events += other.events;
        // cost_usd 累加(None + Some = Some;Some + Some = Some;None + None = None)
        self.cost_usd = match (self.cost_usd, other.cost_usd) {
            (Some(a), Some(b)) => Some(a + b),
            (a, b) => a.or(b),
        };
    }
}

/// 单个月桶文件。`day -> {agent_id:model -> BucketTotals}`。
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MonthBucket {
    pub version: u32,
    pub month: String, // "YYYY-MM"
    pub buckets: BTreeMap<String, BTreeMap<String, BucketTotals>>,
    /// 扫描时的 agent -> primary provider 绑定快照(后续 binding 改动不影响历史)。
    #[serde(default)]
    pub agent_to_provider_at_scan: BTreeMap<String, String>,
    /// 扫描时的 agent -> fallback provider 列表快照(v1 仅 hermes 实际生效)。
    #[serde(default)]
    pub agent_fallbacks_at_scan: BTreeMap<String, Vec<String>>,
    /// RFC3339 UTC。
    #[serde(default)]
    pub last_scan_at: String,
    /// 已计 event 的去重集合 — `(agent_id, session_id, event_id)` 三元组。
    /// 多次 refresh 同一份 events 重算(Claude 按 (session, msg_id) 文件内
    /// dedup、Codex 按 last_total 差值,两侧天然稳定)时,已计的不再 add。
    /// 序列化时转 vec(确定性顺序:sort),反序列化回 HashSet。
    /// v1 之前缺失此字段会导致「刷新两次 = 数字翻倍」的 bug。
    #[serde(
        default,
        rename = "seen_events",
        serialize_with = "serialize_seen_events",
        deserialize_with = "deserialize_seen_events"
    )]
    pub seen_events: HashSet<SeenEventKey>,
}

/// 去重 key 三元组 — 单独类型便于 derive Eq/Hash 和 serde 优化。
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SeenEventKey(pub String, pub String, pub String); // (agent, session, event)

fn serialize_seen_events<S: serde::Serializer>(
    set: &HashSet<SeenEventKey>,
    s: S,
) -> Result<S::Ok, S::Error> {
    let mut v: Vec<&SeenEventKey> = set.iter().collect();
    v.sort();
    v.serialize(s)
}

fn deserialize_seen_events<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<HashSet<SeenEventKey>, D::Error> {
    let v: Vec<SeenEventKey> = Vec::deserialize(d)?;
    Ok(v.into_iter().collect())
}

pub const CURRENT_VERSION: u32 = 1;

/// 读取某月桶,不存在返回默认空桶(版本号 + month 由调用方预填)。
pub fn read_month(home: &Path, year: i32, month: u8) -> MonthBucket {
    let path = super::month_bucket_path(home, year, month);
    if !path.exists() {
        return MonthBucket {
            version: CURRENT_VERSION,
            month: format!("{:04}-{:02}", year, month),
            ..Default::default()
        };
    }
    let raw = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => return MonthBucket {
            version: CURRENT_VERSION,
            month: format!("{:04}-{:02}", year, month),
            ..Default::default()
        },
    };
    match serde_json::from_str::<MonthBucket>(&raw) {
        Ok(b) => b,
        Err(_) => {
            // 损坏的桶文件视为空,不抛(降级可见)
            MonthBucket {
                version: CURRENT_VERSION,
                month: format!("{:04}-{:02}", year, month),
                ..Default::default()
            }
        }
    }
}

/// 把月桶写回磁盘。原子性:写到临时文件再 rename。
pub fn write_month(home: &Path, bucket: &MonthBucket) -> Result<(), String> {
    let path = super::month_bucket_path(home, parse_month(&bucket.month)?.0, parse_month(&bucket.month)?.1);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create_dir_all: {}", e))?;
    }
    let tmp = path.with_extension("json.tmp");
    let content =
        serde_json::to_string_pretty(bucket).map_err(|e| format!("serialize: {}", e))?;
    fs::write(&tmp, content).map_err(|e| format!("write tmp: {}", e))?;
    fs::rename(&tmp, &path).map_err(|e| format!("rename: {}", e))?;
    Ok(())
}

/// 增量更新某天某 (agent, model) 的桶。同月二次调用累加。
///
/// ⚠️ 不带 event 级 dedup — 适合「已知不会重复」的场景(单元测试、迁移工具)。
/// 生产路径请用 `append_event` 或 `append_events_batch`。
pub fn append_bucket(
    home: &Path,
    year: i32,
    month: u8,
    day: &str,
    agent_model_key: &str,
    delta: &BucketTotals,
) -> Result<(), String> {
    let mut b = read_month(home, year, month);
    let day_buckets = b.buckets.entry(day.to_string()).or_default();
    let entry = day_buckets.entry(agent_model_key.to_string()).or_default();
    entry.add(delta);
    write_month(home, &b)
}

/// 单 event append,带 seen_events 去重(防止多次 refresh 累加)。
///
/// `agent_id` + `event.session_id` + `event.event_id` 三元组已存在桶里 → 跳过,
/// 完全幂等。返回 `true` 表示实际写入了桶,`false` 表示命中去重被跳过。
pub fn append_event(
    home: &Path,
    year: i32,
    month: u8,
    agent_id: &str,
    event: &crate::usage::UsageEvent,
) -> Result<bool, String> {
    let day = event
        .ts
        .get(..10)
        .filter(|s| s.len() == 10)
        .unwrap_or("unknown");
    let key = SeenEventKey(agent_id.to_string(), event.session_id.clone(), event.event_id.clone());

    let mut b = read_month(home, year, month);
    if b.seen_events.contains(&key) {
        return Ok(false);
    }
    b.seen_events.insert(key);

    let bucket_key = format!("{}:{}", agent_id, event.model);
    let event_cost = super::pricing::builtin_prices(&event.model).map(|p| {
        p.event_cost(
            event.input_tokens,
            event.cache_read_tokens,
            event.cache_creation_tokens,
            event.output_tokens,
        )
    });
    let delta = BucketTotals {
        input: event.input_tokens,
        cache_read: event.cache_read_tokens,
        cache_creation: event.cache_creation_tokens,
        output: event.output_tokens,
        events: 1,
        cost_usd: event_cost,
    };
    let day_buckets = b.buckets.entry(day.to_string()).or_default();
    let entry = day_buckets.entry(bucket_key).or_default();
    entry.add(&delta);
    write_month(home, &b)?;
    Ok(true)
}

/// 批量 append:按 (year, month) 分桶,每个桶一次性 read + write。
///
/// 复杂度:`O(months * events)` IO 操作,从 `O(events)` 次 read+write 降到
/// `O(months)` 次(实际就是 1~2 次,因为同 agent 的 events 跨月不多)。
///
/// `events` 是同一 agent 的(跨月 OK);`agent_id` 是这批 events 的归属。
/// 返回 `(written, deduped)` = (实际新增 event 数, 去重跳过 event 数)。
pub fn append_events_batch(
    home: &Path,
    agent_id: &str,
    events: &[crate::usage::UsageEvent],
) -> Result<(usize, usize), String> {
    use std::collections::BTreeMap;
    if events.is_empty() {
        return Ok((0, 0));
    }

    // 第一遍:按 (year, month) 把 events 分桶(其它不可解析 day 的归"unknown")
    let mut by_month: BTreeMap<(i32, u8), Vec<&crate::usage::UsageEvent>> = BTreeMap::new();
    let mut bad_ts = 0usize;
    for ev in events {
        let day = ev.ts.get(..10).filter(|s| s.len() == 10).unwrap_or("");
        if day.is_empty() {
            bad_ts += 1;
            continue;
        }
        match parse_day_to_year_month(day) {
            Ok(ym) => by_month.entry(ym).or_default().push(ev),
            Err(_) => bad_ts += 1,
        }
    }
    let _ = bad_ts; // 暂不暴露到返回值;v1 视为 silently skip(同原 append_event 行为)

    let mut written = 0usize;
    let mut deduped = 0usize;

    for ((year, month), month_events) in by_month {
        let mut bucket = read_month(home, year, month);
        let mut new_writes_for_this_bucket = 0usize;

        for ev in &month_events {
            let key = SeenEventKey(
                agent_id.to_string(),
                ev.session_id.clone(),
                ev.event_id.clone(),
            );
            if bucket.seen_events.contains(&key) {
                deduped += 1;
                continue;
            }
            bucket.seen_events.insert(key);

            let day = ev.ts.get(..10).unwrap_or("unknown");
            let bucket_key = format!("{}:{}", agent_id, ev.model);
            let event_cost = super::pricing::builtin_prices(&ev.model).map(|p| {
                p.event_cost(
                    ev.input_tokens,
                    ev.cache_read_tokens,
                    ev.cache_creation_tokens,
                    ev.output_tokens,
                )
            });
            let delta = BucketTotals {
                input: ev.input_tokens,
                cache_read: ev.cache_read_tokens,
                cache_creation: ev.cache_creation_tokens,
                output: ev.output_tokens,
                events: 1,
                cost_usd: event_cost,
            };
            let day_buckets = bucket.buckets.entry(day.to_string()).or_default();
            let entry = day_buckets.entry(bucket_key).or_default();
            entry.add(&delta);
            new_writes_for_this_bucket += 1;
        }

        if new_writes_for_this_bucket > 0 {
            write_month(home, &bucket)?;
        }
        written += new_writes_for_this_bucket;
    }

    Ok((written, deduped))
}

/// 从 day 字符串解析 year/month(供 batch 内部使用)。
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
/// 读取所有月份桶(返回按月份顺序的 vec)。
pub fn read_all(home: &Path) -> Vec<MonthBucket> {
    let dir = super::usage_dir(home);
    if !dir.exists() {
        return vec![];
    }
    let mut months = Vec::new();
    let entries = match fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return vec![],
    };
    for entry in entries.flatten() {
        let p = entry.path();
        let fname = match p.file_name().and_then(|s| s.to_str()) {
            Some(s) => s,
            None => continue,
        };
        if !fname.starts_with("usage-") || !fname.ends_with(".json") {
            continue;
        }
        let stem = &fname[6..fname.len() - 5]; // strip "usage-" + ".json"
        if let Some((y, m)) = parse_month(stem).ok() {
            months.push(read_month(home, y, m));
        }
    }
    // 按 month 排序
    months.sort_by(|a, b| a.month.cmp(&b.month));
    months
}

/// 解析 "YYYY-MM" 字符串。
pub fn parse_month(s: &str) -> Result<(i32, u8), String> {
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 2 {
        return Err(format!("invalid month: {}", s));
    }
    let y = parts[0].parse::<i32>().map_err(|e| format!("year: {}", e))?;
    let m = parts[1].parse::<u8>().map_err(|e| format!("month: {}", e))?;
    if !(1..=12).contains(&m) {
        return Err(format!("month out of range: {}", m));
    }
    Ok((y, m))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct LocalHome(std::path::PathBuf);
    impl LocalHome {
        fn new() -> Self {
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let n = COUNTER.fetch_add(1, Ordering::SeqCst);
            let dir = std::env::temp_dir().join(format!(
                "clawbox-usage-store-test-{}-{}",
                std::process::id(),
                n
            ));
            std::fs::create_dir_all(&dir).unwrap();
            LocalHome(dir)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for LocalHome {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn append_bucket_creates_and_accumulates() {
        let tmp = LocalHome::new();
        let delta1 = BucketTotals { input: 100, output: 50, events: 1, ..Default::default() };
        append_bucket(tmp.path(), 2026, 8, "2026-08-29", "claude-code:sonnet", &delta1).unwrap();

        let read = read_month(tmp.path(), 2026, 8);
        assert_eq!(read.month, "2026-08");
        let day = read.buckets.get("2026-08-29").unwrap();
        let bucket = day.get("claude-code:sonnet").unwrap();
        assert_eq!(bucket.input, 100);
        assert_eq!(bucket.output, 50);
        assert_eq!(bucket.events, 1);

        // 同月再 append,应累加而非覆盖
        let delta2 = BucketTotals { input: 200, cache_read: 50, events: 2, ..Default::default() };
        append_bucket(tmp.path(), 2026, 8, "2026-08-29", "claude-code:sonnet", &delta2).unwrap();

        let read = read_month(tmp.path(), 2026, 8);
        let bucket = read
            .buckets
            .get("2026-08-29")
            .unwrap()
            .get("claude-code:sonnet")
            .unwrap();
        assert_eq!(bucket.input, 300);
        assert_eq!(bucket.output, 50);
        assert_eq!(bucket.cache_read, 50);
        assert_eq!(bucket.events, 3);
    }

    #[test]
    fn read_all_returns_sorted_months() {
        let tmp = LocalHome::new();
        append_bucket(
            tmp.path(),
            2026,
            9,
            "2026-09-01",
            "codex:gpt",
            &BucketTotals { input: 1, events: 1, ..Default::default() },
        )
        .unwrap();
        append_bucket(
            tmp.path(),
            2026,
            7,
            "2026-07-15",
            "claude-code:opus",
            &BucketTotals { input: 2, events: 1, ..Default::default() },
        )
        .unwrap();
        append_bucket(
            tmp.path(),
            2026,
            8,
            "2026-08-20",
            "codex:gpt",
            &BucketTotals { input: 3, events: 1, ..Default::default() },
        )
        .unwrap();

        let months = read_all(tmp.path());
        assert_eq!(months.len(), 3);
        assert_eq!(months[0].month, "2026-07");
        assert_eq!(months[1].month, "2026-08");
        assert_eq!(months[2].month, "2026-09");
    }

    #[test]
    fn missing_bucket_returns_default() {
        let tmp = LocalHome::new();
        let b = read_month(tmp.path(), 2026, 8);
        assert_eq!(b.month, "2026-08");
        assert_eq!(b.buckets.len(), 0);
    }

    #[test]
    fn parse_month_handles_valid_and_invalid() {
        assert_eq!(parse_month("2026-08").unwrap(), (2026, 8));
        assert_eq!(parse_month("2024-01").unwrap(), (2024, 1));
        assert!(parse_month("2026-13").is_err());
        assert!(parse_month("not-a-month").is_err());
        assert!(parse_month("2026-08-15").is_err());
    }

    #[test]
    fn agent_to_provider_snapshot_is_preserved() {
        let tmp = LocalHome::new();
        let mut b = read_month(tmp.path(), 2026, 8);
        b.agent_to_provider_at_scan.insert("claude-code".into(), "p-1".into());
        write_month(tmp.path(), &b).unwrap();
        let read = read_month(tmp.path(), 2026, 8);
        assert_eq!(read.agent_to_provider_at_scan.get("claude-code"), Some(&"p-1".to_string()));
    }

    #[test]
    fn bucket_totals_add_accumulates_all_fields() {
        let mut a = BucketTotals { input: 1, cache_read: 2, cache_creation: 3, output: 4, events: 5, cost_usd: None };
        let b = BucketTotals { input: 10, cache_read: 20, cache_creation: 30, output: 40, events: 50, cost_usd: None };
        a.add(&b);
        assert_eq!(a.input, 11);
        assert_eq!(a.cache_read, 22);
        assert_eq!(a.cache_creation, 33);
        assert_eq!(a.output, 44);
        assert_eq!(a.events, 55);
    }

    #[test]
    fn bucket_totals_add_merges_cost_usd() {
        // None + Some = Some
        let mut a = BucketTotals { cost_usd: None, ..Default::default() };
        let b = BucketTotals { cost_usd: Some(1.50), ..Default::default() };
        a.add(&b);
        assert_eq!(a.cost_usd, Some(1.50));
        // Some + Some = sum
        let c = BucketTotals { cost_usd: Some(0.75), ..Default::default() };
        a.add(&c);
        assert_eq!(a.cost_usd, Some(2.25));
        // None + None = None
        let mut d = BucketTotals { cost_usd: None, ..Default::default() };
        let e = BucketTotals { cost_usd: None, ..Default::default() };
        d.add(&e);
        assert_eq!(d.cost_usd, None);
        // Some + None: 保持 Some 不变
        let mut f = BucketTotals { cost_usd: Some(5.0), ..Default::default() };
        let g = BucketTotals { cost_usd: None, ..Default::default() };
        f.add(&g);
        assert_eq!(f.cost_usd, Some(5.0));
    }

    #[test]
    fn corrupt_bucket_file_treated_as_empty() {
        let tmp = LocalHome::new();
        let dir = tmp.path().join(".clawbox/usage");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("usage-2026-08.json"), "not valid json").unwrap();
        let b = read_month(tmp.path(), 2026, 8);
        assert_eq!(b.buckets.len(), 0);
    }

    #[test]
    fn append_event_dedupes_repeat_calls() {
        let tmp = LocalHome::new();
        let ev = crate::usage::UsageEvent {
            ts: "2026-08-29T10:00:00Z".into(),
            session_id: "sess-1".into(),
            event_id: "msg-1".into(),
            model: "claude-sonnet".into(),
            input_tokens: 100,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            output_tokens: 50,
        };
        // 第一次:写
        assert_eq!(append_event(tmp.path(), 2026, 8, "claude-code", &ev).unwrap(), true);
        // 第二次/第三次:同 key → 跳过(不累加)
        assert_eq!(append_event(tmp.path(), 2026, 8, "claude-code", &ev).unwrap(), false);
        assert_eq!(append_event(tmp.path(), 2026, 8, "claude-code", &ev).unwrap(), false);

        let b = read_month(tmp.path(), 2026, 8);
        let day = b.buckets.get("2026-08-29").unwrap();
        let bucket = day.get("claude-code:claude-sonnet").unwrap();
        // 三次调用只算一次
        assert_eq!(bucket.input, 100);
        assert_eq!(bucket.output, 50);
        assert_eq!(bucket.events, 1);
        // seen_events 记录了 1 个 key
        assert_eq!(b.seen_events.len(), 1);
    }

    #[test]
    fn append_event_distinguishes_agents_with_same_session() {
        let tmp = LocalHome::new();
        let ev = crate::usage::UsageEvent {
            ts: "2026-08-29T10:00:00Z".into(),
            session_id: "shared-sess".into(),
            event_id: "msg-1".into(),
            model: "m".into(),
            input_tokens: 10,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            output_tokens: 0,
        };
        // claude-code 写了
        append_event(tmp.path(), 2026, 8, "claude-code", &ev).unwrap();
        // codex 也用同一个 session_id+event_id,被视为不同 event(agent 不同)
        append_event(tmp.path(), 2026, 8, "codex", &ev).unwrap();

        let b = read_month(tmp.path(), 2026, 8);
        let day = b.buckets.get("2026-08-29").unwrap();
        assert!(day.contains_key("claude-code:m"));
        assert!(day.contains_key("codex:m"));
        assert_eq!(b.seen_events.len(), 2);
    }

    #[test]
    fn append_events_batch_writes_once_per_month_no_double_count() {
        // **核心 bug 验证**:同批 events 跑两次 batch,数字必须稳定。
        let tmp = LocalHome::new();
        let events: Vec<crate::usage::UsageEvent> = (0..100)
            .map(|i| crate::usage::UsageEvent {
                ts: "2026-08-29T10:00:00Z".into(),
                session_id: "s".into(),
                event_id: format!("msg-{i}"),
                model: "m".into(),
                input_tokens: 10,
                cache_read_tokens: 0,
                cache_creation_tokens: 0,
                output_tokens: 5,
            })
            .collect();
        let (written, deduped) = append_events_batch(tmp.path(), "claude-code", &events).unwrap();
        assert_eq!(written, 100);
        assert_eq!(deduped, 0);
        // 第二次跑同批 → 全部 dedup,不累加
        let (w2, d2) = append_events_batch(tmp.path(), "claude-code", &events).unwrap();
        assert_eq!(w2, 0);
        assert_eq!(d2, 100);

        let b = read_month(tmp.path(), 2026, 8);
        let day = b.buckets.get("2026-08-29").unwrap();
        let bucket = day.get("claude-code:m").unwrap();
        // 只累加一次 — 防止 100×100 翻倍(老 bug)
        assert_eq!(bucket.input, 100 * 10);
        assert_eq!(bucket.output, 100 * 5);
        assert_eq!(bucket.events, 100);
    }

    #[test]
    fn append_events_batch_groups_by_month() {
        let tmp = LocalHome::new();
        let events = vec![
            crate::usage::UsageEvent {
                ts: "2026-07-31T23:00:00Z".into(),
                session_id: "s1".into(),
                event_id: "e1".into(),
                model: "m".into(),
                input_tokens: 1,
                cache_read_tokens: 0,
                cache_creation_tokens: 0,
                output_tokens: 0,
            },
            crate::usage::UsageEvent {
                ts: "2026-08-01T01:00:00Z".into(),
                session_id: "s1".into(),
                event_id: "e2".into(),
                model: "m".into(),
                input_tokens: 2,
                cache_read_tokens: 0,
                cache_creation_tokens: 0,
                output_tokens: 0,
            },
        ];
        let (written, _) = append_events_batch(tmp.path(), "claude-code", &events).unwrap();
        assert_eq!(written, 2);

        let july = read_month(tmp.path(), 2026, 7);
        let aug = read_month(tmp.path(), 2026, 8);
        assert_eq!(
            july
                .buckets
                .get("2026-07-31")
                .unwrap()
                .get("claude-code:m")
                .unwrap()
                .input,
            1
        );
        assert_eq!(
            aug.buckets
                .get("2026-08-01")
                .unwrap()
                .get("claude-code:m")
                .unwrap()
                .input,
            2
        );
    }

    #[test]
    fn seen_events_serializes_in_deterministic_order() {
        // 序列化确定性:同一份 seen_events → 同一份 JSON(git diff 友好)
        let tmp = LocalHome::new();
        append_event(
            tmp.path(),
            2026,
            8,
            "z-agent",
            &crate::usage::UsageEvent {
                ts: "2026-08-29T10:00:00Z".into(),
                session_id: "zz".into(),
                event_id: "e1".into(),
                model: "m".into(),
                input_tokens: 0,
                cache_read_tokens: 0,
                cache_creation_tokens: 0,
                output_tokens: 0,
            },
        )
        .unwrap();
        append_event(
            tmp.path(),
            2026,
            8,
            "a-agent",
            &crate::usage::UsageEvent {
                ts: "2026-08-29T10:00:00Z".into(),
                session_id: "aa".into(),
                event_id: "e2".into(),
                model: "m".into(),
                input_tokens: 0,
                cache_read_tokens: 0,
                cache_creation_tokens: 0,
                output_tokens: 0,
            },
        )
        .unwrap();
        let raw =
            std::fs::read_to_string(tmp.path().join(".clawbox/usage/usage-2026-08.json"))
                .unwrap();
        // sort 后 a-agent 必在 z-agent 之前
        let pos_a = raw.find("\"a-agent\"").unwrap();
        let pos_z = raw.find("\"z-agent\"").unwrap();
        assert!(pos_a < pos_z, "expected a-agent before z-agent in JSON");
    }
}
