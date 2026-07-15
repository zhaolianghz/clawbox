//! ACP session lifecycle: spawn bridge -> initialize -> session/new -> prompt.

use crate::acp::adapters::find_adapter;
use crate::acp::jsonrpc::{Inbound, JsonRpcClient};
use crate::acp::permission::{decide, PermDecision, PermOption, PermissionPolicy};
use serde_json::{json, Value};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tokio::time::{timeout, Duration};

pub struct AcpSession {
    client: Arc<JsonRpcClient>,
    session_id: String,
    inbound: Mutex<mpsc::Receiver<Inbound>>,
    policy: PermissionPolicy,
}

pub struct PromptResult {
    pub stop_reason: String,
    pub text: String,
}

/// Pull assistant text out of a `session/update` payload; None for non-text
/// updates. Thought chunks are deliberately dropped: reasoning text pollutes
/// summaries and a fenced block inside it can hijack findings JSON parsing.
pub fn extract_text(update: &Value) -> Option<String> {
    let kind = update.get("sessionUpdate").and_then(|v| v.as_str())?;
    if kind != "agent_message_chunk" {
        return None;
    }
    update
        .get("content")
        .and_then(|c| c.get("text"))
        .and_then(|t| t.as_str())
        .map(|s| s.to_string())
}

fn parse_perm_options(params: &Value) -> Vec<PermOption> {
    params
        .get("options")
        .and_then(|o| o.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|o| {
                    Some(PermOption {
                        option_id: o.get("optionId")?.as_str()?.to_string(),
                        kind: o.get("kind")?.as_str()?.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Extract the structured `toolCall.kind` and display `toolCall.title` from a
/// `session/request_permission` payload (empty strings when absent). `kind` is
/// the reliable signal (`read|edit|delete|move|search|execute|think|fetch|other`);
/// `title` is a human display string — for shell tools it is the raw command
/// line — and is only used as a secondary deny layer / fallback.
fn perm_tool_fields(params: &Value) -> (String, String) {
    let tc = params.get("toolCall");
    let get = |field: &str| {
        tc.and_then(|t| t.get(field))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    };
    (get("kind"), get("title"))
}

impl AcpSession {
    pub async fn start(
        adapter_id: &str,
        cwd: &Path,
        policy: PermissionPolicy,
    ) -> Result<AcpSession, String> {
        let adapter =
            find_adapter(adapter_id).ok_or_else(|| format!("unknown adapter: {}", adapter_id))?;
        let (client, inbound) = JsonRpcClient::spawn(adapter.binary, &[], cwd).await?;
        let client = Arc::new(client);

        // initialize (cold start can take ~40s on first launch)
        client
            .request_with_timeout(
                "initialize",
                json!({
                    "protocolVersion": 1,
                    "clientCapabilities": { "fs": { "readTextFile": false, "writeTextFile": false } }, // don't advertise fs reads — we have no fs/read_text_file handler; agents read via their own process
                    "clientInfo": { "name": "clawbox", "version": env!("CARGO_PKG_VERSION") }
                }),
                90,
            )
            .await?;

        let sess = client
            .request_with_timeout(
                "session/new",
                json!({ "cwd": cwd.to_string_lossy(), "mcpServers": [] }),
                90,
            )
            .await?;
        let session_id = sess
            .get("sessionId")
            .and_then(|v| v.as_str())
            .ok_or("session/new returned no sessionId")?
            .to_string();

        Ok(AcpSession {
            client,
            session_id,
            inbound: Mutex::new(inbound),
            policy,
        })
    }

    /// Send one prompt turn. Answers permission requests per policy while
    /// draining session/update text until the turn's response arrives.
    pub async fn prompt(&self, text: &str) -> Result<PromptResult, String> {
        // Fire the prompt request as a background task so we can service
        // inbound frames (updates + permission requests) that the agent
        // raises mid-turn. `tokio::select!` over `rx.recv()` and the spawned
        // JoinHandle fights the borrow checker (the handle must survive the
        // loop to be awaited afterwards), so we poll: `is_finished()` at the
        // top of the loop, and a short timeout around `recv()` so the finish
        // check runs at least every 200ms.
        let prompt_params = json!({
            "sessionId": self.session_id,
            "prompt": [ { "type": "text", "text": text } ]
        });
        let prompt_fut = {
            let client = self.client.clone();
            tokio::spawn(async move {
                client
                    .request_with_timeout("session/prompt", prompt_params, 600)
                    .await
            })
        };

        let mut collected = String::new();
        let mut rx = self.inbound.lock().await;

        loop {
            if prompt_fut.is_finished() {
                // The turn's response frame is written after any preceding
                // update frames, but our recv loop may not have consumed them
                // yet: drain whatever is already buffered without blocking.
                while let Ok(msg) = rx.try_recv() {
                    self.handle_inbound(msg, &mut collected).await;
                }
                break;
            }
            match timeout(Duration::from_millis(200), rx.recv()).await {
                Ok(Some(msg)) => self.handle_inbound(msg, &mut collected).await,
                Ok(None) => {
                    // Reader task ended: the agent process closed stdout. The
                    // pending oneshot would only resolve via the 600s timeout,
                    // so fail fast instead of hanging.
                    if prompt_fut.is_finished() {
                        break;
                    }
                    prompt_fut.abort();
                    return Err("agent process closed the connection mid-prompt".into());
                }
                Err(_) => {} // recv timeout: loop back and re-check is_finished
            }
        }

        let stop_reason = match prompt_fut.await {
            Ok(Ok(body)) => body
                .get("stopReason")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string(),
            Ok(Err(e)) => return Err(e),
            Err(e) => return Err(format!("prompt task join error: {}", e)),
        };

        Ok(PromptResult { stop_reason, text: collected })
    }

    /// Handle one inbound frame during a prompt turn: accumulate assistant
    /// text from `session/update`, answer `session/request_permission` per
    /// the session policy, ignore everything else.
    async fn handle_inbound(&self, msg: Inbound, collected: &mut String) {
        match msg {
            Inbound::Notification { method, params } => {
                if method == "session/update" {
                    if let Some(u) = params.get("update") {
                        if let Some(t) = extract_text(u) {
                            collected.push_str(&t);
                        }
                    }
                }
            }
            Inbound::Request { id, method, params } => {
                if method == "session/request_permission" {
                    let opts = parse_perm_options(&params);
                    let (kind, title) = perm_tool_fields(&params);
                    let decision = decide(self.policy, &kind, &title, &opts);
                    let result = match decision {
                        PermDecision::Select(opt) => json!({
                            "outcome": { "outcome": "selected", "optionId": opt }
                        }),
                        PermDecision::RejectAll => json!({
                            "outcome": { "outcome": "cancelled" }
                        }),
                    };
                    let _ = self.client.respond(id, result).await;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extract_agent_message_text() {
        let update = json!({
            "sessionUpdate": "agent_message_chunk",
            "content": { "type": "text", "text": "hello" }
        });
        assert_eq!(extract_text(&update), Some("hello".to_string()));
    }

    #[test]
    fn ignores_non_text_updates() {
        let update = json!({ "sessionUpdate": "tool_call", "toolCallId": "x" });
        assert_eq!(extract_text(&update), None);
    }

    #[test]
    fn drops_agent_thought_chunks() {
        // Reasoning text must not leak into summaries or findings parsing.
        let update = json!({
            "sessionUpdate": "agent_thought_chunk",
            "content": { "type": "text", "text": "```json\n[]\n```" }
        });
        assert_eq!(extract_text(&update), None);
    }

    #[test]
    fn perm_tool_fields_extracts_kind_and_title() {
        let params = json!({
            "toolCall": { "kind": "execute", "title": "cat a.txt | tee b.txt" }
        });
        let (kind, title) = perm_tool_fields(&params);
        assert_eq!(kind, "execute");
        assert_eq!(title, "cat a.txt | tee b.txt");
    }

    #[test]
    fn perm_tool_fields_defaults_to_empty() {
        let (kind, title) = perm_tool_fields(&json!({}));
        assert_eq!(kind, "");
        assert_eq!(title, "");
    }
}
