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

/// A task id must be non-empty and contain only `[A-Za-z0-9_-]` — it is used
/// as a filename under the reviews dir, so this blocks path traversal
/// (`../../etc/passwd`) and separator/odd-char injection.
fn valid_task_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

pub fn save_report(r: &ReviewReport) -> Result<(), String> {
    if !valid_task_id(&r.task_id) {
        return Err("invalid task id".to_string());
    }
    ensure_dir()?;
    let path = reviews_dir().join(format!("{}.json", r.task_id));
    let content = serde_json::to_string_pretty(r).map_err(|e| e.to_string())?;
    fs::write(path, content).map_err(|e| format!("write report: {}", e))
}

pub fn load_report(task_id: &str) -> Result<ReviewReport, String> {
    if !valid_task_id(task_id) {
        return Err("invalid task id".to_string());
    }
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

use crate::acp::permission::PermissionPolicy;
use crate::acp::session::AcpSession;
use std::path::Path;

pub fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn reviewer_prompt(scope: &ReviewScope) -> String {
    let scope_desc = match scope {
        ReviewScope::WholeProject => "the entire project in the working directory".to_string(),
        ReviewScope::GitDiff { base } => {
            format!("the changes in `git diff {}...HEAD` (only the modified lines)", base)
        }
    };
    format!(
        "You are a strict code reviewer. Review {scope_desc}. \
Read files as needed (you have read-only access; do not attempt to modify anything). \
Report concrete issues: bugs, security problems, and clear correctness defects. \
Respond with ONLY a JSON array in a ```json fenced block, each item: \
{{\"file\": string, \"line\": number|null, \"severity\": \"info\"|\"warning\"|\"error\", \
\"title\": short string, \"detail\": string}}. \
If you find nothing, return []."
    )
}

pub fn summarizer_prompt(findings: &[Finding]) -> String {
    let json = serde_json::to_string_pretty(findings).unwrap_or_else(|_| "[]".to_string());
    format!(
        "You are a review summarizer. Below are findings from multiple reviewers as JSON. \
Deduplicate near-identical items and write a concise plain-text executive summary \
(3-6 sentences) highlighting the most important issues by severity. \
Do not output JSON, just the prose summary.\n\nFINDINGS:\n{json}"
    )
}

/// Run all reviewers (sequentially for v1 — simpler; parallel is a v2 opt),
/// collect findings, run the summarizer, persist and return the report.
pub async fn run_review(task: ReviewTask) -> Result<ReviewReport, String> {
    let cwd = Path::new(&task.project_path);
    let mut all: Vec<Finding> = Vec::new();

    for role in &task.reviewers {
        let session = AcpSession::start(&role.adapter_id, cwd, PermissionPolicy::ReadOnly).await?;
        let res = session.prompt(&reviewer_prompt(&task.scope)).await?;
        all.extend(parse_findings(&role.adapter_id, &res.text));
    }

    let summary = if all.is_empty() {
        "No issues found.".to_string()
    } else {
        let session =
            AcpSession::start(&task.summarizer.adapter_id, cwd, PermissionPolicy::ReadOnly).await?;
        session.prompt(&summarizer_prompt(&all)).await?.text
    };

    let report = ReviewReport {
        task_id: task.id.clone(),
        findings: all,
        summary,
        status: ReviewStatus::Completed,
        created_at: now_secs(),
    };
    save_report(&report)?;
    Ok(report)
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

    #[test]
    fn valid_task_id_spot_checks() {
        assert!(valid_task_id("review_1720000000"));
        assert!(valid_task_id("abc-DEF_123"));
        assert!(!valid_task_id(""));
        assert!(!valid_task_id("../../etc/passwd"));
        assert!(!valid_task_id("a/b"));
        assert!(!valid_task_id("a.b"));
        assert!(!valid_task_id("a b"));
    }

    #[test]
    fn load_report_rejects_path_traversal() {
        let e = load_report("../../etc/passwd").unwrap_err();
        assert_eq!(e, "invalid task id");
    }

    #[test]
    fn save_report_rejects_invalid_task_id() {
        let r = ReviewReport {
            task_id: "../evil".into(),
            findings: vec![],
            summary: String::new(),
            status: ReviewStatus::Completed,
            created_at: 1,
        };
        assert_eq!(save_report(&r).unwrap_err(), "invalid task id");
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

#[cfg(test)]
mod orchestration_tests {
    use super::*;

    #[test]
    fn reviewer_prompt_mentions_json_and_scope() {
        let p = reviewer_prompt(&ReviewScope::GitDiff { base: "main".into() });
        assert!(p.to_lowercase().contains("json"));
        assert!(p.contains("main"));
    }

    #[test]
    fn reviewer_prompt_wholeproject() {
        let p = reviewer_prompt(&ReviewScope::WholeProject);
        assert!(p.to_lowercase().contains("json"));
    }

    #[test]
    fn summarizer_prompt_includes_findings() {
        let f = vec![Finding {
            file: "a.rs".into(), line: Some(1), severity: Severity::Error,
            title: "boom".into(), detail: "d".into(), reviewer: "r".into(),
        }];
        let p = summarizer_prompt(&f);
        assert!(p.contains("boom"));
    }
}
