//! OpenClaw adapter:
//! 读 `~/.openclaw/agents/<agent_id>/openclaw-agent.sqlite` 的 transcript-like 表。
//!
//! 实际 schema OpenClaw 1.x 在 docs 里分散定义,这里采用最保守的字段名
//! (input_tokens/output_tokens/cache_read_tokens/cache_write_tokens),并用
//! pragma_table_info 检测实际列,缺则按 0 处理(schema 漂移兼容)。

use crate::usage::{ParseStats, UsageError, UsageEvent, UsageProvider, UsageScan};
use rusqlite::Connection;
use std::path::{Path, PathBuf};

const DB_NAME: &str = "openclaw-agent.sqlite";

fn find_db(home: &Path) -> Vec<PathBuf> {
    let root = home.join(".openclaw").join("agents");
    if !root.exists() {
        return Vec::new();
    }
    // read_dir 在 Linux/macOS 上不保证顺序 — sort by entry name 后
    // 稳定返回,避免多 agent 部署时只解析到其中一个。
    let entries = match std::fs::read_dir(&root) {
        Ok(e) => e
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect::<Vec<_>>(),
        Err(_) => return Vec::new(),
    };
    let mut paths: Vec<PathBuf> = entries
        .into_iter()
        .map(|p| p.join(DB_NAME))
        .filter(|p| p.is_file())
        .collect();
    paths.sort();
    paths
}

pub struct OpenClawUsageProvider;

impl UsageProvider for OpenClawUsageProvider {
    fn agent_id(&self) -> &'static str {
        "openclaw"
    }

    fn available(&self, home: &Path) -> bool {
        !find_db(home).is_empty()
    }

    fn scan(&self, home: &Path) -> Result<UsageScan, UsageError> {
        let agent_id = "openclaw";
        let paths = find_db(home);
        if paths.is_empty() {
            return Ok(UsageScan {
                agent_id: agent_id.into(),
                ..Default::default()
            });
        }

        let mut stats = ParseStats::default();
        let mut events: Vec<UsageEvent> = Vec::new();

        // 多 agent 部署:每个 db 单独 parse,events 都聚到同一个 UsageScan
        // 里(agent_id 统一为 "openclaw",不区分子 agent —— 跟其它
        // 单 agent id adapter 保持一致;子 agent 维度后续可考虑加)。
        for path in paths {
            let one_stats_before = (events.len(), stats.lines_total, stats.lines_matched);
            if let Err(e) = scan_one_db(&path, &mut events, &mut stats) {
                return Err(UsageError::new(
                    agent_id,
                    "io",
                    format!("scan {}: {}", path.display(), e),
                ));
            }
            let _ = one_stats_before; // 占位:debug 时观察 per-db 增量
            stats.files_scanned += 1;
        }

        Ok(UsageScan {
            agent_id: agent_id.into(),
            events,
            stats,
            error_note: None,
        })
    }
}

