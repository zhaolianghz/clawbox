//! Gemini CLI adapter:
//! 解析 `~/.gemini/tmp/<hash>/logs.json`(默认)或 telemetry 开启后
//! `~/.gemini/tmp/<hash>/logs/telemetry.log`(JSONL)。
//!
//! token 在 `event.name == "gemini_cli.token.usage"` 事件里,字段从
//! `attributes` 里读:
//! - `model` (string) → model id
//! - `input_tokens` / `output_tokens` / `cached_tokens` / `thought_tokens`
//! - 顶层 `session.id` → session_id
//!
//! 注意:默认 `~/.gemini/settings.json` 未启用 telemetry,本 adapter 默
//! 认 available=false。要启用 telemetry 看官方 docs。

use crate::usage::{ParseStats, UsageError, UsageEvent, UsageProvider, UsageScan};
use std::path::{Path, PathBuf};

const TOKEN_EVENT: &str = "gemini_cli.token.usage";

fn find_gemini_logs(home: &Path) -> Option<PathBuf> {
    let tmp = home.join(".gemini").join("tmp");
    if !tmp.exists() {
        return None;
    }
    // 优先 logs/telemetry.log(开启 telemetry 后才有),其次 logs.json
    let entries = std::fs::read_dir(&tmp).ok()?;
    let mut candidates = Vec::new();
    for entry in entries.flatten() {
        let p = entry.path();
        if !p.is_dir() {
            continue;
        }
        let telemetry = p.join("logs").join("telemetry.log");
        if telemetry.is_file() {
            candidates.push(telemetry);
            continue;
        }
        let logs_json = p.join("logs.json");
        if logs_json.is_file() {
            candidates.push(logs_json);
        }
    }
    candidates.into_iter().next()
}

fn parse_file(path: &Path, events: &mut Vec<UsageEvent>, stats: &mut ParseStats) -> std::io::Result<()> {
    let raw = std::fs::read_to_string(path)?;
    // logs.json 是 JSON 数组,logs/telemetry.log 是 JSONL。自动判断。
    let trimmed = raw.trim_start();
    if trimmed.starts_with('[') {
        // JSON 数组
        let arr: Vec<serde_json::Value> = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(e) => {
                // 整文件解析失败:文件算 scanned 但内容全跳过
                stats.lines_total += 1;
                stats.lines_skipped += 1;
                let _ = e;
                return Ok(());
            }
        };
        for entry in arr {
            handle_event(&entry, events, stats);
        }
    } else {
        for line in raw.lines() {
            if line.trim().is_empty() {
                continue;
            }
            stats.lines_total += 1;
            let v: serde_json::Value = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(_) => {
                    stats.lines_skipped += 1;
                    continue;
                }
            };
            handle_event(&v, events, stats);
        }
    }
    Ok(())
}

