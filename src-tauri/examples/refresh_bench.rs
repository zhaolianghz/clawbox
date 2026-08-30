//! 用量刷新基准测试 — 在你机器上实测 refresh 耗时。
//!
//! 用法:`cargo run --example refresh_bench --release`
//!
//! 第一次跑写入 seen_events;第二次跑全部 dedup — 两次对比即可看出 dedup 工作正常。

use std::path::PathBuf;
use std::time::Instant;

fn main() {
    let home = dirs::home_dir().expect("no home dir");
    println!("== refresh_bench ==");
    println!("home: {}", home.display());
    println!();

    // 第一次:全量扫描
    let t = Instant::now();
    let r1 = clawbox_lib::usage::aggregate::refresh(
        &home,
        &clawbox_lib::commands::config::Config::default(),
        &Default::default(),
    );
    let d1 = t.elapsed();
    println!("#1 first scan: {:?}", d1);
    match &r1 {
        Ok(rep) => {
            println!(
                "  added_events={} added_buckets={} deduped={}",
                rep.added_events, rep.added_buckets, rep.parse_health.added_events_deduped
            );
            for (agent_id, stats) in &rep.parse_health.per_agent {
                println!(
                    "  {}: files={} matched={} skipped={} matched_ratio={:.1}%",
                    agent_id,
                    stats.files_scanned,
                    stats.lines_matched,
                    stats.lines_skipped,
                    if stats.lines_total > 0 {
                        stats.lines_matched as f64 / stats.lines_total as f64 * 100.0
                    } else {
                        100.0
                    }
                );
            }
        }
        Err(e) => println!("  ERR: {}", e),
    }
    println!();

    // 第二次:同数据 → 全部 dedup,add_events ≈ 0
    let t = Instant::now();
    let r2 = clawbox_lib::usage::aggregate::refresh(
        &home,
        &clawbox_lib::commands::config::Config::default(),
        &Default::default(),
    );
    let d2 = t.elapsed();
    println!("#2 second scan (same data): {:?}", d2);
    match &r2 {
        Ok(rep) => {
            println!(
                "  added_events={} added_buckets={} deduped={}",
                rep.added_events, rep.added_buckets, rep.parse_health.added_events_deduped
            );
        }
        Err(e) => println!("  ERR: {}", e),
    }
    println!();

    println!("# 验证:d1 和 d2 后,月桶数据应该完全一致(无翻倍)");
    let months = clawbox_lib::usage::store::read_all(&home);
    let mut total_input = 0u64;
    let mut total_events = 0u64;
    for m in &months {
        for (_day, day_buckets) in &m.buckets {
            for (_key, totals) in day_buckets {
                total_input += totals.input;
                total_events += totals.events;
            }
        }
    }
    println!(
        "  total across {} months: input={} events={}",
        months.len(),
        total_input,
        total_events
    );
    // 看存储大小
    let usage_dir = home.join(".clawbox/usage");
    if usage_dir.exists() {
        let total_size: u64 = std::fs::read_dir(&usage_dir)
            .map(|rd| {
                rd.flatten()
                    .flat_map(|e| std::fs::metadata(e.path()).ok())
                    .map(|m| m.len())
                    .sum()
            })
            .unwrap_or(0);
        println!(
            "  usage/ dir: {} bytes ({:.1} KB)",
            total_size,
            total_size as f64 / 1024.0
        );
    }

    // 不要留着脏数据污染用户机器 — 提示用户清理
    let _ = r1;
    let _ = r2;
    let _ = PathBuf::from(&home);
    println!();
    println!("==> 跑完没改任何代码,只往 ~/.clawbox/usage/ 写了月桶。");
    println!("    想清理:`rm -rf ~/.clawbox/usage/`");
}