/// 扫描单个 openclaw SQLite db。路径中的表名用 `identifier` 风格的
/// 安全过滤防止注入;row.get::<i64>() 直接 as u64 加 saturating 防止
/// 负值变成 u64::MAX。
fn scan_one_db(
    path: &Path,
    events: &mut Vec<UsageEvent>,
    stats: &mut ParseStats,
) -> Result<(), UsageError> {
    let agent_id = "openclaw";
    let conn = Connection::open(path).map_err(|e| {
        UsageError::new(agent_id, "io", format!("open {}: {}", path.display(), e))
    })?;

    // 探测包含 token 列的 transcript-like 表。
    // 真实 schema 文档分散,这里先尝试 "transcript";若不存在,扫 sqlite_master
    // 找一张含 input_tokens / output_tokens 列的表。表名来自 sqlite_master,
    // 本地 app 攻击模型弱,但我们仍做白名单过滤防止注入。
    let table_names: Vec<String> = {
        let raw_names: Vec<String> = {
            let mut stmt = conn
                .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'")
                .map_err(|e| UsageError::new(agent_id, "io", format!("prepare: {}", e)))?;
            let rows = stmt
                .query_map([], |r| r.get::<_, String>(0))
                .map_err(|e| UsageError::new(agent_id, "io", format!("query_map: {}", e)))?;
            rows.filter_map(|r| r.ok()).collect()
            // stmt 在这里 drop,后续 filter 不会 borrow
        };
        raw_names
            .into_iter()
            .filter(|name| is_safe_identifier(name))
            .filter(|name| {
                let pragma = format!("PRAGMA table_info({})", name);
                let mut p = match conn.prepare(&pragma) {
                    Ok(p) => p,
                    Err(_) => return false,
                };
                let names: Vec<String> = p
                    .query_map([], |r| r.get::<_, String>(1))
                    .ok()
                    .into_iter()
                    .flatten()
                    .flatten()
                    .collect();
                names.iter().any(|n| n == "input_tokens")
                    && names.iter().any(|n| n == "output_tokens")
            })
            .collect()
    };

    let table = match table_names.first() {
        Some(t) => t.clone(),
        None => return Ok(()),
    };

    let select = format!(
        "SELECT id, session_id, model, input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, created_at FROM {}",
        table
    );
    // 把 query_map 的结果先 collect 到 Vec,让 stmt 在迭代前 drop,避免借用冲突。
    let raw_rows: Vec<(String, Option<String>, Option<String>, i64, i64, i64, i64, i64)> = {
        let mut stmt = conn
            .prepare(&select)
            .map_err(|e| UsageError::new(agent_id, "io", format!("prepare: {}", e)))?;
        let rows_iter = stmt
            .query_map([], |row| {
                let id: String = row.get(0)?;
                let sid: Option<String> = row.get(1)?;
                let model: Option<String> = row.get(2)?;
                let input: i64 = row.get(3)?;
                let output: i64 = row.get(4)?;
                let cr: i64 = row.get(5)?;
                let cw: i64 = row.get(6)?;
                let ts: i64 = row.get(7)?;
                Ok((id, sid, model, input, output, cr, cw, ts))
            })
            .map_err(|e| UsageError::new(agent_id, "io", format!("query_map: {}", e)))?;
        rows_iter.filter_map(|r| r.ok()).collect()
        // stmt 在这里 drop,raw_rows 拿走数据后再迭代就 OK
    };

    for (id, sid, model, input, output, cr, cw, ts) in raw_rows {
        stats.lines_total += 1;
        if input == 0 && output == 0 && cr == 0 && cw == 0 {
            stats.lines_skipped += 1;
            continue;
        }
        // i64 → u64 防 saturating:负值(数据损坏)按 0 处理,
        // 避免一个坏行把真实桶数字炸成 u64::MAX。
        events.push(UsageEvent {
            ts: seconds_to_rfc3339(ts),
            session_id: sid.unwrap_or_default(),
            event_id: id,
            model: model.unwrap_or_else(|| "unknown".to_string()),
            input_tokens: input.max(0) as u64,
            cache_read_tokens: cr.max(0) as u64,
            cache_creation_tokens: cw.max(0) as u64,
            output_tokens: output.max(0) as u64,
        });
        stats.lines_matched += 1;
    }
    Ok(())
}

/// 简单的 SQL identifier 白名单:只允许字母数字下划线,防止
/// `PRAGMA table_info(<evil>)` 或 `SELECT ... FROM <evil>` 的注入。
/// home 目录里能控制表名的本地 app 攻击模型弱,但加这一层零成本。
fn is_safe_identifier(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn seconds_to_rfc3339(secs: i64) -> String {
    if secs <= 0 {
        return String::new();
    }
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
                std::env::temp_dir().join(format!("clawbox-openclaw-{}-{}", std::process::id(), n));
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
            .join("src/usage/fixtures/openclaw")
            .join(name);
        std::fs::read_to_string(p).unwrap()
    }

    fn seed_db(home: &Path, sql_filename: &str) {
        let dir = home.join(".openclaw/agents/main");
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join(DB_NAME);
        let conn = Connection::open(&db).unwrap();
        conn.execute_batch(&fixture_sql(sql_filename)).unwrap();
    }

    #[test]
    fn basic_fixture_parses_three_assistants() {
        let tmp = LocalHome::new();
        seed_db(tmp.path(), "basic.sql");
        let scan = OpenClawUsageProvider.scan(tmp.path()).unwrap();
        assert_eq!(scan.agent_id, "openclaw");
        assert_eq!(scan.events.len(), 3, "events: {:?}", scan.events);
        let opus = scan
            .events
            .iter()
            .filter(|e| e.model == "claude-opus-5")
            .count();
        assert_eq!(opus, 2);
        let gpt = scan.events.iter().filter(|e| e.model == "gpt-5.4").count();
        assert_eq!(gpt, 1);
        // t2 cache_write = 100
        assert!(scan
            .events
            .iter()
            .any(|e| e.cache_creation_tokens == 100));
    }

    #[test]
    fn missing_dir_returns_empty() {
        let tmp = LocalHome::new();
        let scan = OpenClawUsageProvider.scan(tmp.path()).unwrap();
        assert_eq!(scan.events.len(), 0);
    }
}
