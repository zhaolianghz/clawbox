//! Pi adapter:解析 `~/.pi/agent/sessions/<cwd-encoded>/*.jsonl`。
//!
//! Pi(badlogic/pi-mono)是 OpenCode 团队的另一个 coding CLI,日志格式是 JSONL
//! 每行一个事件,顶层 `type` 字段驱动:
//!
//! - `session`           — 标记一个 session 开始(可有可无,不强依赖)
//! - `model_change`      — 当前 model 切换(`modelId` 字段)
//! - `thinking_level_change` — 无关,跳过
//! - `message`           — 一次实际对话;assistant 角色的 message 自带
//!                         `usage: { input, output, cacheRead, cacheWrite,
//!                         reasoning, totalTokens, cost }` + `model`
//!
//! 形状提取策略:
//! - 文件内维护 `current_model`(`model_change` 触发更新,缺失 → "unknown")
//! - 仅取 `message.role == "assistant"` 且 `usage` 非 null 的行
//! - event_id 用 message 的 id 字段(全局唯一)
//! - session_id 从 `session` 行的 id 字段,缺失 → 用 filename(去掉扩展名前缀)
//!
//! 路径:每个 cwd 一个子目录,目录名是 cwd 的 `path.sep` → `-` 编码。
//! 我们递归所有 `*.jsonl`,不关心 cwd 维度(agent_id + model 足够区分)。

use crate::usage::{ParseStats, UsageError, UsageEvent, UsageProvider, UsageScan};
use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

