use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone)]
pub struct LogFile {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub modified: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct LogLine {
    pub timestamp: String,
    pub level: String,
    pub message: String,
}

fn logs_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".clawbox")
        .join("logs")
}

#[tauri::command]
pub fn get_log_files() -> Result<Vec<LogFile>, String> {
    let dir = logs_dir();

    if !dir.exists() {
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        return Ok(vec![]);
    }

    let mut files = Vec::new();

    let entries = fs::read_dir(&dir).map_err(|e| e.to_string())?;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().map_or(false, |ext| ext == "log") {
            let metadata = entry.metadata().ok();
            let modified = metadata
                .as_ref()
                .and_then(|m| m.modified().ok())
                .and_then(|t| {
                    let datetime: time::OffsetDateTime = t.into();
                    // Use RFC3339 format which is built-in
                    datetime.format(&time::format_description::well_known::Rfc3339).ok()
                        .map(|s| s.to_string())
                })
                .unwrap_or_default();

            files.push(LogFile {
                name: path.file_name().unwrap().to_string_lossy().to_string(),
                path: path.to_string_lossy().to_string(),
                size: metadata.map(|m| m.len()).unwrap_or(0),
                modified,
            });
        }
    }

    files.sort_by(|a, b| b.modified.cmp(&a.modified));
    Ok(files)
}

#[tauri::command]
pub fn get_log_content(path: String, filter: Option<String>) -> Result<Vec<LogLine>, String> {
    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let filter_lower = filter.map(|f| f.to_lowercase());

    let lines: Vec<LogLine> = content
        .lines()
        .filter_map(parse_log_line)
        .filter(|line| {
            if let Some(ref filter) = filter_lower {
                line.message.to_lowercase().contains(filter)
                    || line.level.to_lowercase().contains(filter)
            } else {
                true
            }
        })
        .collect();

    Ok(lines)
}

fn parse_log_line(line: &str) -> Option<LogLine> {
    let parts: Vec<&str> = line.splitn(4, ' ').collect();

    if parts.len() >= 4 {
        let timestamp = format!("{} {}", parts[0], parts[1]);
        let level = parts[2].trim_end_matches(':').to_lowercase();
        let message = parts[3].to_string();

        Some(LogLine {
            timestamp,
            level,
            message,
        })
    } else if parts.len() >= 1 {
        Some(LogLine {
            timestamp: String::new(),
            level: "info".to_string(),
            message: line.to_string(),
        })
    } else {
        None
    }
}
