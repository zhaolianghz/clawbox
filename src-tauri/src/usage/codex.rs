//! Codex adapter:解析 `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl`。
//!
//! **差值口径**: Codex 的 `event_msg.type == "token_count"` 给的是
//! 累积 `total_token_usage`,而非单次 turn 增量。维护文件内 last_total
//! 快照,差值即本次 turn 增量;首个事件 last_total = 0,差值 = total。
//!
//! 模型:从 `turn_context.payload.model` 取,缺失则归 "unknown"。
//! 跨 turn 共享同一个 turn_context,后续 token_count 都用该 model。
//!
//! `total_tokens` 字段是 `input + cached + output + reasoning` 之和(由
//! Codex 维护),不参与差值,避免四舍五入抖动 — 我们用 `input_tokens -
//! cached_input_tokens` 作 input,`cached_input_tokens` 作 cache_read,
//! `output_tokens + reasoning_output_tokens` 作 output。

use crate::usage::{ParseStats, UsageError, UsageEvent, UsageProvider, UsageScan};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

/// 单个 token_count 事件的 total 快照(用于差值)。
#[derive(Default, Clone, Copy, Debug)]
struct CodexTotalSnapshot {
    input: u64,
    cached: u64,
    output: u64,
    reasoning: u64,
}

impl CodexTotalSnapshot {
    /// 从 `total_token_usage` 字段构造,容错:字段缺失视为 0。
    fn from_payload(value: &serde_json::Value) -> Self {
        let get = |k: &str| value.get(k).and_then(|v| v.as_u64()).unwrap_or(0);
        Self {
            input: get("input_tokens"),
            cached: get("cached_input_tokens"),
            output: get("output_tokens"),
            reasoning: get("reasoning_output_tokens"),
        }
    }

    /// 与前一个快照的差值,各项下限为 0(防止 last_token_usage 偶发比
    /// total 小导致负值)。
    fn delta_from(&self, prev: &Self) -> Self {
        Self {
            input: self.input.saturating_sub(prev.input),
            cached: self.cached.saturating_sub(prev.cached),
            output: self.output.saturating_sub(prev.output),
            reasoning: self.reasoning.saturating_sub(prev.reasoning),
        }
    }
}

