//! Kimi Code CLI adapter:
//! 解析 `~/.kimi-code/agents/<cwd-encoded>/agents/main/wire.jsonl`。
//!
//! 字段口径(Kimi 官方 docs):
//! - `usage.inputOther` → input_tokens
//! - `usage.output` → output_tokens
//! - `usage.inputCacheRead` → cache_read_tokens
//! - `usage.inputCacheCreation` → cache_creation_tokens
//! - 顶层 `model` 字段取 model id;缺失 → "unknown"
//!
//! session 来源:首行 `type=session` 的 id;缺失 → 文件名(去 .jsonl 后缀)。

use crate::usage::{ParseStats, UsageError, UsageEvent, UsageProvider, UsageScan};
use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

fn kimi_sessions_root(home: &Path) -> PathBuf {
    home.join(".kimi-code").join("agents")
}

fn parse_file(path: &Path, events: &mut Vec<UsageEvent>, stats: &mut ParseStats) -> std::io::Result<()> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut current_model: Option<String> = None;
    let mut current_session: Option<String> = None;
    let mut seen_in_file: HashSet<(String, String)> = HashSet::new();
    let fallback_session = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => {
                stats.lines_skipped += 1;
                continue;
            }
        };
        if line.trim().is_empty() {
            continue;
        }
        stats.lines_total += 1;

        let entry: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => {
                stats.lines_skipped += 1;
                continue;
            }
        };

        match entry.get("type").and_then(|v| v.as_str()).unwrap_or("") {
            "session" => {
                if let Some(id) = entry.get("id").and_then(|v| v.as_str()) {
                    current_session = Some(id.to_string());
                }
                stats.lines_skipped += 1;
            }
            "model_change" => {
                if let Some(model) = entry.get("model").and_then(|v| v.as_str()) {
                    if !model.is_empty() {
                        current_model = Some(model.to_string());
                    }
                }
                stats.lines_skipped += 1;
            }
            "assistant" => {
                let usage = match entry.get("usage") {
                    Some(u) if !u.is_null() => u,
                    _ => {
                        stats.lines_skipped += 1;
                        continue;
                    }
                };
                let model = entry
                    .get("model")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .or_else(|| current_model.clone())
                    .unwrap_or_else(|| "unknown".to_string());
                let session_id = current_session.clone().unwrap_or_else(|| fallback_session.clone());
                let event_id = entry
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if event_id.is_empty() {
                    stats.lines_skipped += 1;
                    continue;
                }
                let key = (session_id.clone(), event_id.clone());
                if !seen_in_file.insert(key) {
                    continue;
                }
                let ts = entry
                    .get("time")
                    .and_then(|v| v.as_i64())
                    .map(|ms| millis_to_rfc3339(ms))
                    .unwrap_or_default();
                let input = usage
                    .get("inputOther")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let output = usage.get("output").and_then(|v| v.as_u64()).unwrap_or(0);
                let cache_read = usage
                    .get("inputCacheRead")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let cache_write = usage
                    .get("inputCacheCreation")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);

                events.push(UsageEvent {
                    ts,
                    session_id,
                    event_id,
                    model,
                    input_tokens: input,
                    cache_read_tokens: cache_read,
                    cache_creation_tokens: cache_write,
                    output_tokens: output,
                });
                stats.lines_matched += 1;
            }
            _ => {
                stats.lines_skipped += 1;
            }
        }
    }
    Ok(())
}

fn walk_jsonl(root: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.is_file() && p.extension().and_then(|s| s.to_str()) == Some("jsonl") {
                out.push(p);
            }
        }
    }
    let mut out = Vec::new();
    walk(root, &mut out);
    out
}

fn millis_to_rfc3339(ms: i64) -> String {
    if ms <= 0 {
        return String::new();
    }
    let secs = ms.div_euclid(1000);
    match time::OffsetDateTime::from_unix_timestamp(secs) {
        Ok(t) => format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
            t.year(),
            u8::from(t.month()),
            t.day(),
            t.hour(),
            t.minute(),
            t.second(),
        ),
        Err(_) => String::new(),
    }
}

pub struct KimiUsageProvider;

impl UsageProvider for KimiUsageProvider {
    fn agent_id(&self) -> &'static str {
        "kimi"
    }

    fn available(&self, home: &Path) -> bool {
        kimi_sessions_root(home).exists()
    }

    fn scan(&self, home: &Path) -> Result<UsageScan, UsageError> {
        let agent_id = "kimi";
        let root = kimi_sessions_root(home);
        if !root.exists() {
            return Ok(UsageScan {
                agent_id: agent_id.into(),
                ..Default::default()
            });
        }
        let mut scan = UsageScan {
            agent_id: agent_id.into(),
            ..Default::default()
        };
        for jsonl in walk_jsonl(&root) {
            scan.stats.files_scanned += 1;
            if parse_file(&jsonl, &mut scan.events, &mut scan.stats).is_err() {
                scan.stats.files_skipped += 1;
            }
        }
        Ok(scan)
    }
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
            let dir = std::env::temp_dir().join(format!("clawbox-kimi-{}-{}", std::process::id(), n));
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

    fn fixture_path(name: &str) -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/usage/fixtures/kimi")
            .join(name)
    }

    fn run_fixture(name: &str) -> UsageScan {
        let tmp = LocalHome::new();
        let dest = tmp.path().join(".kimi-code/agents/wd-tmp/agents/main");
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::copy(fixture_path(name), dest.join(name)).unwrap();
        KimiUsageProvider.scan(tmp.path()).unwrap()
    }

    #[test]
    fn basic_fixture_parses_three_assistants() {
        let scan = run_fixture("basic.jsonl");
        assert_eq!(scan.agent_id, "kimi");
        // session 行 + model_change 行 + 1 user 行 + 3 assistant 行 = 7
        assert_eq!(scan.events.len(), 3, "events: {:?}", scan.events);
        assert_eq!(scan.stats.lines_matched, 3);
        let first = scan.events.iter().find(|e| e.model == "kimi-k2.5").unwrap();
        assert_eq!(first.input_tokens, 100);
        assert_eq!(first.output_tokens, 50);
        let second = scan
            .events
            .iter()
            .find(|e| e.model == "claude-sonnet-4-5")
            .unwrap();
        assert_eq!(second.input_tokens, 1500);
        assert_eq!(second.cache_read_tokens, 500);
        assert_eq!(second.cache_creation_tokens, 100);
        // 第三个 assistant 没有 model 字段,应 fallback 到 current_model
        // (kimi 协议:model_change 之后所有 assistant 都用新 model,直到下次 model_change)
        let third = scan.events.iter().find(|e| e.output_tokens == 80).unwrap();
        assert_eq!(third.model, "claude-sonnet-4-5");
    }

    #[test]
    fn missing_dir_returns_empty() {
        let tmp = LocalHome::new();
        let scan = KimiUsageProvider.scan(tmp.path()).unwrap();
        assert_eq!(scan.events.len(), 0);
    }

    #[test]
    fn available_reflects_root() {
        let tmp = LocalHome::new();
        assert!(!KimiUsageProvider.available(tmp.path()));
        std::fs::create_dir_all(tmp.path().join(".kimi-code/agents")).unwrap();
        assert!(KimiUsageProvider.available(tmp.path()));
    }
}
