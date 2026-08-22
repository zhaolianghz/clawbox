use serde::Serialize;
use std::time::{Duration, Instant};

/// Result of a provider connectivity test. camelCase on the wire to match the
/// frontend `ProviderTestResult` type field-for-field.
#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderTestResult {
    pub ok: bool,
    pub latency_ms: u64,
    /// Model ids fetched from the provider (on success; may be empty if the
    /// response was 200 but not parseable).
    pub models: Vec<String>,
    /// Short human-readable reason on failure. English, shown verbatim.
    pub error: Option<String>,
}

impl ProviderTestResult {
    fn fail(latency_ms: u64, error: impl Into<String>) -> Self {
        Self {
            ok: false,
            latency_ms,
            models: Vec::new(),
            error: Some(error.into()),
        }
    }
}

/// Build the models-listing URL for a provider endpoint.
///
/// - `openai`: `{base}/models`
/// - `anthropic`: `{base}/v1/models`, except when the base already ends with
///   `/v1` (common in catalog hosts like `https://api.anthropic.com/v1`), in
///   which case just `/models` is appended to avoid `/v1/v1/models`.
///
/// Trailing slashes on `base_url` are stripped first.
pub fn build_models_url(base_url: &str, flavor: &str) -> String {
    let base = base_url.trim().trim_end_matches('/');
    match flavor {
        "anthropic" => {
            if base.ends_with("/v1") {
                format!("{}/models", base)
            } else {
                format!("{}/v1/models", base)
            }
        }
        // openai and anything else: plain /models next to the base.
        _ => format!("{}/models", base),
    }
}

/// Build the Anthropic messages URL for a base endpoint, following the same
/// `/v1` de-duplication rule as [`build_models_url`]. Used as a reachability
/// fallback for Anthropic-compatible gateways that don't expose `/v1/models`.
pub fn build_messages_url(base_url: &str) -> String {
    let base = base_url.trim().trim_end_matches('/');
    if base.ends_with("/v1") {
        format!("{}/messages", base)
    } else {
        format!("{}/v1/messages", base)
    }
}

/// Best-effort extraction of model ids from a models-listing response.
///
/// Accepted shapes (OpenAI and Anthropic both use the first):
/// - `{"data": [{"id": "..."}]}`
/// - `{"models": [{"id": "..."}]}` or `{"models": ["..."]}`
/// - top-level `[{"id": "..."}]` or `["..."]`
///
/// Anything unrecognized yields an empty list — the caller still reports
/// ok=true for a 200 response.
pub fn parse_models(json: &serde_json::Value) -> Vec<String> {
    let arr = json
        .get("data")
        .and_then(|v| v.as_array())
        .or_else(|| json.get("models").and_then(|v| v.as_array()))
        .or_else(|| json.as_array());
    let Some(arr) = arr else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|item| {
            item.as_str()
                .map(String::from)
                .or_else(|| item.get("id").and_then(|v| v.as_str()).map(String::from))
        })
        .collect()
}

/// 连通性拨测核心:命令层与 doctor 体检共用。构建 models URL 并 GET,
/// anthropic 404 时回退探测 /v1/messages。永不 Err(网络/HTTP 失败都
/// 折进 `ok=false` 的结果里),只构建 client 失败才 Err。
pub async fn test_endpoint(
    base_url: &str,
    api_key: &str,
    flavor: &str,
) -> Result<ProviderTestResult, String> {
    let url = build_models_url(base_url, flavor);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

    let request = match flavor {
        "anthropic" => client
            .get(&url)
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01"),
        _ => client
            .get(&url)
            .header("Authorization", format!("Bearer {}", api_key)),
    };

    let start = Instant::now();
    let response = request.send().await;
    let latency_ms = start.elapsed().as_millis() as u64;

    let response = match response {
        Ok(r) => r,
        Err(e) => {
            let reason = if e.is_timeout() {
                "Request timed out (8s)".to_string()
            } else {
                // Strip the url from reqwest's Display output noise.
                format!("Network error: {}", e.without_url())
            };
            return Ok(ProviderTestResult::fail(latency_ms, reason));
        }
    };

    let status = response.status();
    if !status.is_success() {
        // Anthropic-compatible gateways (Aliyun Bailian, etc.) expose only
        // `POST /v1/messages`, not `GET /v1/models`. A 404 on the models probe
        // there means "no model-listing route" — NOT a wrong base URL — so fall
        // back to a reachability probe against the messages endpoint before
        // reporting failure.
        if flavor == "anthropic" && status.as_u16() == 404 {
            return Ok(probe_anthropic_messages(&client, base_url, api_key, start).await);
        }
        let error = match status.as_u16() {
            401 | 403 => "Invalid API key or insufficient permissions".to_string(),
            404 => "Endpoint not found (check Base URL)".to_string(),
            code => format!("HTTP {} from provider", code),
        };
        return Ok(ProviderTestResult::fail(latency_ms, error));
    }

    // 200: parse models best-effort; unparseable body is still a passing test.
    let models = match response.json::<serde_json::Value>().await {
        Ok(json) => parse_models(&json),
        Err(_) => Vec::new(),
    };
    Ok(ProviderTestResult {
        ok: true,
        latency_ms,
        models,
        error: None,
    })
}

#[tauri::command]
pub async fn provider_test(
    base_url: String,
    api_key: String,
    flavor: String,
) -> Result<ProviderTestResult, String> {
    test_endpoint(&base_url, &api_key, &flavor).await
}

