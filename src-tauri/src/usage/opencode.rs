//! OpenCode adapter:从 `~/.local/share/opencode/opencode.db`(SQLite)读
//! `message` 表,字段 `data` 是 JSON,含 token 信息。
//!
//! 形状提取:
//! - `data.role == "assistant"` 且 `data.tokens` 非空 → 取 token
//! - `data.modelID` 字段取 model 名,缺失 → "unknown"
//! - `data.time.created`(ms epoch)是事件时间
//! - `message.session_id` + `message.id` 联合 → 当 session_id 与 event_id
//!
//! 兼容考虑:opencode 早期版本可能 tokens 字段结构略有差异(比如无
//! `cache.read`),我们用 `unwrap_or(0)` 防御。

use crate::usage::{ParseStats, UsageError, UsageEvent, UsageProvider, UsageScan};
use rusqlite::Connection;
use std::path::Path;

const DB_NAME: &str = "opencode.db";

pub struct OpenCodeUsageProvider;

impl UsageProvider for OpenCodeUsageProvider {
    fn agent_id(&self) -> &'static str {
        "opencode"
    }

    fn available(&self, home: &Path) -> bool {
        home.join(".local/share/opencode").join(DB_NAME).exists()
    }

    fn scan(&self, home: &Path) -> Result<UsageScan, UsageError> {
        let agent_id = "opencode";
        let db_path = home.join(".local/share/opencode").join(DB_NAME);
        if !db_path.exists() {
            return Ok(UsageScan {
                agent_id: agent_id.into(),
                ..Default::default()
            });
        }

        let conn = match Connection::open(&db_path) {
            Ok(c) => c,
            Err(e) => {
                return Err(UsageError::new(
                    agent_id,
                    "io",
                    format!("open {}: {}", db_path.display(), e),
                ));
            }
        };

        let mut stmt = match conn.prepare(
            "SELECT id, session_id, time_created, data FROM message",
        ) {
            Ok(s) => s,
            Err(e) => {
                return Err(UsageError::new(
                    agent_id,
                    "io",
                    format!("prepare: {}", e),
                ));
            }
        };

        let rows = stmt.query_map([], |row| {
            let id: String = row.get(0)?;
            let session_id: String = row.get(1)?;
            let time_created: i64 = row.get(2)?;
            let data: String = row.get(3)?;
            Ok((id, session_id, time_created, data))
        });

        let mut events: Vec<UsageEvent> = Vec::new();
        let mut stats = ParseStats {
            files_scanned: 1,
            ..Default::default()
        };

        let mut rows = match rows {
            Ok(r) => r,
            Err(e) => {
                return Err(UsageError::new(
                    agent_id,
                    "io",
                    format!("query_map: {}", e),
                ));
            }
        };

        while let Some(row) = rows.next() {
            match row {
                Ok((id, session_id, time_created, data)) => {
                    stats.lines_total += 1;
                    let entry: serde_json::Value = match serde_json::from_str(&data) {
                        Ok(v) => v,
                        Err(_) => {
                            stats.lines_skipped += 1;
                            continue;
                        }
                    };
                    let role = entry.get("role").and_then(|v| v.as_str()).unwrap_or("");
                    if role != "assistant" {
                        stats.lines_skipped += 1;
                        continue;
                    }
                    let tokens = match entry.get("tokens") {
                        Some(t) if !t.is_null() => t,
                        _ => {
                            stats.lines_skipped += 1;
                            continue;
                        }
                    };
                    let model = entry
                        .get("modelID")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string();
                    let input = tokens.get("input").and_then(|v| v.as_u64()).unwrap_or(0);
                    let output = tokens.get("output").and_then(|v| v.as_u64()).unwrap_or(0);
                    let reasoning = tokens.get("reasoning").and_then(|v| v.as_u64()).unwrap_or(0);
                    let cache_read = tokens
                        .get("cache")
                        .and_then(|c| c.get("read"))
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    let cache_write = tokens
                        .get("cache")
                        .and_then(|c| c.get("write"))
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);

                    let ts = millis_to_rfc3339(time_created);
                    events.push(UsageEvent {
                        ts,
                        session_id,
                        event_id: id,
                        model,
                        input_tokens: input,
                        cache_read_tokens: cache_read,
                        cache_creation_tokens: cache_write,
                        output_tokens: output + reasoning,
                    });
                    stats.lines_matched += 1;
                }
                Err(_) => {
                    stats.lines_skipped += 1;
                }
            }
        }

        Ok(UsageScan {
            agent_id: agent_id.into(),
            events,
            stats,
            error_note: None,
        })
    }
}

