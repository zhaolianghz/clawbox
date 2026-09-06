//! Hermes adapter:从 `~/.hermes/state.db`(SQLite)读 `session_model_usage` 表。
//!
//! Hermes 是 Hermes Agent(NousResearch)的本地 CLI,自带的 SQLite store 里
//! 已经按 `(session_id, model, billing_provider, billing_base_url, billing_mode, task)`
//! 维度把每次 model 调用的 token 累加好了。我们这里**只读**这些行,转成
//! UsageEvent 写进 ClawBox 月桶。
//!
//! Schema 兼容性:hermes 早期版本可能没有 `cache_write_tokens` /
//! `reasoning_tokens` 列 → adapter 用 `pragma_table_info` 检查列存在,
//! 缺则按 0 处理,不抛错。
//!
//! 口径选择:直接用 hermes 表里的 input_tokens / output_tokens /
//! cache_read_tokens / cache_write_tokens / reasoning_tokens,不做估算。
//! 这与 hermes 自己的计费口径同源(它表的 estimated_cost_usd 字段就是按
//! 这些数算的)。
//!
//! 去重:`session_id` + `model` + `billing_provider` + `billing_base_url`
//! + `billing_mode` + `task` 联合 key(与 PRIMARY KEY 一致)→ 自合 event_id。

use crate::usage::{ParseStats, UsageError, UsageEvent, UsageProvider, UsageScan};
use rusqlite::Connection;
use std::path::Path;

/// Hermes 自家 state.db 文件名,在 home 下。
const STATE_DB_NAME: &str = "state.db";

/// 期望的表名 — hermes 现在用的是这个名字;以后改了我们只跳过这一个
/// adapter,其它 agent 照常工作。
const TABLE_NAME: &str = "session_model_usage";

/// 已知列名(全用 Option 表达,缺失列视为 0 / NULL)。
#[derive(Default)]
struct HermesColumns {
    session_id: bool,
    model: bool,
    billing_provider: bool,
    billing_base_url: bool,
    billing_mode: bool,
    task: bool,
    input_tokens: bool,
    output_tokens: bool,
    cache_read_tokens: bool,
    cache_write_tokens: bool,
    reasoning_tokens: bool,
    last_seen: bool,
}

fn detect_columns(conn: &Connection) -> Result<HermesColumns, rusqlite::Error> {
    let mut cols = HermesColumns::default();
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({})", TABLE_NAME))?;
    let rows = stmt.query_map([], |row| {
        let name: String = row.get(1)?;
        Ok(name)
    })?;
    for name in rows {
        let n = name?;
        match n.as_str() {
            "session_id" => cols.session_id = true,
            "model" => cols.model = true,
            "billing_provider" => cols.billing_provider = true,
            "billing_base_url" => cols.billing_base_url = true,
            "billing_mode" => cols.billing_mode = true,
            "task" => cols.task = true,
            "input_tokens" => cols.input_tokens = true,
            "output_tokens" => cols.output_tokens = true,
            "cache_read_tokens" => cols.cache_read_tokens = true,
            "cache_write_tokens" => cols.cache_write_tokens = true,
            "reasoning_tokens" => cols.reasoning_tokens = true,
            "last_seen" => cols.last_seen = true,
            _ => {}
        }
    }
    Ok(cols)
}

