//! Append-only 月桶 `~/.clawbox/usage/usage-YYYY-MM.json`。
//!
//! 核心:与原始 JSONL 格式解耦,抗 30 天会话清理。
//!
//! `agent_to_provider_at_scan` 在每次扫描时按当时 `Config.agent_providers`
//! 快照落库 — 后续 binding 变化不影响历史桶。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// 月桶 key = `agent_id:model`(BTreeMap 保证稳定遍历序)。
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
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
}

impl BucketTotals {
    pub fn add(&mut self, other: &BucketTotals) {
        self.input += other.input;
        self.cache_read += other.cache_read;
        self.cache_creation += other.cache_creation;
        self.output += other.output;
        self.events += other.events;
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
        let mut a = BucketTotals { input: 1, cache_read: 2, cache_creation: 3, output: 4, events: 5 };
        let b = BucketTotals { input: 10, cache_read: 20, cache_creation: 30, output: 40, events: 50 };
        a.add(&b);
        assert_eq!(a.input, 11);
        assert_eq!(a.cache_read, 22);
        assert_eq!(a.cache_creation, 33);
        assert_eq!(a.output, 44);
        assert_eq!(a.events, 55);
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
}
