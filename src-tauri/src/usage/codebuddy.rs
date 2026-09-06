//! CodeBuddy CLI adapter:
//! 解析 `~/.codebuddy/projects/<cwd>/.jsonl`(tokscale 列出的路径)。
//!
//! 字段口径:
//! - `usage.input_tokens` → input_tokens
//! - `usage.output_tokens` → output_tokens
//! - `usage.cache_read_input_tokens` → cache_read_tokens

use crate::usage::{ParseStats, UsageError, UsageEvent, UsageProvider, UsageScan};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

fn codebuddy_projects_root(home: &Path) -> PathBuf {
    home.join(".codebuddy").join("projects")
}

fn parse_file(path: &Path, events: &mut Vec<UsageEvent>, stats: &mut ParseStats) -> std::io::Result<()> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut current_model: Option<String> = None;
    let mut current_session: Option<String> = None;
    // 文件级 dedup counter:同一 (session, ts, model) 内多次 token 事件
    // 编号递增,避免 store 的 seen_events 三元组把同秒多 turn 视为重复。
    let mut seq_counter: HashMap<String, u64> = HashMap::new();
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
                if let Some(sid) = entry.get("sessionId").and_then(|v| v.as_str()) {
                    current_session = Some(sid.to_string());
                }
                stats.lines_skipped += 1;
            }
            "model_change" => {
                if let Some(m) = entry.get("model").and_then(|v| v.as_str()) {
                    if !m.is_empty() {
                        current_model = Some(m.to_string());
                    }
                }
                stats.lines_skipped += 1;
            }
            "message" => {
                let role = entry.get("role").and_then(|v| v.as_str()).unwrap_or("");
                if role != "assistant" {
                    stats.lines_skipped += 1;
                    continue;
                }
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
                let ts = entry
                    .get("timestamp")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let input = usage.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                let output = usage.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                let cache_read = usage
                    .get("cache_read_input_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                // event_id = session_id + ts + model + 文件级 seq,防撞。
                let dedup_key = format!("{}|{}|{}", session_id, ts, model);
                let seq = seq_counter.entry(dedup_key.clone()).or_insert(0u64);
                *seq += 1;
                let event_id = format!("{}#{}-codebuddy", dedup_key, seq);

                events.push(UsageEvent {
                    ts,
                    session_id,
                    event_id,
                    model,
                    input_tokens: input,
                    cache_read_tokens: cache_read,
                    cache_creation_tokens: 0,
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

pub struct CodeBuddyUsageProvider;

impl UsageProvider for CodeBuddyUsageProvider {
    fn agent_id(&self) -> &'static str {
        "codebuddy"
    }

    fn available(&self, home: &Path) -> bool {
        codebuddy_projects_root(home).exists()
    }

    fn scan(&self, home: &Path) -> Result<UsageScan, UsageError> {
        let agent_id = "codebuddy";
        let root = codebuddy_projects_root(home);
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
            let dir =
                std::env::temp_dir().join(format!("clawbox-codebuddy-{}-{}", std::process::id(), n));
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
            .join("src/usage/fixtures/codebuddy")
            .join(name)
    }

    fn run_fixture(name: &str) -> UsageScan {
        let tmp = LocalHome::new();
        let dest = tmp.path().join(".codebuddy/projects/tmp");
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::copy(fixture_path(name), dest.join(name)).unwrap();
        CodeBuddyUsageProvider.scan(tmp.path()).unwrap()
    }

    #[test]
    fn basic_fixture_parses_three_assistants() {
        let scan = run_fixture("basic.jsonl");
        assert_eq!(scan.agent_id, "codebuddy");
        assert_eq!(scan.events.len(), 3);
        let glm = scan.events.iter().filter(|e| e.model == "glm-4.7").count();
        assert_eq!(glm, 2);
        let sonnet = scan
            .events
            .iter()
            .filter(|e| e.model == "claude-sonnet-4-5")
            .count();
        assert_eq!(sonnet, 1);
        assert_eq!(scan.events[0].input_tokens, 1200);
        assert_eq!(scan.events[0].output_tokens, 300);
        assert_eq!(scan.events[1].cache_read_tokens, 200);
    }
}
