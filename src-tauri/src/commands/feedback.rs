use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Feedback {
    pub id: String,
    pub category: String,
    pub message: String,
    pub contact: Option<String>,
    /// Unix epoch seconds when the feedback was submitted.
    pub created_at: i64,
}

fn feedback_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".clawbox")
        .join("feedback.json")
}

fn ensure_dir() -> Result<(), String> {
    let dir = feedback_path().parent().unwrap().to_path_buf();
    if !dir.exists() {
        fs::create_dir_all(&dir).map_err(|e| format!("Failed to create config dir: {}", e))?;
    }
    Ok(())
}

fn read_all() -> Vec<Feedback> {
    let path = feedback_path();
    if !path.exists() {
        return Vec::new();
    }
    fs::read_to_string(&path)
        .ok()
        .and_then(|c| serde_json::from_str(&c).ok())
        .unwrap_or_default()
}

fn write_all(items: &[Feedback]) -> Result<(), String> {
    ensure_dir()?;
    let content = serde_json::to_string_pretty(items)
        .map_err(|e| format!("Failed to serialize feedback: {}", e))?;
    fs::write(feedback_path(), content).map_err(|e| format!("Failed to write feedback: {}", e))
}

/// Append a feedback entry. Returns the created entry (with generated id +
/// timestamp) so the frontend can render it without a re-fetch.
#[tauri::command]
pub fn feedback_submit(
    category: String,
    message: String,
    contact: Option<String>,
) -> Result<Feedback, String> {
    if message.trim().is_empty() {
        return Err("Feedback message cannot be empty".to_string());
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let entry = Feedback {
        id: format!("fb_{}", now),
        category,
        message: message.trim().to_string(),
        contact: contact.filter(|c| !c.trim().is_empty()),
        created_at: now,
    };

    let mut items = read_all();
    items.push(entry.clone());
    write_all(&items)?;

    Ok(entry)
}

/// List feedback entries, newest first.
#[tauri::command]
pub fn feedback_list() -> Result<Vec<Feedback>, String> {
    let mut items = read_all();
    items.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_message_is_rejected() {
        let err = feedback_submit("bug".into(), "   ".into(), None).unwrap_err();
        assert!(err.contains("empty"));
    }

    #[test]
    fn serialize_roundtrip() {
        let fb = Feedback {
            id: "fb_1".into(),
            category: "bug".into(),
            message: "hello".into(),
            contact: Some("me@example.com".into()),
            created_at: 123,
        };
        let json = serde_json::to_string(&fb).unwrap();
        let back: Feedback = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "fb_1");
        assert_eq!(back.message, "hello");
        assert_eq!(back.created_at, 123);
    }
}
