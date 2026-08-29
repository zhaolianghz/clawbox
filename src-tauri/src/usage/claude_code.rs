//! Claude Code adapter:解析 `~/.claude/projects/**/*.jsonl`。
//!
//! 形状提取(避免紧耦合 schema):每行 JSON,只看 `type == "assistant"`
//! 且 `message.role == "assistant"` 且 `message.usage` 非空。其它行一律
//! 跳过。模型字段从 `message.model` 取,空则归 "unknown"。
//!
//! 去重: 内存 HashSet 按 `(session_id, message.id)` 去重 — 同一 message
//! 在 sidechain 多次出现只算一次(借鉴 ccusage 成熟口径)。

use crate::usage::{ParseStats, UsageError, UsageEvent, UsageProvider, UsageScan};
use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

/// 解析单个 JSONL 文件,append 到 events。逐行容错。
///
/// 返回 `std::io::Result` 是「打不开文件」级别的硬错误(整文件不可访问),
/// 行级解析失败一律走 `stats.lines_skipped`,不抛。
fn parse_file(path: &Path, events: &mut Vec<UsageEvent>, stats: &mut ParseStats) -> std::io::Result<()> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut seen_in_file: HashSet<(String, String)> = HashSet::new();

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

        let t = entry.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if t != "assistant" {
            stats.lines_skipped += 1;
            continue;
        }
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

        let session_id = entry
            .get("sessionId")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let msg_id = msg
            .and_then(|m| m.get("id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if session_id.is_empty() || msg_id.is_empty() {
            stats.lines_skipped += 1;
            continue;
        }

        let key = (session_id.clone(), msg_id.clone());
        if !seen_in_file.insert(key) {
            // 文件内重复 → 跳过但既不算 matched 也不算 skipped(去重是正常的)
            continue;
        }

        let model = msg
            .and_then(|m| m.get("model"))
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        let ts = entry
            .get("timestamp")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let input_tokens = usage.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
        let cache_read = usage.get("cache_read_input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
        let cache_creation = usage
            .get("cache_creation_input_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let output_tokens = usage.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0);

        events.push(UsageEvent {
            ts,
            session_id,
            event_id: msg_id,
            model,
            input_tokens,
            cache_read_tokens: cache_read,
            cache_creation_tokens: cache_creation,
            output_tokens,
        });
        stats.lines_matched += 1;
    }
    Ok(())
}

pub struct ClaudeCodeUsageProvider;

impl UsageProvider for ClaudeCodeUsageProvider {
    fn agent_id(&self) -> &'static str {
        "claude-code"
    }

    fn available(&self, home: &Path) -> bool {
        home.join(".claude").join("projects").exists()
    }

    fn scan(&self, home: &Path) -> Result<UsageScan, UsageError> {
        let agent_id = "claude-code";
        let projects = home.join(".claude").join("projects");
        if !projects.exists() {
            return Ok(UsageScan {
                agent_id: agent_id.into(),
                ..Default::default()
            });
        }

        let mut scan = UsageScan {
            agent_id: agent_id.into(),
            ..Default::default()
        };

        let entries = match std::fs::read_dir(&projects) {
            Ok(e) => e,
            Err(e) => {
                return Err(UsageError::new(
                    agent_id,
                    "io",
                    format!("read_dir {}: {}", projects.display(), e),
                ));
            }
        };

        for entry in entries.flatten() {
            let sub = entry.path();
            if !sub.is_dir() {
                continue;
            }
            let jsonl_files = match std::fs::read_dir(&sub) {
                Ok(f) => f,
                Err(_) => {
                    scan.stats.files_skipped += 1;
                    continue;
                }
            };
            for f in jsonl_files.flatten() {
                let p = f.path();
                if p.extension().and_then(|s| s.to_str()) != Some("jsonl") {
                    continue;
                }
                scan.stats.files_scanned += 1;
                if parse_file(&p, &mut scan.events, &mut scan.stats).is_err() {
                    scan.stats.files_skipped += 1;
                }
            }
        }

        Ok(scan)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// 用例隔离的临时 home,自创建自清理,绝不碰真实用户文件。
    /// 与 sync::test_util::TempHome 保持同样语义但复制实现 — sync 的版本
    /// 是 pub(crate) 不可跨模块复用。
    struct LocalHome(std::path::PathBuf);
    impl LocalHome {
        fn new() -> Self {
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let n = COUNTER.fetch_add(1, Ordering::SeqCst);
            let dir = std::env::temp_dir().join(format!(
                "clawbox-usage-test-{}-{}",
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
            .join("src/usage/fixtures/claude-code")
            .join(name)
    }

    /// 把 fixture 拷贝到 home 下的 `.claude/projects/<proj>/<fixture>` 并扫描。
    fn run_fixture(name: &str) -> UsageScan {
        let tmp = LocalHome::new();
        let dest = tmp.path().join(".claude/projects/redacted");
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::copy(fixture_path(name), dest.join(name)).unwrap();
        ClaudeCodeUsageProvider.scan(tmp.path()).unwrap()
    }

    #[test]
    fn fixture_files_are_valid_jsonl() {
        // 健康检查:所有 fixture 行都是合法 JSON
        for name in ["basic.jsonl", "cache_heavy.jsonl", "sidechain_dedup.jsonl"] {
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
    fn basic_fixture_parses_three_events() {
        let scan = run_fixture("basic.jsonl");
        assert_eq!(scan.agent_id, "claude-code");
        assert_eq!(scan.events.len(), 3, "events: {:?}", scan.events);
        assert_eq!(scan.stats.lines_matched, 3);
        assert!(scan.events.iter().any(|e| e.input_tokens > 100));
        assert!(scan.events.iter().any(|e| e.cache_creation_tokens > 1000));
    }

    #[test]
    fn cache_heavy_fixture_picks_up_cache_tokens() {
        let scan = run_fixture("cache_heavy.jsonl");
        assert_eq!(scan.events.len(), 3);
        let max_cr = scan.events.iter().map(|e| e.cache_read_tokens).max().unwrap();
        assert!(max_cr > 100_000, "expected cache_read > 100k, got {}", max_cr);
    }

    #[test]
    fn sidechain_dedup_drops_duplicates() {
        let scan = run_fixture("sidechain_dedup.jsonl");
        let ids: HashSet<_> = scan
            .events
            .iter()
            .map(|e| (e.session_id.clone(), e.event_id.clone()))
            .collect();
        // 去重后必须 unique,数量 < 文件原始行数
        assert_eq!(ids.len(), scan.events.len(), "events not unique");
        assert!(scan.events.len() < scan.stats.lines_total + scan.events.len());
    }

    #[test]
    fn malformed_lines_are_skipped_not_fatal() {
        let tmp = LocalHome::new();
        let dest = tmp.path().join(".claude/projects/proj1");
        std::fs::create_dir_all(&dest).unwrap();
        let path = dest.join("broken.jsonl");
        std::fs::write(
            &path,
            "not json at all\n{\"type\":\"assistant\",\"sessionId\":\"s1\",\"message\":{\"id\":\"m1\",\"role\":\"assistant\",\"model\":\"claude-x\",\"usage\":{\"input_tokens\":10,\"output_tokens\":5}}}\n{\"unterminated\":\n",
        )
        .unwrap();
        let scan = ClaudeCodeUsageProvider.scan(tmp.path()).unwrap();
        assert_eq!(scan.events.len(), 1, "should parse the one good row");
        assert_eq!(scan.events[0].event_id, "m1");
        assert!(scan.stats.lines_skipped >= 2);
    }

    #[test]
    fn missing_projects_dir_returns_empty_no_error() {
        let tmp = LocalHome::new();
        let scan = ClaudeCodeUsageProvider.scan(tmp.path()).unwrap();
        assert_eq!(scan.events.len(), 0);
        assert_eq!(scan.stats.lines_total, 0);
    }

    #[test]
    fn available_reflects_projects_dir() {
        let tmp = LocalHome::new();
        assert!(!ClaudeCodeUsageProvider.available(tmp.path()));
        std::fs::create_dir_all(tmp.path().join(".claude/projects")).unwrap();
        assert!(ClaudeCodeUsageProvider.available(tmp.path()));
    }
}
