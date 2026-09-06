//! Qoder 桌面端 adapter:
//! 读 `~/Library/Application Support/Qoder/SharedClientCache/cache/db/local.db`
//! 的 `chat_message.token_info`(JSON) + `chat_record.extra.modelConfig`(model)。
//!
//! 字段口径:
//! - `token_info.prompt_tokens` → input_tokens(Anthropic 口径:含 cached)
//! - `token_info.completion_tokens` → output_tokens
//! - `token_info.cached_tokens` → cache_read_tokens(命中读)
//! - 没有 cache_creation_tokens(Qoder 不单独统计,按 0)
//!
//! model 来源:`chat_record.extra` 是 JSON,优先取 `modelConfig.key`(具体 model id),
//! 缺失则取 `chat_message.model_info.model_key`,再缺失 → "unknown"。

use crate::usage::{ParseStats, UsageError, UsageEvent, UsageProvider, UsageScan};
use rusqlite::Connection;
use std::path::{Path, PathBuf};

#[cfg(target_os = "macos")]
fn db_path(home: &Path) -> Option<PathBuf> {
    home
        .join("Library/Application Support/Qoder/SharedClientCache/cache/db/local.db")
        .into()
}

#[cfg(target_os = "linux")]
fn db_path(home: &Path) -> Option<PathBuf> {
    Some(home.join(".config/Qoder/SharedClientCache/cache/db/local.db"))
}

#[cfg(target_os = "windows")]
fn db_path(home: &Path) -> Option<PathBuf> {
    // %APPDATA%\Qoder\SharedClientCache\cache\db\local.db
    home.join("AppData/Roaming/Qoder/SharedClientCache/cache/db/local.db").into()
}

pub struct QoderUsageProvider;