fn parse_file(path: &Path, events: &mut Vec<UsageEvent>, stats: &mut ParseStats) -> std::io::Result<()> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut current_model: Option<String> = None;
    let mut current_session: Option<String> = None;
    let mut seen_in_file: HashSet<(String, String)> = HashSet::new();
    // 文件名当 session_id 兜底(去 .jsonl 后缀 + 去除时间戳前缀)
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
                // session 行不计入 matched(不是 token 事件)
                stats.lines_skipped += 1;
            }
            "model_change" => {
                if let Some(model_id) = entry.get("modelId").and_then(|v| v.as_str()) {
                    if !model_id.is_empty() {
                        current_model = Some(model_id.to_string());
                    }
                }
                stats.lines_skipped += 1;
            }
            "message" => {
                let msg = entry.get("message");
                let role = msg
                    .and_then(|m| m.get("role"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if role != "assistant" {
                    stats.lines_skipped += 1;
                    continue;
                }
                let usage = match msg.and_then(|m| m.get("usage")) {
                    Some(u) if !u.is_null() => u,
                    _ => {
                        stats.lines_skipped += 1;
                        continue;
                    }
                };
                // model:优先用 message 自身的 model 字段,缺失用 current_model
                let model = msg
                    .and_then(|m| m.get("model"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .or_else(|| current_model.clone())
                    .unwrap_or_else(|| "unknown".to_string());
                let session_id = current_session
                    .clone()
                    .unwrap_or_else(|| fallback_session.clone());
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
                    .get("timestamp")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let input_tokens = usage.get("input").and_then(|v| v.as_u64()).unwrap_or(0);
                let output_tokens = usage.get("output").and_then(|v| v.as_u64()).unwrap_or(0);
                let cache_read = usage.get("cacheRead").and_then(|v| v.as_u64()).unwrap_or(0);
                let cache_write = usage.get("cacheWrite").and_then(|v| v.as_u64()).unwrap_or(0);
                let reasoning = usage.get("reasoning").and_then(|v| v.as_u64()).unwrap_or(0);

                events.push(UsageEvent {
                    ts,
                    session_id,
                    event_id,
                    model,
                    input_tokens,
                    cache_read_tokens: cache_read,
                    cache_creation_tokens: cache_write,
                    // pi 的 output 已是最终对外输出;reasoning 是 thinking token,
                    // 对齐 codex 口径:合并到 output(也跟 hermes 一致)
                    output_tokens: output_tokens + reasoning,
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

/// 手写 walkdir 替代(codex 那边的实现我们直接复制,简单稳定)。
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
            } else if p.is_file()
                && p.extension().and_then(|s| s.to_str()) == Some("jsonl")
            {
                out.push(p);
            }
        }
    }
    let mut out = Vec::new();
    walk(root, &mut out);
    out
}

pub struct PiUsageProvider;

impl UsageProvider for PiUsageProvider {
    fn agent_id(&self) -> &'static str {
        "pi"
    }

    fn available(&self, home: &Path) -> bool {
        home.join(".pi").join("agent").join("sessions").exists()
    }

    fn scan(&self, home: &Path) -> Result<UsageScan, UsageError> {
        let agent_id = "pi";
        let sessions = home.join(".pi").join("agent").join("sessions");
        if !sessions.exists() {
            return Ok(UsageScan {
                agent_id: agent_id.into(),
                ..Default::default()
            });
        }

        let mut scan = UsageScan {
            agent_id: agent_id.into(),
            ..Default::default()
        };

        for jsonl in walk_jsonl(&sessions) {
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
            let dir = std::env::temp_dir().join(format!("clawbox-pi-{}-{}", std::process::id(), n));
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
            .join("src/usage/fixtures/pi")
            .join(name)
    }

    fn run_fixture(name: &str) -> UsageScan {
        let tmp = LocalHome::new();
        let dest = tmp.path().join(".pi/agent/sessions/--tmp--");
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::copy(fixture_path(name), dest.join(name)).unwrap();
        PiUsageProvider.scan(tmp.path()).unwrap()
    }

    #[test]
    fn fixture_files_are_valid_jsonl() {
        for name in ["basic.jsonl", "missing_model.jsonl"] {
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
    fn basic_fixture_picks_up_two_models() {
        let scan = run_fixture("basic.jsonl");
        assert_eq!(scan.agent_id, "pi");
        assert_eq!(scan.events.len(), 2, "events: {:?}", scan.events);
        // 第一条:gpt-5.6-terra,input 37175,output 540+0(reasoning=0)
        let first = scan.events.iter().find(|e| e.model == "gpt-5.6-terra").unwrap();
        assert_eq!(first.input_tokens, 37175);
        assert_eq!(first.output_tokens, 540);
        // 第二条:claude-sonnet-4-5,input 1000,output 200+50(reasoning=50)
        let second = scan
            .events
            .iter()
            .find(|e| e.model == "claude-sonnet-4-5")
            .unwrap();
        assert_eq!(second.input_tokens, 1000);
        assert_eq!(second.output_tokens, 250);
        assert_eq!(second.cache_read_tokens, 500);
        assert_eq!(second.cache_creation_tokens, 100);
        // 两条用同一个 session_id(来自 session 行)
        assert!(scan.events.iter().all(|e| e.session_id == "01a0568f-a546-777a-ba82-9555947e008d"));
    }

    #[test]
    fn missing_model_falls_back_to_unknown() {
        let scan = run_fixture("missing_model.jsonl");
        // 无 model_change 也无 message.model → fallback "unknown"
        assert_eq!(scan.events.len(), 1);
        assert_eq!(scan.events[0].model, "unknown");
    }

    #[test]
    fn missing_sessions_dir_returns_empty_no_error() {
        let tmp = LocalHome::new();
        let scan = PiUsageProvider.scan(tmp.path()).unwrap();
        assert_eq!(scan.events.len(), 0);
        assert_eq!(scan.stats.lines_total, 0);
    }

    #[test]
    fn available_reflects_sessions_dir() {
        let tmp = LocalHome::new();
        assert!(!PiUsageProvider.available(tmp.path()));
        std::fs::create_dir_all(tmp.path().join(".pi/agent/sessions")).unwrap();
        assert!(PiUsageProvider.available(tmp.path()));
    }
}