/// Reachability fallback for Anthropic-compatible gateways without `/v1/models`.
///
/// Sends a deliberately empty `POST /v1/messages`. These gateways validate the
/// API key *before* the request body, so the status cleanly separates cases:
/// - `401`/`403` → bad key
/// - `404` → genuinely wrong base URL
/// - `400`/`422`/`2xx`/`429` → endpoint reachable and key accepted (body rejected)
///
/// Model listing isn't available on this path, so `models` is always empty.
async fn probe_anthropic_messages(
    client: &reqwest::Client,
    base_url: &str,
    api_key: &str,
    start: Instant,
) -> ProviderTestResult {
    let url = build_messages_url(base_url);
    let response = client
        .post(&url)
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&serde_json::json!({}))
        .send()
        .await;
    let latency_ms = start.elapsed().as_millis() as u64;

    let response = match response {
        Ok(r) => r,
        Err(e) => {
            let reason = if e.is_timeout() {
                "Request timed out (8s)".to_string()
            } else {
                format!("Network error: {}", e.without_url())
            };
            return ProviderTestResult::fail(latency_ms, reason);
        }
    };

    match response.status().as_u16() {
        404 => ProviderTestResult::fail(latency_ms, "Endpoint not found (check Base URL)"),
        401 | 403 => {
            ProviderTestResult::fail(latency_ms, "Invalid API key or insufficient permissions")
        }
        // Auth passed; body was (expectedly) rejected, or the request went
        // through — either way the endpoint is a live Anthropic gateway.
        code if (200..500).contains(&code) => ProviderTestResult {
            ok: true,
            latency_ms,
            models: Vec::new(),
            error: None,
        },
        code => ProviderTestResult::fail(latency_ms, format!("HTTP {} from provider", code)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ---- build_models_url ----

    #[test]
    fn openai_url_appends_models() {
        assert_eq!(
            build_models_url("https://api.openai.com/v1", "openai"),
            "https://api.openai.com/v1/models"
        );
    }

    #[test]
    fn openai_url_strips_trailing_slash() {
        assert_eq!(
            build_models_url("https://api.openai.com/v1/", "openai"),
            "https://api.openai.com/v1/models"
        );
        assert_eq!(
            build_models_url("https://api.openai.com/v1//", "openai"),
            "https://api.openai.com/v1/models"
        );
    }

    #[test]
    fn anthropic_url_inserts_v1() {
        assert_eq!(
            build_models_url("https://api.moonshot.cn/anthropic", "anthropic"),
            "https://api.moonshot.cn/anthropic/v1/models"
        );
    }

    #[test]
    fn anthropic_url_dedupes_existing_v1() {
        assert_eq!(
            build_models_url("https://api.anthropic.com/v1", "anthropic"),
            "https://api.anthropic.com/v1/models"
        );
        assert_eq!(
            build_models_url("https://api.anthropic.com/v1/", "anthropic"),
            "https://api.anthropic.com/v1/models"
        );
    }

    // ---- build_messages_url ----

    #[test]
    fn messages_url_inserts_v1_for_gateway() {
        // Aliyun Bailian: only /v1/messages exists, no /v1/models.
        assert_eq!(
            build_messages_url("https://dashscope.aliyuncs.com/apps/anthropic"),
            "https://dashscope.aliyuncs.com/apps/anthropic/v1/messages"
        );
        assert_eq!(
            build_messages_url("https://api.moonshot.cn/anthropic/"),
            "https://api.moonshot.cn/anthropic/v1/messages"
        );
    }

    #[test]
    fn messages_url_dedupes_existing_v1() {
        assert_eq!(
            build_messages_url("https://api.anthropic.com/v1"),
            "https://api.anthropic.com/v1/messages"
        );
        assert_eq!(
            build_messages_url("https://api.anthropic.com/v1/"),
            "https://api.anthropic.com/v1/messages"
        );
    }

    // ---- parse_models ----

    #[test]
    fn parses_openai_style_data_array() {
        let json = json!({"object":"list","data":[{"id":"gpt-4o"},{"id":"gpt-4o-mini"}]});
        assert_eq!(parse_models(&json), vec!["gpt-4o", "gpt-4o-mini"]);
    }

    #[test]
    fn parses_anthropic_style_data_array() {
        let json = json!({"data":[{"id":"claude-fable-5","type":"model"}],"has_more":false});
        assert_eq!(parse_models(&json), vec!["claude-fable-5"]);
    }

    #[test]
    fn parses_top_level_array_and_models_key() {
        assert_eq!(
            parse_models(&json!([{"id":"m1"},{"id":"m2"}])),
            vec!["m1", "m2"]
        );
        assert_eq!(parse_models(&json!({"models":["m1","m2"]})), vec!["m1", "m2"]);
        assert_eq!(parse_models(&json!({"models":[{"id":"m1"}]})), vec!["m1"]);
    }

    #[test]
    fn unparseable_shapes_yield_empty_list() {
        assert_eq!(parse_models(&json!({"foo":"bar"})), Vec::<String>::new());
        assert_eq!(parse_models(&json!("just a string")), Vec::<String>::new());
        // Entries without a usable id are skipped, not errors.
        assert_eq!(
            parse_models(&json!({"data":[{"name":"no-id"},{"id":"ok"}]})),
            vec!["ok"]
        );
    }
}