/// unix epoch seconds → RFC3339 "YYYY-MM-DDTHH:MM:SSZ"。失败 / 负数 → 空串。
fn epoch_to_rfc3339(secs: f64) -> String {
    if secs < 0.0 || !secs.is_finite() {
        return String::new();
    }
    let secs = secs as i64;
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

/// 把 SQLite 行(列是否存在的判断在外)转成 UsageEvent。
/// `last_seen` 用于事件时间戳,缺失 → 空字符串(降级到 "unknown" day)。
#[allow(clippy::too_many_arguments)]
fn row_to_event(
    _cols: &HermesColumns,
    session_id: &str,
    model: &str,
    task: &str,
    input: u64,
    output: u64,
    cache_read: u64,
    cache_write: u64,
    reasoning: u64,
    last_seen: f64,
) -> UsageEvent {
    let ts = if last_seen > 0.0 {
        epoch_to_rfc3339(last_seen)
    } else {
        String::new()
    };
    // event_id 唯一性:同 session+model+task 的多行(实际不会发生,PRIMARY KEY
    // 强制唯一)由 store::append_events_batch 的 seen_events 三元组去重,
    // 这里合成一个确定性 key 即可。
    let event_id = format!("{}|{}|{}", session_id, model, task);
    // 关键:reasoning_tokens 不属于 output(很多 hermes 用户的 output 不含
    // reasoning,这里按 Claude 口径合并:output 单独记,reasoning 也单独进
    // 月桶的 output 列,符合 user 的"模型输出"直觉)。
    let _ = reasoning;
    UsageEvent {
        ts,
        session_id: session_id.to_string(),
        event_id,
        model: model.to_string(),
        input_tokens: input,
        cache_read_tokens: cache_read,
        cache_creation_tokens: cache_write,
        // 输出含 reasoning(对齐 codex 口径:reasoning 也算模型输出 token)
        output_tokens: output + reasoning,
    }
}

pub struct HermesUsageProvider;

impl UsageProvider for HermesUsageProvider {
    fn agent_id(&self) -> &'static str {
        "hermes"
    }

    fn available(&self, home: &Path) -> bool {
        home.join(".hermes").join(STATE_DB_NAME).exists()
    }

    fn scan(&self, home: &Path) -> Result<UsageScan, UsageError> {
        let agent_id = "hermes";
        let db_path = home.join(".hermes").join(STATE_DB_NAME);
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

        // 表不存在 → 视为该 hermes 版本太旧没这特性,正常返回空
        let table_exists: bool = conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1",
                [TABLE_NAME],
                |_| Ok(true),
            )
            .unwrap_or(false);
        if !table_exists {
            return Ok(UsageScan {
                agent_id: agent_id.into(),
                ..Default::default()
            });
        }

        let cols = match detect_columns(&conn) {
            Ok(c) => c,
            Err(e) => {
                return Err(UsageError::new(
                    agent_id,
                    "schema",
                    format!("detect_columns: {}", e),
                ));
            }
        };

        // 必须有 session_id / model 才能拼 event;任一缺失 → 致命错误(数据库损坏)
        if !cols.session_id || !cols.model {
            return Err(UsageError::new(
                agent_id,
                "schema",
                "session_model_usage 缺 session_id 或 model 列",
            ));
        }

        let names: [&str; 12] = [
            "session_id",
            "model",
            "billing_provider",
            "billing_base_url",
            "billing_mode",
            "task",
            "input_tokens",
            "output_tokens",
            "cache_read_tokens",
            "cache_write_tokens",
            "reasoning_tokens",
            "last_seen",
        ];
        let present: [bool; 12] = [
            cols.session_id,
            cols.model,
            cols.billing_provider,
            cols.billing_base_url,
            cols.billing_mode,
            cols.task,
            cols.input_tokens,
            cols.output_tokens,
            cols.cache_read_tokens,
            cols.cache_write_tokens,
            cols.reasoning_tokens,
            cols.last_seen,
        ];
        // 仅 SELECT 实际存在的列 → row[real_idx] 索引跟 SELECT 顺序一一对应
        let select_cols: Vec<&str> = names
            .iter()
            .zip(present.iter())
            .filter_map(|(n, b)| if *b { Some(*n) } else { None })
            .collect();
        let select = format!("SELECT {} FROM {}", select_cols.join(", "), TABLE_NAME);

        let mut stmt = match conn.prepare(&select) {
            Ok(s) => s,
            Err(e) => {
                return Err(UsageError::new(
                    agent_id,
                    "io",
                    format!("prepare: {}", e),
                ));
            }
        };

        let mut events: Vec<UsageEvent> = Vec::new();
        let rows = stmt.query_map([], |row| {
            // 顺序读 row[real_idx](present[i]==true 时)否则默认值。
            // 不用 FnMut 闭包共享 `&mut real_idx` — 改用 Cell<usize>
            // 提供内部可变性,所有闭包都不可变借用 Cell,然后各自 read 自己的 idx。
            let counter = std::cell::Cell::new(0usize);
            let read_str = |row: &rusqlite::Row, idx: usize, counter: &std::cell::Cell<usize>| -> String {
                if !present[idx] {
                    return String::new();
                }
                let pos = counter.get();
                let v: Option<String> = row.get(pos).unwrap_or(None);
                counter.set(pos + 1);
                v.unwrap_or_default()
            };
            let read_i64 = |row: &rusqlite::Row, idx: usize, counter: &std::cell::Cell<usize>| -> i64 {
                if !present[idx] {
                    return 0;
                }
                let pos = counter.get();
                let v: Option<i64> = row.get(pos).unwrap_or(None);
                counter.set(pos + 1);
                v.unwrap_or(0)
            };
            let read_f64 = |row: &rusqlite::Row, idx: usize, counter: &std::cell::Cell<usize>| -> f64 {
                if !present[idx] {
                    return 0.0;
                }
                let pos = counter.get();
                let v: Option<f64> = row.get(pos).unwrap_or(None);
                counter.set(pos + 1);
                v.unwrap_or(0.0)
            };

            let session_id = read_str(row, 0, &counter);
            let model = read_str(row, 1, &counter);
            let billing_provider = read_str(row, 2, &counter);
            let _billing_base_url = read_str(row, 3, &counter);
            let _billing_mode = read_str(row, 4, &counter);
            let task = read_str(row, 5, &counter);
            let input = read_i64(row, 6, &counter);
            let output = read_i64(row, 7, &counter);
            let cache_read = read_i64(row, 8, &counter);
            let cache_write = read_i64(row, 9, &counter);
            let reasoning = read_i64(row, 10, &counter);
            let last_seen = read_f64(row, 11, &counter);
            Ok((
                session_id,
                model,
                billing_provider,
                task,
                input,
                output,
                cache_read,
                cache_write,
                reasoning,
                last_seen,
            ))
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

        let mut stats = ParseStats::default();
        stats.files_scanned = 1; // 单数据库视作单"逻辑文件"
        loop {
            let row = match rows.next() {
                Some(r) => r,
                None => break,
            };
            match row {
                Ok((sid, model, _bp, task, i, o, cr, cw, r, ls)) => {
                    if sid.is_empty() || model.is_empty() {
                        stats.lines_skipped += 1;
                        continue;
                    }
                    // session_id 里的 '|' 会破坏 event_id;罕见的,直接 normalize
                    let sid_norm = sid.replace('|', "_");
                    let task_norm = if task.is_empty() { "_" } else { task.as_str() };
                    events.push(row_to_event(
                        &cols,
                        &sid_norm,
                        &model,
                        task_norm,
                        i as u64,
                        o as u64,
                        cr as u64,
                        cw as u64,
                        r as u64,
                        ls,
                    ));
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct LocalHome(std::path::PathBuf);
    impl LocalHome {
        fn new() -> Self {
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let n = COUNTER.fetch_add(1, Ordering::SeqCst);
            let dir = std::env::temp_dir().join(format!("clawbox-hermes-{}-{}", std::process::id(), n));
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
            .join("src/usage/fixtures/hermes")
            .join(name);
        std::fs::read_to_string(p).unwrap()
    }

    fn seed_db(home: &Path, sql_filename: &str) {
        let dir = home.join(".hermes");
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join(STATE_DB_NAME);
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
    fn fixture_sql_files_load_clean() {
        // 每个 fixture 在它自己的临时 db 里完整执行(SQLite 解释器对拆开的 statement
        // 不能保留 CREATE 上下文,所以必须一次跑完整 SQL 才能保证前后语句可解析)。
        for name in ["basic.sql", "missing_columns.sql"] {
            let tmp = std::env::temp_dir().join(format!(
                "hermes-fixture-check-{}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                name
            ));
            let conn = Connection::open(&tmp).unwrap();
            conn.execute_batch(&fixture_sql(name))
                .unwrap_or_else(|e| panic!("{} load failed: {}", name, e));
            // 顺便验 schema 真有这张表
            let n: i64 = conn
                .query_row("SELECT COUNT(*) FROM session_model_usage", [], |r| r.get(0))
                .unwrap();
            assert!(n >= 1, "{} should have at least 1 row, got {}", name, n);
            let _ = std::fs::remove_file(&tmp);
        }
    }

    #[test]
    fn available_reflects_state_db() {
        let tmp = LocalHome::new();
        assert!(!HermesUsageProvider.available(tmp.path()));
        std::fs::create_dir_all(tmp.path().join(".hermes")).unwrap();
        assert!(
            !HermesUsageProvider.available(tmp.path()),
            "只有目录没有 db 文件不应判 available"
        );
        std::fs::write(tmp.path().join(".hermes").join(STATE_DB_NAME), b"").unwrap();
        // 空文件不是 SQLite,可用性判断只比 path.exists(),所以会 true;
        // scan 阶段才会打开失败 → 不会让可用性假装 false 把 agent 漏掉
        assert!(HermesUsageProvider.available(tmp.path()));
    }

    #[test]
    fn missing_state_db_returns_empty_no_error() {
        let tmp = LocalHome::new();
        let scan = HermesUsageProvider.scan(tmp.path()).unwrap();
        assert_eq!(scan.events.len(), 0);
        assert_eq!(scan.stats.lines_total, 0);
    }

    #[test]
    fn basic_fixture_parses_into_events() {
        let tmp = LocalHome::new();
        seed_db(tmp.path(), "basic.sql");
        let scan = HermesUsageProvider.scan(tmp.path()).unwrap();
        // 5 row → 5 events(全部 PRIMARY KEY 不冲突)
        assert_eq!(scan.agent_id, "hermes");
        assert_eq!(scan.events.len(), 5, "events: {:?}", scan.events);
        assert_eq!(scan.stats.lines_matched, 5);
        // 验证 task 维度:同一 session_id 但不同 task 应拆成 2 个 event
        let same_session = scan
            .events
            .iter()
            .filter(|e| e.session_id == "20260411_154958_ccddc6dc")
            .count();
        assert_eq!(same_session, 2, "task 维度未拆分");
        // 验证 reasoning 合并进 output
        let with_reasoning = scan
            .events
            .iter()
            .find(|e| e.session_id == "20260411_154740_037dda")
            .unwrap();
        // 这条 reasoning=0,output=20750
        assert_eq!(with_reasoning.output_tokens, 20750);
        // 验证 cache_write 字段
        assert!(scan.events.iter().any(|e| e.cache_creation_tokens > 1000));
        // 验证 model 字段保留原始值(非 "unknown")
        assert!(scan.events.iter().any(|e| e.model == "deepseek-v4-flash"));
        // 验证 event_id 包含 task 维度(PRIMARY KEY 末段)
        assert!(scan.events.iter().any(|e| e.event_id.contains("|compact")));
    }

    #[test]
    fn missing_columns_fallback_to_zero() {
        let tmp = LocalHome::new();
        seed_db(tmp.path(), "missing_columns.sql");
        let scan = HermesUsageProvider.scan(tmp.path()).unwrap();
        assert_eq!(scan.events.len(), 1);
        // 缺 cache_write / reasoning → 应按 0
        assert_eq!(scan.events[0].cache_creation_tokens, 0);
        assert_eq!(scan.events[0].output_tokens, 50); // reasoning=0 不影响
    }

    #[test]
    fn epoch_to_rfc3339_basic() {
        // 1775916600 epoch s = 2026-04-11 14:10:00 UTC(实测校准)
        assert_eq!(epoch_to_rfc3339(1775916600.0), "2026-04-11T14:10:00Z");
        // 负数/极小 → 空串
        assert_eq!(epoch_to_rfc3339(-1.0), "");
    }
}
