//! Review engine — orchestrates read-only ACP reviewers + a summarizer,
//! produces a structured report persisted under ~/.clawbox/reviews/.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ReviewScope {
    WholeProject,
    GitDiff { base: String },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RoleAssignment {
    pub adapter_id: String,
    pub model: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ReviewTask {
    pub id: String,
    pub project_path: String,
    pub scope: ReviewScope,
    pub reviewers: Vec<RoleAssignment>,
    pub summarizer: RoleAssignment,
    pub created_at: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Finding {
    pub file: String,
    pub line: Option<u32>,
    pub severity: Severity,
    pub title: String,
    pub detail: String,
    pub reviewer: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ReviewStatus {
    Running,
    Completed,
    Failed { message: String },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ReviewReport {
    pub task_id: String,
    pub findings: Vec<Finding>,
    pub summary: String,
    pub status: ReviewStatus,
    pub created_at: i64,
}

pub fn reviews_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".clawbox")
        .join("reviews")
}

fn ensure_dir() -> Result<(), String> {
    let d = reviews_dir();
    if !d.exists() {
        fs::create_dir_all(&d).map_err(|e| format!("create reviews dir: {}", e))?;
    }
    Ok(())
}

pub fn save_report(r: &ReviewReport) -> Result<(), String> {
    ensure_dir()?;
    let path = reviews_dir().join(format!("{}.json", r.task_id));
    let content = serde_json::to_string_pretty(r).map_err(|e| e.to_string())?;
    fs::write(path, content).map_err(|e| format!("write report: {}", e))
}

pub fn load_report(task_id: &str) -> Result<ReviewReport, String> {
    let path = reviews_dir().join(format!("{}.json", task_id));
    let content = fs::read_to_string(path).map_err(|e| format!("read report: {}", e))?;
    serde_json::from_str(&content).map_err(|e| format!("parse report: {}", e))
}

pub fn list_reports() -> Vec<ReviewReport> {
    let dir = reviews_dir();
    if !dir.exists() {
        return Vec::new();
    }
    let mut out: Vec<ReviewReport> = fs::read_dir(&dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| e.path().extension().map(|x| x == "json").unwrap_or(false))
                .filter_map(|e| fs::read_to_string(e.path()).ok())
                .filter_map(|c| serde_json::from_str::<ReviewReport>(&c).ok())
                .collect()
        })
        .unwrap_or_default();
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_roundtrip() {
        let r = ReviewReport {
            task_id: "t1".into(),
            findings: vec![Finding {
                file: "a.rs".into(), line: Some(10), severity: Severity::Warning,
                title: "x".into(), detail: "y".into(), reviewer: "claude-agent-acp".into(),
            }],
            summary: "s".into(),
            status: ReviewStatus::Completed,
            created_at: 1,
        };
        let json = serde_json::to_string(&r).unwrap();
        let back: ReviewReport = serde_json::from_str(&json).unwrap();
        assert_eq!(back.task_id, "t1");
        assert_eq!(back.findings.len(), 1);
    }

    #[test]
    fn scope_serializes_gitdiff() {
        let s = ReviewScope::GitDiff { base: "main".into() };
        let j = serde_json::to_string(&s).unwrap();
        assert!(j.contains("main"));
    }
}