/// 处理单条 Gemini telemetry event。
///
/// 容错契约(跟其它 adapter 一致,保证 matched_ratio 真实):
/// - 入口先 `lines_total += 1`,任何过滤路径都先记账;
/// - 非 token 事件 / 空 event_id / 全 0 token 都计 `lines_skipped += 1`,
///   **不静默丢弃**,否则 UI 看到的 matched_ratio 会虚高。
/// - event_id 必须来自 telemetry 协议的 `event.id`(OTel 规范保证)
///   —— 缺失视为数据异常,跳过;不 fallback 到 timestamp,因为同秒
///   多 turn 会撞 store 的 seen_events 三元组,导致真实 token 漏算。
fn handle_event(entry: &serde_json::Value, events: &mut Vec<UsageEvent>, stats: &mut ParseStats) {
    stats.lines_total += 1;
    let event_name = entry
        .get("event.name")
        .or_else(|| entry.get("eventName"))
        .or_else(|| entry.get("event_name"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if event_name != TOKEN_EVENT {
        stats.lines_skipped += 1;
        return;
    }
    let attrs = entry.get("attributes").unwrap_or(entry);
    let input = attrs
        .get("input_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let output = attrs
        .get("output_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let cached = attrs
        .get("cached_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let thought = attrs
        .get("thought_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    if input == 0 && output == 0 && cached == 0 && thought == 0 {
        stats.lines_skipped += 1;
        return;
    }
    let model = attrs
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    let session_id = entry
        .get("session.id")
        .or_else(|| entry.get("sessionId"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let ts = entry
        .get("timestamp")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    // event.id 是 OTel 协议规范字段,缺失视为格式异常跳过,不 fallback。
    let event_id = entry
        .get("event.id")
        .or_else(|| entry.get("eventId"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let event_id = match event_id {
        Some(s) if !s.is_empty() => s,
        _ => {
            stats.lines_skipped += 1;
            return;
        }
    };

    events.push(UsageEvent {
        ts,
        session_id,
        event_id,
        model,
        input_tokens: input,
        cache_read_tokens: cached,
        cache_creation_tokens: 0,
        output_tokens: output + thought,
    });
    stats.lines_matched += 1;
}

pub struct GeminiUsageProvider;

impl UsageProvider for GeminiUsageProvider {
    fn agent_id(&self) -> &'static str {
        "gemini"
    }

    fn available(&self, home: &Path) -> bool {
        // 只有真的存在 telemetry 文件时才认为 available — 默认不开 telemetry
        // 的话,logs.json 是消息日志,无 token 数据,available=false 让 UI 自然跳过。
        if let Some(p) = find_gemini_logs(home) {
            p.to_string_lossy().contains("telemetry")
        } else {
            false
        }
    }

    fn scan(&self, home: &Path) -> Result<UsageScan, UsageError> {
        let agent_id = "gemini";
        let path = match find_gemini_logs(home) {
            Some(p) => p,
            None => {
                return Ok(UsageScan {
                    agent_id: agent_id.into(),
                    ..Default::default()
                });
            }
        };
        let mut stats = ParseStats::default();
        let mut events: Vec<UsageEvent> = Vec::new();
        if let Err(e) = parse_file(&path, &mut events, &mut stats) {
            return Err(UsageError::new(agent_id, "io", format!("read {}: {}", path.display(), e)));
        }
        Ok(UsageScan {
            agent_id: agent_id.into(),
            events,
            stats,
            error_note: None,
        })
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
                std::env::temp_dir().join(format!("clawbox-gemini-{}-{}", std::process::id(), n));
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
            .join("src/usage/fixtures/gemini")
            .join(name)
    }

    fn run_fixture(name: &str) -> UsageScan {
        let tmp = LocalHome::new();
        let dest = tmp.path().join(".gemini/tmp/abc/logs");
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::copy(fixture_path(name), dest.join("telemetry.log")).unwrap();
        GeminiUsageProvider.scan(tmp.path()).unwrap()
    }

    #[test]
    fn basic_fixture_parses_three_token_events() {
        let scan = run_fixture("basic.jsonl");
        assert_eq!(scan.agent_id, "gemini");
        assert_eq!(scan.events.len(), 3, "events: {:?}", scan.events);
        // thought_tokens 合并进 output
        let e1 = scan
            .events
            .iter()
            .find(|e| e.model == "gemini-2.5-flash" && e.input_tokens == 1500)
            .unwrap();
        assert_eq!(e1.output_tokens, 450); // 400 + 50
        assert_eq!(e1.cache_read_tokens, 0);
        let e3 = scan
            .events
            .iter()
            .find(|e| e.model == "gemini-3.7-flash")
            .unwrap();
        assert_eq!(e3.output_tokens, 600); // 500 + 100
    }

    #[test]
    fn missing_telemetry_returns_empty_and_not_available() {
        let tmp = LocalHome::new();
        // 只有 logs.json,telemetry.log 不存在 → available=false
        let dest = tmp.path().join(".gemini/tmp/abc");
        std::fs::create_dir_all(dest.join("logs")).unwrap();
        std::fs::write(dest.join("logs.json"), "[]").unwrap();
        let scan = GeminiUsageProvider.scan(tmp.path()).unwrap();
        assert_eq!(scan.events.len(), 0);
        assert!(!GeminiUsageProvider.available(tmp.path()));
    }
}
