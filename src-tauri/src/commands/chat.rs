use std::process::Command;

/// The chat WebSocket (frontend `chat.ts`) authenticates against the OpenClaw
/// gateway with the token stored at `gateway.auth.token` in the openclaw
/// config. An empty string means no token is configured (auth mode "none"),
/// which the frontend handles by connecting without a token.
#[tauri::command]
pub fn get_gateway_token() -> Result<String, String> {
    let output = Command::new("openclaw")
        .args(["config", "get", "gateway.auth.token"])
        .output()
        .map_err(|e| format!("Failed to run openclaw: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "openclaw config get failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(parse_token_output(&stdout))
}

/// `openclaw config get` prints "Config path not found: ..." (exit 0) when the
/// path is absent, and may quote string values.
fn parse_token_output(stdout: &str) -> String {
    if stdout.is_empty() || stdout.starts_with("Config path not found") {
        return String::new();
    }
    stdout.trim_matches('"').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_token() {
        assert_eq!(parse_token_output("abc123"), "abc123");
    }

    #[test]
    fn strips_json_quotes() {
        assert_eq!(parse_token_output("\"abc123\""), "abc123");
    }

    #[test]
    fn not_found_means_no_token() {
        assert_eq!(
            parse_token_output("Config path not found: gateway.auth.token. Run openclaw config validate to inspect config shape."),
            ""
        );
    }

    #[test]
    fn empty_output_means_no_token() {
        assert_eq!(parse_token_output(""), "");
    }
}
