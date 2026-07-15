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

/// Extract a JSON array from agent text: prefer a ```json fenced block,
/// else the first top-level `[...]` span.
pub fn extract_json_block(text: &str) -> Option<&str> {
    if let Some(start) = text.find("```json") {
        let rest = &text[start + 7..];
        if let Some(end) = rest.find("```") {
            return Some(rest[..end].trim());
        }
    }
    let lb = text.find('[')?;
    let rb = text.rfind(']')?;
    if rb > lb {
        Some(&text[lb..=rb])
    } else {
        None
    }
}

#[derive(Deserialize)]
struct RawFinding {
    file: String,
    line: Option<u32>,
    severity: Option<String>,
    title: String,
    detail: Option<String>,
}

fn severity_from_str(s: Option<&str>) -> Severity {
    match s.map(|x| x.to_lowercase()).as_deref() {
        Some("error") => Severity::Error,
        Some("warning") => Severity::Warning,
        _ => Severity::Info,
    }
}

pub fn parse_findings(reviewer: &str, agent_text: &str) -> Vec<Finding> {
    if let Some(block) = extract_json_block(agent_text) {
        if let Ok(raws) = serde_json::from_str::<Vec<RawFinding>>(block) {
            return raws
                .into_iter()
                .map(|r| Finding {
                    file: r.file,
                    line: r.line,
                    severity: severity_from_str(r.severity.as_deref()),
                    title: r.title,
                    detail: r.detail.unwrap_or_default(),
                    reviewer: reviewer.to_string(),
                })
                .collect();
        }
    }
    // Fallback: keep the agent's prose as one Info finding so nothing is lost.
    vec![Finding {
        file: String::new(),
        line: None,
        severity: Severity::Info,
        title: "Unstructured review output".to_string(),
        detail: agent_text.trim().to_string(),
        reviewer: reviewer.to_string(),
    }]
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

#[cfg(test)]
mod parse_tests {
    use super::*;

    #[test]
    fn parses_fenced_json() {
        let text = "Here are issues:\n```json\n[{\"file\":\"a.rs\",\"line\":3,\"severity\":\"warning\",\"title\":\"t\",\"detail\":\"d\"}]\n```\ndone";
        let f = parse_findings("claude-agent-acp", text);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].file, "a.rs");
        assert_eq!(f[0].line, Some(3));
        assert_eq!(f[0].reviewer, "claude-agent-acp");
    }

    #[test]
    fn falls_back_to_text_on_invalid_json() {
        let text = "I found a null-deref but I'm not giving you JSON.";
        let f = parse_findings("codex-acp", text);
        assert_eq!(f.len(), 1);
        assert!(matches!(f[0].severity, Severity::Info));
        assert!(f[0].detail.contains("null-deref"));
    }

    #[test]
    fn empty_array_yields_no_findings() {
        let f = parse_findings("claude-agent-acp", "```json\n[]\n```");
        assert_eq!(f.len(), 0);
    }
}
