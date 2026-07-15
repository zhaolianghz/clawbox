//! stdio JSON-RPC transport for ACP agent bridges.
//!
//! Spawns the bridge as a child process and multiplexes over stdin/stdout:
//! outbound requests get a numeric id and await a matching response; inbound
//! frames from the agent are classified into responses (to our requests),
//! notifications (session/update), or requests (session/request_permission).

use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio::time::{timeout, Duration};

pub enum Frame {
    Response { id: Value, body: Value },
    Notification { method: String, params: Value },
    Request { id: Value, method: String, params: Value },
}

pub fn classify(v: &Value) -> Frame {
    let has_id = v.get("id").map(|i| !i.is_null()).unwrap_or(false);
    let method = v.get("method").and_then(|m| m.as_str());
    match (has_id, method) {
        (true, Some(m)) => Frame::Request {
            id: v.get("id").cloned().unwrap_or(Value::Null),
            method: m.to_string(),
            params: v.get("params").cloned().unwrap_or(json!({})),
        },
        (true, None) => Frame::Response {
            id: v.get("id").cloned().unwrap_or(Value::Null),
            body: v.clone(),
        },
        (false, Some(m)) => Frame::Notification {
            method: m.to_string(),
            params: v.get("params").cloned().unwrap_or(json!({})),
        },
        (false, None) => Frame::Notification { method: String::new(), params: json!({}) },
    }
}

pub enum Inbound {
    Notification { method: String, params: Value },
    Request { id: Value, method: String, params: Value },
}

type Pending = Arc<Mutex<HashMap<i64, oneshot::Sender<Value>>>>;

pub struct JsonRpcClient {
    stdin: Arc<Mutex<ChildStdin>>,
    next_id: AtomicI64,
    pending: Pending,
    _child: Arc<Mutex<Child>>,
}

impl JsonRpcClient {
    pub async fn spawn(
        binary: &str,
        args: &[&str],
        cwd: &Path,
    ) -> Result<(Self, mpsc::Receiver<Inbound>), String> {
        let mut child = Command::new(binary)
            .args(args)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true) // reap the bridge when the client is dropped (no zombie processes across review runs)
            .spawn()
            .map_err(|e| format!("failed to spawn {}: {}", binary, e))?;

        let stdin = child.stdin.take().ok_or("no stdin")?;
        let stdout = child.stdout.take().ok_or("no stdout")?;
        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let (tx, rx) = mpsc::channel::<Inbound>(256);

        let pending_reader = pending.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if line.trim().is_empty() {
                    continue;
                }
                let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
                match classify(&v) {
                    Frame::Response { id, body } => {
                        if let Some(n) = id.as_i64() {
                            if let Some(sender) = pending_reader.lock().await.remove(&n) {
                                let _ = sender.send(body);
                            }
                        }
                    }
                    Frame::Notification { method, params } => {
                        let _ = tx.send(Inbound::Notification { method, params }).await;
                    }
                    Frame::Request { id, method, params } => {
                        let _ = tx.send(Inbound::Request { id, method, params }).await;
                    }
                }
            }
        });

        Ok((
            Self {
                stdin: Arc::new(Mutex::new(stdin)),
                next_id: AtomicI64::new(1),
                pending,
                _child: Arc::new(Mutex::new(child)),
            },
            rx,
        ))
    }

    pub async fn request(&self, method: &str, params: Value) -> Result<Value, String> {
        self.request_with_timeout(method, params, 60).await
    }

    pub async fn request_with_timeout(
        &self,
        method: &str,
        params: Value,
        secs: u64,
    ) -> Result<Value, String> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);

        let msg = json!({"jsonrpc":"2.0","id":id,"method":method,"params":params});
        self.write(&msg).await?;

        match timeout(Duration::from_secs(secs), rx).await {
            Ok(Ok(body)) => {
                if let Some(err) = body.get("error") {
                    Err(format!("rpc error: {}", err))
                } else {
                    Ok(body.get("result").cloned().unwrap_or(Value::Null))
                }
            }
            Ok(Err(_)) => Err("response channel closed".into()),
            Err(_) => {
                self.pending.lock().await.remove(&id);
                Err(format!("request {} timed out after {}s", method, secs))
            }
        }
    }

    pub async fn respond(&self, id: Value, result: Value) -> Result<(), String> {
        let msg = json!({"jsonrpc":"2.0","id":id,"result":result});
        self.write(&msg).await
    }

    pub async fn notify(&self, method: &str, params: Value) -> Result<(), String> {
        let msg = json!({"jsonrpc":"2.0","method":method,"params":params});
        self.write(&msg).await
    }

    async fn write(&self, msg: &Value) -> Result<(), String> {
        let mut line = serde_json::to_string(msg).map_err(|e| e.to_string())?;
        line.push('\n');
        let mut w = self.stdin.lock().await;
        w.write_all(line.as_bytes()).await.map_err(|e| e.to_string())?;
        w.flush().await.map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_response() {
        let v: serde_json::Value = serde_json::from_str(
            r#"{"jsonrpc":"2.0","id":1,"result":{"ok":true}}"#).unwrap();
        assert!(matches!(classify(&v), Frame::Response { .. }));
    }

    #[test]
    fn classify_notification() {
        let v: serde_json::Value = serde_json::from_str(
            r#"{"jsonrpc":"2.0","method":"session/update","params":{}}"#).unwrap();
        assert!(matches!(classify(&v), Frame::Notification { .. }));
    }

    #[test]
    fn classify_inbound_request() {
        let v: serde_json::Value = serde_json::from_str(
            r#"{"jsonrpc":"2.0","id":5,"method":"session/request_permission","params":{}}"#).unwrap();
        assert!(matches!(classify(&v), Frame::Request { .. }));
    }

    #[test]
    fn classify_error() {
        let v: serde_json::Value = serde_json::from_str(
            r#"{"jsonrpc":"2.0","id":1,"error":{"code":-1,"message":"boom"}}"#).unwrap();
        assert!(matches!(classify(&v), Frame::Response { .. }));
    }
}