/// unix epoch ms → RFC3339 "YYYY-MM-DDTHH:MM:SSZ"。负数 / 异常 → 空串。
fn millis_to_rfc3339(ms: i64) -> String {
    if ms < 0 {
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
                std::env::temp_dir().join(format!("clawbox-opencode-{}-{}", std::process::id(), n));
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

    fn fixture_sql(name: &str) -> String {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/usage/fixtures/opencode")
            .join(name);
        std::fs::read_to_string(p).unwrap()
    }

    fn seed_db(home: &Path, sql_filename: &str) {
        let dir = home.join(".local/share/opencode");
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join(DB_NAME);
        let conn = Connection::open(&db).unwrap();
        for stmt in fixture_sql(sql_filename).split(';') {
            let s = stmt.trim();
            if s.is_empty() {
                continue;
            }
            conn.execute(s, []).unwrap();
        }
    }

    #[test]
    fn fixture_sql_loads_clean() {
        let tmp = LocalHome::new();
        seed_db(tmp.path(), "basic.sql");
        // 不崩即过
    }

    #[test]
    fn basic_fixture_parses_three_events() {
        let tmp = LocalHome::new();
        seed_db(tmp.path(), "basic.sql");
        let scan = OpenCodeUsageProvider.scan(tmp.path()).unwrap();
        assert_eq!(scan.agent_id, "opencode");
        // 5 message 行,其中 2 条 assistant 带 tokens(msg-1 / msg-2 / msg-3),
        // msg-4 user → 跳, msg-5 assistant 无 tokens → 跳
        assert_eq!(scan.events.len(), 3, "events: {:?}", scan.events);
        assert_eq!(scan.stats.lines_matched, 3);
        // 验证模型维度拆分
        let mimo_count = scan.events.iter().filter(|e| e.model == "mimo-v2.5-free").count();
        let claude_count = scan.events.iter().filter(|e| e.model == "claude-sonnet-4-5").count();
        assert_eq!(mimo_count, 2);
        assert_eq!(claude_count, 1);
        // 验证 cache_write 字段
        let with_cw = scan
            .events
            .iter()
            .find(|e| e.cache_creation_tokens > 0)
            .unwrap();
        assert_eq!(with_cw.cache_creation_tokens, 500);
        // 验证 reasoning 合并进 output
        let with_reasoning = scan.events.iter().find(|e| e.output_tokens == 250).unwrap();
        assert!(with_reasoning.cache_read_tokens > 0);
    }

    #[test]
    fn missing_db_returns_empty_no_error() {
        let tmp = LocalHome::new();
        let scan = OpenCodeUsageProvider.scan(tmp.path()).unwrap();
        assert_eq!(scan.events.len(), 0);
        assert_eq!(scan.stats.lines_total, 0);
    }

    #[test]
    fn available_reflects_db() {
        let tmp = LocalHome::new();
        assert!(!OpenCodeUsageProvider.available(tmp.path()));
        std::fs::create_dir_all(tmp.path().join(".local/share/opencode")).unwrap();
        std::fs::write(tmp.path().join(".local/share/opencode").join(DB_NAME), b"").unwrap();
        assert!(OpenCodeUsageProvider.available(tmp.path()));
    }

    #[test]
    fn millis_to_rfc3339_basic() {
        // 1775916600000 ms = 2026-04-11 14:10:00 UTC(实测校准,避开夏令时坑)
        assert_eq!(millis_to_rfc3339(1775916600000), "2026-04-11T14:10:00Z");
        // 负数 → 空串
        assert_eq!(millis_to_rfc3339(-1), "");
    }
}
