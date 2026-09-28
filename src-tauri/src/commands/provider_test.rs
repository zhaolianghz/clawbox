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

/// 从 anthropic 槽的 base 推出**同 host** 的 OpenAI 约定列模型 URL。
///
/// Anthropic 协议本身没有列模型接口；官方有 `GET /v1/models`，但第三方
/// anthropic 网关普遍不实现（实测 MiMo：`/anthropic` 下所有 models 路径全
/// 404）。而很多网关把两种协议挂在**同一个 host** 上，那里按 OpenAI 约定
/// 就有列表 —— 所以先扔末尾的 `anthropic` 段（常见布局 `{root}/anthropic`
/// + `{root}/v1`），保留其它前缀，再拼 `/v1/models`。
///
/// 例：`https://h/anthropic` → `https://h/v1/models`；
///     `https://h/gw/anthropic` → `https://h/gw/v1/models`。
fn host_models_url(base_url: &str) -> Option<String> {
    let base = base_url.trim().trim_end_matches('/');
    if base.is_empty() {
        return None;
    }
    let root = base.strip_suffix("/anthropic").unwrap_or(base).trim_end_matches('/');
    if root.is_empty() {
        return None;
    }
    Some(if root.ends_with("/v1") {
        format!("{}/models", root)
    } else {
        format!("{}/v1/models", root)
    })
}

/// 用 OpenAI 约定去列模型（anthropic 槽的同 host 回退）。
///
/// 认证头两个都带：网关可能认 OpenAI 的 `Bearer`，也可能只认 `x-api-key`，
/// 凭据本是同一把。返回 `None` = 这次尝试不可用（网络失败 / 非 2xx / 2xx 但
/// 列表为空），调用方据此继续降级到 messages 探针。
async fn probe_host_models(
    client: &reqwest::Client,
    models_url: &str,
    api_key: &str,
    start: Instant,
) -> Option<ProviderTestResult> {
    let response = client
        .get(models_url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("x-api-key", api_key)
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    let models = match response.json::<serde_json::Value>().await {
        Ok(json) => parse_models(&json),
        Err(_) => Vec::new(),
    };
    if models.is_empty() {
        // 2xx 但空列表：交给 messages 探针去判可达性，别把「连得上但无列表」
        // 说成「列模型成功但空」——两者对用户的含义不同。
        return None;
    }
    Some(ProviderTestResult {
        ok: true,
        latency_ms: start.elapsed().as_millis() as u64,
        models,
        error: None,
    })
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
            // 该 anthropic 前缀下没有列模型路由。先试同 host 的 OpenAI 约定
            // `/v1/models`（双协议共用一个 host 的网关在那里有列表）；与刚
            // 才试过的 URL 相同时不必重试。都不行才降级到 messages 可达性
            // 探针 —— 那条路径的 models 恒为空。
            if let Some(host_url) = host_models_url(base_url) {
                if host_url != url {
                    if let Some(r) = probe_host_models(&client, &host_url, api_key, start).await {
                        return Ok(r);
                    }
                }
            }
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

    // ---- host_models_url ----

    #[test]
    fn host_models_url_strips_anthropic_segment() {
        // 真实案例:小米 MiMo 的两种协议同 host,anthropic 前缀下无列模型路由。
        assert_eq!(
            host_models_url("https://token-plan-cn.xiaomimimo.com/anthropic").as_deref(),
            Some("https://token-plan-cn.xiaomimimo.com/v1/models")
        );
        // 尾斜杠照旧
        assert_eq!(
            host_models_url("https://h.example/anthropic/").as_deref(),
            Some("https://h.example/v1/models")
        );
    }

    #[test]
    fn host_models_url_keeps_other_path_prefixes() {
        // 网关挂在子路径下时不能把前缀扔掉 —— 只扔末尾那个 anthropic 段。
        assert_eq!(
            host_models_url("https://h.example/gw/anthropic").as_deref(),
            Some("https://h.example/gw/v1/models")
        );
        // 槽自带 /v1 时按去重规则拼(不产生 /v1/v1)
        assert_eq!(
            host_models_url("https://h.example/v1/anthropic").as_deref(),
            Some("https://h.example/v1/models")
        );
    }

    #[test]
    fn host_models_url_degrades_to_same_url_without_anthropic_suffix() {
        // 没有 anthropic 段(如官方端点)时推出的是**同一个** URL —— 调用方靠
        // `host_url != url` 跳过重试,不会白打第二次请求。
        let base = "https://api.anthropic.com";
        assert_eq!(
            host_models_url(base).as_deref(),
            Some(build_models_url(base, "anthropic").as_str())
        );
        assert_eq!(host_models_url("").as_deref(), None);
        assert_eq!(host_models_url("/").as_deref(), None);
        assert_eq!(host_models_url("/anthropic").as_deref(), None);
    }
}