/// 解析单个 rollout JSONL 文件,append 到 events。逐行容错。
fn parse_file(
    path: &Path,
    events: &mut Vec<UsageEvent>,
    stats: &mut ParseStats,
) -> std::io::Result<()> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut last_total: Option<CodexTotalSnapshot> = None;
    let mut current_model: Option<String> = None;
    let mut current_session: Option<String> = None;
    let mut turn_index: u64 = 0;

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
            "session_meta" => {
                if let Some(payload) = entry.get("payload") {
                    if let Some(id) = payload.get("id").and_then(|v| v.as_str()) {
                        current_session = Some(id.to_string());
                    }
                }
                // session_meta 行不计入 matched(它不是 token_count)
                stats.lines_skipped += 1;
            }
            "turn_context" => {
                if let Some(payload) = entry.get("payload") {
                    if let Some(model) = payload.get("model").and_then(|v| v.as_str()) {
                        if !model.is_empty() {
                            current_model = Some(model.to_string());
                        }
                    }
                }
                stats.lines_skipped += 1;
            }
            "event_msg" => {
                let payload = entry.get("payload");
                let payload_type = payload
                    .and_then(|p| p.get("type"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if payload_type != "token_count" {
                    stats.lines_skipped += 1;
                    continue;
                }
                let info = payload.and_then(|p| p.get("info"));
                let total_value = match info.and_then(|i| i.get("total_token_usage")) {
                    Some(t) => t,
                    None => {
                        stats.lines_skipped += 1;
                        continue;
                    }
                };
                let total = CodexTotalSnapshot::from_payload(total_value);

                // 差值计算
                let delta = match last_total {
                    None => total, // 首事件 = 全部为本次增量
                    Some(prev) => total.delta_from(&prev),
                };
                last_total = Some(total);

                let model = current_model
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string());
                let session_id = current_session
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string());
                let ts = entry
                    .get("timestamp")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                turn_index += 1;
                let event_id = format!("{}#{}", session_id, turn_index);

                // 输入拆: input - cached(因为 input_tokens 已经含 cached 部分)
                let input_only = delta.input.saturating_sub(delta.cached);

                events.push(UsageEvent {
                    ts,
                    session_id,
                    event_id,
                    model,
                    input_tokens: input_only,
                    cache_read_tokens: delta.cached,
                    cache_creation_tokens: 0, // Codex 不单独给
                    output_tokens: delta.output + delta.reasoning,
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

pub struct CodexUsageProvider;

impl UsageProvider for CodexUsageProvider {
    fn agent_id(&self) -> &'static str {
        "codex"
    }

    fn available(&self, home: &Path) -> bool {
        home.join(".codex").join("sessions").exists()
    }

    fn scan(&self, home: &Path) -> Result<UsageScan, UsageError> {
        let agent_id = "codex";
        let sessions = home.join(".codex").join("sessions");
        if !sessions.exists() {
            return Ok(UsageScan {
                agent_id: agent_id.into(),
                events: vec![],
                stats: ParseStats::default(),
            });
        }

        let mut scan = UsageScan {
            agent_id: agent_id.into(),
            events: vec![],
            stats: ParseStats::default(),
        };

        // rollout 文件可能散在 YYYY/MM/DD/ 子目录下,用 walkdir 递归
        for entry in walkdir_resilient(&sessions) {
            let p = entry;
            if !p.is_file() {
                continue;
            }
            let fname = match p.file_name().and_then(|s| s.to_str()) {
                Some(s) => s,
                None => continue,
            };
            if !fname.starts_with("rollout-") || !fname.ends_with(".jsonl") {
                continue;
            }
            scan.stats.files_scanned += 1;
            if parse_file(&p, &mut scan.events, &mut scan.stats).is_err() {
                scan.stats.files_skipped += 1;
            }
        }

        Ok(scan)
    }
}

/// 手写 walkdir 替代 — 避免拉 crate 依赖。深度优先递归所有子目录。
fn walkdir_resilient(root: &Path) -> Vec<std::path::PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.is_file() {
                out.push(p);
            }
        }
    }
    let mut out = Vec::new();
    walk(root, &mut out);
    out
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
                "clawbox-usage-codex-test-{}-{}",
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

    fn fixture_path(name: &str) -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/usage/fixtures/codex")
            .join(name)
    }

    fn run_fixture(name: &str) -> UsageScan {
        let tmp = LocalHome::new();
        // 模拟 YYYY/MM/DD 嵌套结构
        let dest = tmp.path().join(".codex/sessions/2026/06/05");
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::copy(fixture_path(name), dest.join("rollout-fixture.jsonl")).unwrap();
        CodexUsageProvider.scan(tmp.path()).unwrap()
    }

    #[test]
    fn fixture_files_are_valid_jsonl() {
        for name in [
            "initial_token_count.jsonl",
            "multiple_turns.jsonl",
            "missing_model.jsonl",
        ] {
            let content = std::fs::read_to_string(fixture_path(name)).unwrap();
            for line in content.lines() {
                if line.trim().is_empty() {
                    continue;
                }
                serde_json::from_str::<serde_json::Value>(line)
                    .unwrap_or_else(|e| panic!("{} invalid JSON: {}\n{}", name, e, line));
            }
        }
    }

    #[test]
    fn initial_token_count_produces_one_event() {
        let scan = run_fixture("initial_token_count.jsonl");
        eprintln!("DEBUG initial: events={} stats={:?}", scan.events.len(), scan.stats);
        assert_eq!(scan.agent_id, "codex");
        assert_eq!(scan.events.len(), 1);
        // total = input:17993 cached:2432 output:314 reasoning:27
        // input 拆 = 17993 - 2432 = 15561
        assert_eq!(scan.events[0].input_tokens, 15561);
        assert_eq!(scan.events[0].cache_read_tokens, 2432);
        assert_eq!(scan.events[0].cache_creation_tokens, 0);
        // output 含 reasoning
        assert_eq!(scan.events[0].output_tokens, 314 + 27);
    }

    #[test]
    fn multiple_turns_uses_diff_accumulation() {
        let scan = run_fixture("multiple_turns.jsonl");
        assert_eq!(scan.events.len(), 3, "should have 3 turn events");
        // 第二个事件 delta: input 23983 cached 17792 output 318 reasoning 0
        // 拆 input = 23983 - 17792 = 6191
        assert_eq!(scan.events[1].input_tokens, 23983 - 17792);
        assert_eq!(scan.events[1].cache_read_tokens, 17792);
        assert_eq!(scan.events[1].output_tokens, 318);
        // 第三个: input 32309 cached 2432 output 1397 reasoning 45
        // 拆 input = 32309 - 2432 = 29877
        assert_eq!(scan.events[2].input_tokens, 32309 - 2432);
        assert_eq!(scan.events[2].cache_read_tokens, 2432);
        assert_eq!(scan.events[2].output_tokens, 1397 + 45);
    }

    #[test]
    fn missing_model_falls_back_to_unknown() {
        let scan = run_fixture("missing_model.jsonl");
        assert_eq!(scan.events.len(), 3);
        for e in &scan.events {
            assert_eq!(e.model, "unknown", "expected model=unknown, got {}", e.model);
        }
    }

    #[test]
    fn no_token_count_events_returns_empty() {
        // 手写一个没有 token_count 的 rollout
        let tmp = LocalHome::new();
        let dest = tmp.path().join(".codex/sessions/2026/06/05");
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::write(
            dest.join("rollout-empty.jsonl"),
            "{\"type\":\"session_meta\",\"payload\":{\"id\":\"s1\"}}\n{\"type\":\"turn_context\",\"payload\":{\"model\":\"gpt-5\"}}\n{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\"}}\n",
        ).unwrap();
        let scan = CodexUsageProvider.scan(tmp.path()).unwrap();
        assert_eq!(scan.events.len(), 0);
        assert!(scan.stats.lines_total > 0);
        assert_eq!(scan.stats.lines_matched, 0);
    }

    #[test]
    fn missing_sessions_dir_returns_empty_no_error() {
        let tmp = LocalHome::new();
        let scan = CodexUsageProvider.scan(tmp.path()).unwrap();
        assert_eq!(scan.events.len(), 0);
        assert_eq!(scan.stats.lines_total, 0);
    }

    #[test]
    fn available_reflects_sessions_dir() {
        let tmp = LocalHome::new();
        assert!(!CodexUsageProvider.available(tmp.path()));
        std::fs::create_dir_all(tmp.path().join(".codex/sessions")).unwrap();
        assert!(CodexUsageProvider.available(tmp.path()));
    }
}