impl UsageProvider for QoderUsageProvider {
    fn agent_id(&self) -> &'static str {
        // 必须跟 src-tauri/src/agents/mod.rs 里 AGENTS[].id 完全一致,
        // 否则 aggregate 落桶按这个 key 但 sync / provider binding 查不到。
        "qodercli"
    }

    fn available(&self, home: &Path) -> bool {
        db_path(home).map(|p| p.exists()).unwrap_or(false)
    }

    fn scan(&self, home: &Path) -> Result<UsageScan, UsageError> {
        let agent_id = "qoder";
        let path = match db_path(home) {
            Some(p) => p,
            None => {
                return Ok(UsageScan {
                    agent_id: agent_id.into(),
                    ..Default::default()
                });
            }
        };
        if !path.exists() {
            return Ok(UsageScan {
                agent_id: agent_id.into(),
                ..Default::default()
            });
        }

        let conn = match Connection::open(&path) {
            Ok(c) => c,
            Err(e) => {
                return Err(UsageError::new(
                    agent_id,
                    "io",
                    format!("open {}: {}", path.display(), e),
                ));
            }
        };

        // chat_message 表不存在 → 视为未启用
        let table_exists: bool = conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type='table' AND name='chat_message'",
                [],
                |_| Ok(true),
            )
            .unwrap_or(false);
        if !table_exists {
            return Ok(UsageScan {
                agent_id: agent_id.into(),
                ..Default::default()
            });
        }

        let mut stmt = match conn.prepare(
            "SELECT id, session_id, request_id, role, content, gmt_create, token_info, model_info \
             FROM chat_message WHERE role='assistant'",
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
            let session_id: Option<String> = row.get(1)?;
            let request_id: Option<String> = row.get(2)?;
            let role: String = row.get(3)?;
            let _content: String = row.get(4)?;
            let gmt_create: i64 = row.get(5)?;
            let token_info: String = row.get(6)?;
            let model_info: String = row.get(7)?;
            Ok((id, session_id, request_id, role, gmt_create, token_info, model_info))
        });

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

        let mut stats = ParseStats {
            files_scanned: 1,
            ..Default::default()
        };
        let mut events: Vec<UsageEvent> = Vec::new();

        while let Some(row) = rows.next() {
            match row {
                Ok((id, sid, rid, _role, ts, token_info, model_info)) => {
                    stats.lines_total += 1;
                    if token_info.is_empty() || token_info == "null" {
                        stats.lines_skipped += 1;
                        continue;
                    }
                    let token: serde_json::Value = match serde_json::from_str(&token_info) {
                        Ok(v) => v,
                        Err(_) => {
                            stats.lines_skipped += 1;
                            continue;
                        }
                    };
                    let prompt = token
                        .get("prompt_tokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    let completion = token
                        .get("completion_tokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    let cached = token
                        .get("cached_tokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    if prompt == 0 && completion == 0 && cached == 0 {
                        // 全 0 当作无意义,跳过
                        stats.lines_skipped += 1;
                        continue;
                    }
                    // model: 优先 message 自身的 model_info,缺失 → "unknown"
                    let model = extract_model_from_model_info(&model_info)
                        .or_else(|| extract_model_from_extra_lookup(&conn, rid.as_deref()))
                        .unwrap_or_else(|| "unknown".to_string());

                    events.push(UsageEvent {
                        ts: millis_to_rfc3339(ts),
                        session_id: sid.unwrap_or_default(),
                        event_id: id,
                        model,
                        input_tokens: prompt,
                        cache_read_tokens: cached,
                        cache_creation_tokens: 0,
                        output_tokens: completion,
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

fn extract_model_from_model_info(s: &str) -> Option<String> {
    if s.is_empty() {
        return None;
    }
    let v: serde_json::Value = serde_json::from_str(s).ok()?;
    v.get("model_key")
        .and_then(|x| x.as_str())
        .map(|s| s.to_string())
        .or_else(|| {
            v.get("model")
                .and_then(|x| x.as_str())
                .map(|s| s.to_string())
        })
}

fn extract_model_from_extra_lookup(conn: &Connection, request_id: Option<&str>) -> Option<String> {
    let rid = request_id?;
    // chat_record.extra JSON 的 modelConfig.key 是 model 真名
    let extra: String = conn
        .query_row(
            "SELECT extra FROM chat_record WHERE request_id = ?1",
            [rid],
            |row| row.get(0),
        )
        .ok()?;
    let v: serde_json::Value = serde_json::from_str(&extra).ok()?;
    v.get("modelConfig")
        .and_then(|m| m.get("key"))
        .and_then(|x| x.as_str())
        .map(|s| s.to_string())
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct LocalHome(std::path::PathBuf);
    impl LocalHome {
        fn new() -> Self {
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let n = COUNTER.fetch_add(1, Ordering::SeqCst);
            let dir = std::env::temp_dir().join(format!("clawbox-qoder-{}-{}", std::process::id(), n));
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
            .join("src/usage/fixtures/qoder")
            .join(name);
        std::fs::read_to_string(p).unwrap()
    }

    fn seed_db(home: &Path, sql_filename: &str) {
        // 在测试里强制走 macOS 路径布局,但 home 起点换成 tmp
        let dir = home.join("Library/Application Support/Qoder/SharedClientCache/cache/db");
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("local.db");
        let conn = Connection::open(&db).unwrap();
        conn.execute_batch(&fixture_sql(sql_filename)).unwrap();
    }

    #[test]
    fn fixture_sql_loads_clean() {
        let tmp = LocalHome::new();
        seed_db(tmp.path(), "basic.sql");
    }

    #[test]
    fn basic_fixture_parses_three_assistant_with_tokens() {
        let tmp = LocalHome::new();
        seed_db(tmp.path(), "basic.sql");
        let scan = QoderUsageProvider.scan(tmp.path()).unwrap();
        // assistant with token_info: 3 条 + 1 条 token_info 为空的(跳过) = 3 个 event
        assert_eq!(scan.events.len(), 3, "events: {:?}", scan.events);
        assert_eq!(scan.stats.lines_matched, 3);
        // model 应来自 chat_record.extra.modelConfig.key(因为 message.model_info 也填了,但优先 extra)
        let opus = scan
            .events
            .iter()
            .filter(|e| e.model == "claude-opus-4-5")
            .count();
        assert_eq!(opus, 2, "opus 应该有 2 条");
        let sonnet = scan
            .events
            .iter()
            .filter(|e| e.model == "claude-sonnet-4-5")
            .count();
        assert_eq!(sonnet, 1);
        // cache_read 字段透传
        assert!(scan
            .events
            .iter()
            .any(|e| e.cache_read_tokens == 14891));
        // 全 0 token 被跳过:stats.lines_skipped 应该是 1
        assert_eq!(scan.stats.lines_skipped, 1);
    }

    #[test]
    fn missing_db_returns_empty() {
        let tmp = LocalHome::new();
        let scan = QoderUsageProvider.scan(tmp.path()).unwrap();
        assert_eq!(scan.events.len(), 0);
    }
}
