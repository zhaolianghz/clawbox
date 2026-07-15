# ACP Agent 接入 + 代码审核工作流 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 ClawBox 接入 ACP 兼容 agent（ClaudeCode/Codex），并在其上实现只读的多 agent 代码审核功能。

**Architecture:** ACP 作为独立子系统 `src-tauri/src/acp/`，与现有 `Backend` trait 完全解耦。传输层用轻量 tokio + serde JSON-RPC over stdio（spike 已证明可行），协议类型复用官方 `agent-client-protocol-schema` crate。审核引擎在 ACP 会话之上编排 reviewer + summarizer 角色，只读策略在协议的 `request_permission` 层硬保证。前端把无后端的 agents 占位页改造成 Review 页。

**Tech Stack:** Rust / Tauri v2 / tokio / serde_json / agent-client-protocol-schema；前端 Svelte 5 + TypeScript。

## Global Constraints

- 后端语言 Rust（edition 2021），前端 Svelte 5 + TypeScript（见 `project.md`）。
- 不改动现有 `Backend` trait、OpenClaw/Hermes 后端、capabilities 体系。
- 测试沿用仓库惯例：解析/策略逻辑用单元测试（`#[cfg(test)]` 内联模块，fixture 驱动，TDD）；live 集成测试放 `tests/smoke.rs`，用 `is_installed` 探测 gate（本机无 `claude-agent-acp` 时跳过，绝不失败）。
- 新增 Tauri 命令必须注册进 `src-tauri/src/lib.rs` 的 `generate_handler!`。
- ACP 协议版本：`protocolVersion = 1`（spike 实测握手返回值）。
- 审核角色权限：v1 全部 ReadOnly——写类工具调用回 `RejectOnce`。
- 数据落盘位置沿用现有惯例 `~/.clawbox/`（见 `commands/config.rs`、`commands/feedback.rs`）：审核报告存 `~/.clawbox/reviews/<task_id>.json`。
- 提交粒度：每个 Task 末尾 commit。commit message 用现有风格（`feat:`/`test:`/`fix:` 前缀）。
- 冷启动：首次 spawn `claude-agent-acp` 约 40s，所有 live 测试和 session 建立的超时 ≥ 60s。

---

## 文件结构

**新建（后端）：**
- `src-tauri/src/acp/mod.rs` — 子系统入口，导出 adapters/session/permission/review，定义共享类型
- `src-tauri/src/acp/adapters.rs` — 适配器注册表 + 检测/安装
- `src-tauri/src/acp/jsonrpc.rs` — stdio JSON-RPC 传输（spawn 子进程、请求/响应/通知路由）
- `src-tauri/src/acp/session.rs` — `AcpSessionManager`：initialize/session/prompt 生命周期
- `src-tauri/src/acp/permission.rs` — 权限策略（ReadOnly / AskUser）
- `src-tauri/src/acp/review.rs` — 审核引擎：ReviewTask/角色编排/报告生成落盘
- `src-tauri/src/commands/acp.rs` — Tauri 命令层（薄封装，调用 acp 模块）

**修改（后端）：**
- `src-tauri/Cargo.toml` — 加依赖 tokio、agent-client-protocol-schema
- `src-tauri/src/lib.rs` — 注册新命令 + `mod acp`
- `src-tauri/src/commands/mod.rs` — 加 `pub mod acp;`
- `src-tauri/tests/smoke.rs` — 加 ACP live smoke 测试

**新建/修改（前端）：**
- `src/lib/api/acp.ts` — ACP/审核命令的 TS 封装
- `src/routes/agents/+page.svelte` — 改造为 Review 页
- `src/lib/i18n/en.json`、`zh.json` — 审核相关文案

---

## Milestone M1：ACP 会话层

### Task 1：加依赖 + acp 模块骨架

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Create: `src-tauri/src/acp/mod.rs`
- Modify: `src-tauri/src/lib.rs:1`（加 `pub mod acp;`）

**Interfaces:**
- Produces: `mod acp` 可被编译引用；`acp::AdapterId` 类型别名（`String`）。

- [ ] **Step 1：加依赖**

修改 `src-tauri/Cargo.toml` 的 `[dependencies]`，追加：

```toml
tokio = { version = "1", features = ["rt-multi-thread", "process", "io-util", "sync", "macros", "time"] }
agent-client-protocol-schema = "1.4"
```

- [ ] **Step 2：建 acp 模块入口**

创建 `src-tauri/src/acp/mod.rs`：

```rust
//! ACP (Agent Client Protocol) subsystem — spawns ACP-compatible agent
//! bridges over stdio and drives sessions. Independent from the `Backend`
//! trait (which manages long-running gateway/cron runtimes).

pub mod adapters;
pub mod jsonrpc;
pub mod permission;
pub mod review;
pub mod session;

/// Adapter identifier, e.g. "claude-agent-acp".
pub type AdapterId = String;
```

- [ ] **Step 3：声明 mod**

修改 `src-tauri/src/lib.rs`，在第 1 行 `pub mod backends;` 后加：

```rust
pub mod acp;
```

（此时 adapters/jsonrpc/... 还不存在，先建空占位文件避免编译失败）创建以下空文件，各写一行注释：
- `src-tauri/src/acp/adapters.rs` → `//! Adapter registry.`
- `src-tauri/src/acp/jsonrpc.rs` → `//! stdio JSON-RPC transport.`
- `src-tauri/src/acp/permission.rs` → `//! Permission policy.`
- `src-tauri/src/acp/review.rs` → `//! Review engine.`
- `src-tauri/src/acp/session.rs` → `//! Session manager.`

- [ ] **Step 4：编译验证**

Run: `cd src-tauri && cargo check`
Expected: 编译通过（可能有 unused warning，允许）。

- [ ] **Step 5：Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/acp/ src-tauri/src/lib.rs
git commit -m "feat(acp): add tokio/acp-schema deps + acp module skeleton"
```

---

### Task 2：适配器注册表

**Files:**
- Modify: `src-tauri/src/acp/adapters.rs`

**Interfaces:**
- Produces:
  - `struct AcpAdapter { pub id: &'static str, pub label: &'static str, pub binary: &'static str, pub install_hint: &'static str, pub check_probe: &'static [&'static str] }`
  - `fn adapters() -> &'static [AcpAdapter]`
  - `fn find_adapter(id: &str) -> Option<&'static AcpAdapter>`
  - `impl AcpAdapter { fn is_installed(&self) -> bool; fn version(&self) -> Option<String> }`
  - `#[derive(Serialize)] struct AdapterInfo { id, label, installed: bool, version: Option<String>, install_hint }`
  - `fn list_adapter_info() -> Vec<AdapterInfo>`

- [ ] **Step 1：写失败测试**

在 `src-tauri/src/acp/adapters.rs` 末尾：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_claude_and_codex() {
        let ids: Vec<_> = adapters().iter().map(|a| a.id).collect();
        assert!(ids.contains(&"claude-agent-acp"));
        assert!(ids.contains(&"codex-acp"));
    }

    #[test]
    fn find_adapter_works() {
        assert!(find_adapter("claude-agent-acp").is_some());
        assert!(find_adapter("nonexistent").is_none());
    }

    #[test]
    fn list_info_covers_all_adapters() {
        assert_eq!(list_adapter_info().len(), adapters().len());
    }
}
```

- [ ] **Step 2：运行测试确认失败**

Run: `cd src-tauri && cargo test --lib acp::adapters`
Expected: FAIL（`adapters` 等未定义）。

- [ ] **Step 3：实现**

替换 `src-tauri/src/acp/adapters.rs` 顶部占位注释为完整实现：

```rust
//! Adapter registry — ACP-compatible agent bridges installable as CLIs.

use serde::Serialize;
use std::process::Command;

pub struct AcpAdapter {
    pub id: &'static str,
    pub label: &'static str,
    pub binary: &'static str,
    pub install_hint: &'static str,
    pub check_probe: &'static [&'static str],
}

impl AcpAdapter {
    pub fn is_installed(&self) -> bool {
        self.version().is_some()
    }

    pub fn version(&self) -> Option<String> {
        let out = Command::new(self.binary).args(self.check_probe).output().ok()?;
        if !out.status.success() {
            return None;
        }
        let s = String::from_utf8_lossy(&out.stdout).trim().lines().next()?.to_string();
        if s.is_empty() { None } else { Some(s) }
    }
}

static ADAPTERS: &[AcpAdapter] = &[
    AcpAdapter {
        id: "claude-agent-acp",
        label: "ClaudeCode",
        binary: "claude-agent-acp",
        install_hint: "npm install -g @agentclientprotocol/claude-agent-acp",
        check_probe: &["--version"],
    },
    AcpAdapter {
        id: "codex-acp",
        label: "Codex",
        binary: "codex-acp",
        install_hint: "npm install -g --force @agentclientprotocol/codex-acp",
        check_probe: &["--version"],
    },
];

pub fn adapters() -> &'static [AcpAdapter] {
    ADAPTERS
}

pub fn find_adapter(id: &str) -> Option<&'static AcpAdapter> {
    ADAPTERS.iter().find(|a| a.id == id)
}

#[derive(Serialize)]
pub struct AdapterInfo {
    pub id: String,
    pub label: String,
    pub installed: bool,
    pub version: Option<String>,
    pub install_hint: String,
}

pub fn list_adapter_info() -> Vec<AdapterInfo> {
    ADAPTERS
        .iter()
        .map(|a| {
            let version = a.version();
            AdapterInfo {
                id: a.id.to_string(),
                label: a.label.to_string(),
                installed: version.is_some(),
                version,
                install_hint: a.install_hint.to_string(),
            }
        })
        .collect()
}
```

- [ ] **Step 4：运行测试确认通过**

Run: `cd src-tauri && cargo test --lib acp::adapters`
Expected: PASS（3 tests）。

- [ ] **Step 5：Commit**

```bash
git add src-tauri/src/acp/adapters.rs
git commit -m "feat(acp): adapter registry with claude/codex + install detection"
```

---

### Task 3：stdio JSON-RPC 传输层

**Files:**
- Modify: `src-tauri/src/acp/jsonrpc.rs`

**Interfaces:**
- Produces:
  - `struct JsonRpcClient`（持有子进程 stdin writer + 请求 id 计数 + pending map）
  - `async fn JsonRpcClient::spawn(binary: &str, args: &[&str], cwd: &Path) -> Result<(JsonRpcClient, mpsc::Receiver<Notification>), String>`
  - `async fn JsonRpcClient::request(&self, method: &str, params: serde_json::Value) -> Result<serde_json::Value, String>`（默认 60s 超时）
  - `fn JsonRpcClient::request_with_timeout(&self, method, params, secs) -> ...`
  - `struct Notification { pub method: String, pub params: serde_json::Value }`
  - `async fn JsonRpcClient::respond(&self, id: serde_json::Value, result: serde_json::Value)`（回应 agent 发来的 request，如 permission）
  - pending inbound requests 通过同一 `mpsc::Receiver<Notification>` 传递（method 非空且带 id 的即为 request）→ 用 `enum Inbound { Notification(Notification), Request { id, method, params } }`

- [ ] **Step 1：写失败测试**

JSON-RPC 帧解析是纯逻辑，可脱离子进程测。在 `jsonrpc.rs` 末尾：

```rust
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
```

- [ ] **Step 2：运行测试确认失败**

Run: `cd src-tauri && cargo test --lib acp::jsonrpc`
Expected: FAIL（`classify`/`Frame` 未定义）。

- [ ] **Step 3：实现**

替换 `jsonrpc.rs` 内容：

```rust
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
```

- [ ] **Step 4：运行测试确认通过**

Run: `cd src-tauri && cargo test --lib acp::jsonrpc`
Expected: PASS（4 tests）。

- [ ] **Step 5：Commit**

```bash
git add src-tauri/src/acp/jsonrpc.rs
git commit -m "feat(acp): stdio JSON-RPC transport (spawn + request/notify/respond routing)"
```

---

### Task 4：权限策略

**Files:**
- Modify: `src-tauri/src/acp/permission.rs`

**Interfaces:**
- Consumes: `PermissionOptionKind`（来自 `agent_client_protocol_schema`）。
- Produces:
  - `#[derive(Clone, Copy)] enum PermissionPolicy { ReadOnly, AskUser }`
  - `fn decide(policy: PermissionPolicy, tool_name: &str, options: &[PermOption]) -> PermDecision`
  - `struct PermOption { pub option_id: String, pub kind: String }`（从 request params 解析出的精简形态）
  - `enum PermDecision { Select(String) /* optionId */, RejectAll }`
  - `fn is_write_tool(tool_name: &str) -> bool`

- [ ] **Step 1：写失败测试**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> Vec<PermOption> {
        vec![
            PermOption { option_id: "allow".into(), kind: "allow_once".into() },
            PermOption { option_id: "reject".into(), kind: "reject_once".into() },
        ]
    }

    #[test]
    fn readonly_rejects_write_tools() {
        let d = decide(PermissionPolicy::ReadOnly, "write_file", &opts());
        assert!(matches!(d, PermDecision::RejectAll));
    }

    #[test]
    fn readonly_allows_read_tools() {
        let d = decide(PermissionPolicy::ReadOnly, "read_file", &opts());
        assert!(matches!(d, PermDecision::Select(ref id) if id == "allow"));
    }

    #[test]
    fn is_write_tool_detection() {
        assert!(is_write_tool("write_file"));
        assert!(is_write_tool("edit"));
        assert!(is_write_tool("apply_patch"));
        assert!(!is_write_tool("read_file"));
        assert!(!is_write_tool("grep"));
    }
}
```

- [ ] **Step 2：运行测试确认失败**

Run: `cd src-tauri && cargo test --lib acp::permission`
Expected: FAIL。

- [ ] **Step 3：实现**

替换 `permission.rs`：

```rust
//! Permission policy — decides how to answer `session/request_permission`.
//!
//! ReadOnly is the policy used for all v1 review roles: any tool that could
//! mutate the workspace is rejected at the protocol layer, so a reviewer
//! literally cannot write, regardless of its prompt.

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PermissionPolicy {
    ReadOnly,
    AskUser,
}

pub struct PermOption {
    pub option_id: String,
    pub kind: String, // "allow_once" | "allow_always" | "reject_once" | "reject_always"
}

#[derive(Debug, PartialEq)]
pub enum PermDecision {
    Select(String),
    RejectAll,
}

/// Tool names that mutate the workspace. ACP tool names are not fully
/// standardized across agents, so match on common substrings.
pub fn is_write_tool(tool_name: &str) -> bool {
    let t = tool_name.to_lowercase();
    const WRITE_MARKERS: &[&str] = &[
        "write", "edit", "create", "delete", "remove", "apply_patch",
        "patch", "move", "rename", "mkdir", "chmod",
    ];
    WRITE_MARKERS.iter().any(|m| t.contains(m))
}

pub fn decide(policy: PermissionPolicy, tool_name: &str, options: &[PermOption]) -> PermDecision {
    match policy {
        PermissionPolicy::ReadOnly => {
            if is_write_tool(tool_name) {
                PermDecision::RejectAll
            } else {
                // Prefer an allow_once option; fall back to any allow.
                options
                    .iter()
                    .find(|o| o.kind == "allow_once")
                    .or_else(|| options.iter().find(|o| o.kind.starts_with("allow")))
                    .map(|o| PermDecision::Select(o.option_id.clone()))
                    .unwrap_or(PermDecision::RejectAll)
            }
        }
        PermissionPolicy::AskUser => {
            // v1: no interactive review path uses AskUser; default deny for safety.
            // Real UI wiring comes with the chat scenario (out of scope here).
            PermDecision::RejectAll
        }
    }
}
```

- [ ] **Step 4：运行测试确认通过**

Run: `cd src-tauri && cargo test --lib acp::permission`
Expected: PASS（3 tests）。

- [ ] **Step 5：Commit**

```bash
git add src-tauri/src/acp/permission.rs
git commit -m "feat(acp): read-only permission policy (reject write tools at protocol layer)"
```

---

### Task 5：会话管理器

**Files:**
- Modify: `src-tauri/src/acp/session.rs`

**Interfaces:**
- Consumes: `JsonRpcClient`、`Inbound`（Task 3）；`PermissionPolicy`、`decide`、`PermOption`（Task 4）；`find_adapter`（Task 2）。
- Produces:
  - `struct AcpSession { client: JsonRpcClient, session_id: String }`
  - `async fn AcpSession::start(adapter_id: &str, cwd: &Path, policy: PermissionPolicy) -> Result<AcpSession, String>`（spawn → initialize → session/new，并起后台任务处理 inbound permission 请求）
  - `async fn AcpSession::prompt(&self, text: &str) -> Result<PromptResult, String>`
  - `struct PromptResult { pub stop_reason: String, pub text: String }`（text 为累积的 assistant 文本块）
  - inbound `session/update` 的 assistant 文本累积逻辑：`fn extract_text(update: &Value) -> Option<String>`（可单测）

- [ ] **Step 1：写失败测试（纯解析部分）**

`session/update` 的文本抽取是纯逻辑，可脱离进程测。在 `session.rs` 末尾：

```rust
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
}
```

- [ ] **Step 2：运行测试确认失败**

Run: `cd src-tauri && cargo test --lib acp::session`
Expected: FAIL。

- [ ] **Step 3：实现**

替换 `session.rs`：

```rust
//! ACP session lifecycle: spawn bridge -> initialize -> session/new -> prompt.

use crate::acp::adapters::find_adapter;
use crate::acp::jsonrpc::{Inbound, JsonRpcClient};
use crate::acp::permission::{decide, PermDecision, PermOption, PermissionPolicy};
use serde_json::{json, Value};
use std::path::Path;

pub struct AcpSession {
    client: JsonRpcClient,
    session_id: String,
}

pub struct PromptResult {
    pub stop_reason: String,
    pub text: String,
}

/// Pull assistant text out of a `session/update` payload; None for non-text updates.
pub fn extract_text(update: &Value) -> Option<String> {
    let kind = update.get("sessionUpdate").and_then(|v| v.as_str())?;
    if kind != "agent_message_chunk" && kind != "agent_thought_chunk" {
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

fn tool_name_from_perm(params: &Value) -> String {
    params
        .get("toolCall")
        .and_then(|tc| tc.get("title").or_else(|| tc.get("kind")))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

impl AcpSession {
    pub async fn start(
        adapter_id: &str,
        cwd: &Path,
        policy: PermissionPolicy,
    ) -> Result<AcpSession, String> {
        let adapter = find_adapter(adapter_id)
            .ok_or_else(|| format!("unknown adapter: {}", adapter_id))?;

        let (client, mut inbound) = JsonRpcClient::spawn(adapter.binary, &[], cwd).await?;

        // initialize (cold start can take ~40s on first launch)
        client
            .request_with_timeout(
                "initialize",
                json!({
                    "protocolVersion": 1,
                    "clientCapabilities": { "fs": { "readTextFile": true, "writeTextFile": false } },
                    "clientInfo": { "name": "clawbox", "version": env!("CARGO_PKG_VERSION") }
                }),
                90,
            )
            .await?;

        // session/new
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

        // Background task: answer permission requests per policy, drain updates.
        // (Updates that matter for a prompt are also collected synchronously in
        // prompt(); this loop guarantees permission requests never block.)
        // We move only a cloneable handle; JsonRpcClient methods take &self, so
        // wrap the client in Arc for shared access.
        // NOTE: implemented inline in prompt() below for v1 — see prompt().

        // For v1 we keep a single-flight model: no background loop; prompt()
        // owns the inbound receiver. Store it on the session.
        Ok(AcpSession { client, session_id }).map(|mut s| {
            s.attach_inbound(inbound);
            s
        })
    }

    // inbound receiver is consumed by prompt(); hold it in an Option.
    fn attach_inbound(&mut self, _rx: tokio::sync::mpsc::Receiver<Inbound>) {
        // placeholder to keep signature; real storage added below
    }

    pub async fn prompt(&self, _text: &str) -> Result<PromptResult, String> {
        unimplemented!("filled in Step 3b")
    }
}
```

> 注：上面 `start` 里 inbound 的所有权处理需要落到结构体字段。**Step 3b** 收敛为最终形态（把 `inbound` 存进 `AcpSession`，`prompt` 里边发 prompt 边循环处理 inbound）。

- [ ] **Step 3b：收敛 session 实现为最终形态**

用下面完整版**替换** Step 3 写的 `impl AcpSession` 与 struct 定义（保留文件顶部 use 和 `extract_text`/`parse_perm_options`/`tool_name_from_perm`）：

```rust
pub struct AcpSession {
    client: std::sync::Arc<JsonRpcClient>,
    session_id: String,
    inbound: tokio::sync::Mutex<tokio::sync::mpsc::Receiver<Inbound>>,
    policy: PermissionPolicy,
}

impl AcpSession {
    pub async fn start(
        adapter_id: &str,
        cwd: &Path,
        policy: PermissionPolicy,
    ) -> Result<AcpSession, String> {
        let adapter = find_adapter(adapter_id)
            .ok_or_else(|| format!("unknown adapter: {}", adapter_id))?;
        let (client, inbound) = JsonRpcClient::spawn(adapter.binary, &[], cwd).await?;
        let client = std::sync::Arc::new(client);

        client
            .request_with_timeout(
                "initialize",
                json!({
                    "protocolVersion": 1,
                    "clientCapabilities": { "fs": { "readTextFile": true, "writeTextFile": false } },
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
            inbound: tokio::sync::Mutex::new(inbound),
            policy,
        })
    }

    /// Send one prompt turn. Answers permission requests per policy while
    /// draining session/update text until the turn's response arrives.
    pub async fn prompt(&self, text: &str) -> Result<PromptResult, String> {
        // Fire the prompt request in the background so we can service inbound
        // permission requests that the agent raises mid-turn.
        let client = self.client.clone();
        let session_id = self.session_id.clone();
        let prompt_params = json!({
            "sessionId": session_id,
            "prompt": [ { "type": "text", "text": text } ]
        });
        let prompt_fut = {
            let client = client.clone();
            tokio::spawn(async move {
                client
                    .request_with_timeout("session/prompt", prompt_params, 600)
                    .await
            })
        };

        let mut collected = String::new();
        let mut rx = self.inbound.lock().await;

        loop {
            tokio::select! {
                maybe = rx.recv() => {
                    match maybe {
                        Some(Inbound::Notification { method, params }) => {
                            if method == "session/update" {
                                if let Some(u) = params.get("update") {
                                    if let Some(t) = extract_text(u) {
                                        collected.push_str(&t);
                                    }
                                }
                            }
                        }
                        Some(Inbound::Request { id, method, params }) => {
                            if method == "session/request_permission" {
                                let opts = parse_perm_options(&params);
                                let tool = tool_name_from_perm(&params);
                                let decision = decide(self.policy, &tool, &opts);
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
                        None => break,
                    }
                }
                res = &mut { &prompt_fut }, if false => { let _ = res; }
            }

            if prompt_fut.is_finished() {
                break;
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
}
```

> 实现者注意：`tokio::select!` 中的 `prompt_fut` 分支写法以“轮询 `is_finished()` + 主体收 inbound”为准，上面 select 的第二分支仅占位；若 borrow-checker 不满意，改为在 loop 顶部 `if prompt_fut.is_finished() { break }` 并对 `rx.recv()` 加 `tokio::time::timeout(Duration::from_millis(200), ...)` 轮询即可。目标行为：prompt 完成即退出 loop，其间持续应答 permission。

- [ ] **Step 4：运行测试确认通过**

Run: `cd src-tauri && cargo test --lib acp::session`
Expected: PASS（2 tests，纯 `extract_text` 逻辑）。

- [ ] **Step 5：编译整体**

Run: `cd src-tauri && cargo check`
Expected: 通过。

- [ ] **Step 6：Commit**

```bash
git add src-tauri/src/acp/session.rs
git commit -m "feat(acp): session manager (initialize/session/prompt + inline permission handling)"
```

---

### Task 6：M1 live smoke 测试

**Files:**
- Modify: `src-tauri/tests/smoke.rs`

**Interfaces:**
- Consumes: `clawbox_lib::acp::adapters::find_adapter`、`clawbox_lib::acp::session::AcpSession`、`PermissionPolicy`。
- 需要 `AcpSession`、`start`、`prompt`、`PermissionPolicy`、`adapters` 为 `pub`（M1 各 task 已 pub）。

- [ ] **Step 1：确认现有 smoke 测试 gate 模式**

Run: `sed -n '1,30p' src-tauri/tests/smoke.rs`
Expected: 看到现有 live 测试用 `is_installed()` gate 的写法，照抄该模式。

- [ ] **Step 2：加 ACP live 测试**

在 `src-tauri/tests/smoke.rs` 末尾追加（gate 在 claude-agent-acp 未装时跳过）：

```rust
#[tokio::test]
async fn acp_claude_handshake_and_prompt() {
    use clawbox_lib::acp::adapters::find_adapter;
    use clawbox_lib::acp::permission::PermissionPolicy;
    use clawbox_lib::acp::session::AcpSession;

    let adapter = find_adapter("claude-agent-acp").unwrap();
    if !adapter.is_installed() {
        eprintln!("skip: claude-agent-acp not installed");
        return;
    }

    let cwd = std::env::temp_dir();
    let session = AcpSession::start("claude-agent-acp", &cwd, PermissionPolicy::ReadOnly)
        .await
        .expect("session start");

    let result = session
        .prompt("Reply with exactly the word: pong")
        .await
        .expect("prompt");

    assert!(!result.stop_reason.is_empty());
    assert!(
        result.text.to_lowercase().contains("pong"),
        "expected 'pong' in reply, got: {}",
        result.text
    );
}
```

- [ ] **Step 3：确认 lib 暴露 acp（tests 走 `clawbox_lib`）**

`src/lib.rs` 已 `pub mod acp;`（Task 1）。若 `tests/smoke.rs` 用的 crate 名不同，按现有 smoke 测试的 `use` 前缀对齐。

- [ ] **Step 4：运行 smoke（本机已装 claude-agent-acp）**

Run: `cd src-tauri && cargo test --test smoke acp_claude -- --nocapture`
Expected: PASS（真实拿到含 "pong" 的回复；首次冷启动可能接近 90s）。

- [ ] **Step 5：Commit**

```bash
git add src-tauri/tests/smoke.rs
git commit -m "test(acp): live smoke — claude-agent-acp handshake + prompt returns pong"
```

---

## Milestone M2：审核引擎

### Task 7：审核数据模型 + 报告落盘

**Files:**
- Modify: `src-tauri/src/acp/review.rs`

**Interfaces:**
- Produces:
  - `#[derive(Serialize, Deserialize, Clone)] enum ReviewScope { WholeProject, GitDiff { base: String } }`
  - `#[derive(Serialize, Deserialize, Clone)] struct RoleAssignment { adapter_id: String, model: Option<String> }`
  - `#[derive(Serialize, Deserialize, Clone)] struct ReviewTask { id, project_path: String, scope: ReviewScope, reviewers: Vec<RoleAssignment>, summarizer: RoleAssignment, created_at: i64 }`
  - `#[derive(Serialize, Deserialize, Clone)] enum Severity { Info, Warning, Error }`
  - `#[derive(Serialize, Deserialize, Clone)] struct Finding { file: String, line: Option<u32>, severity: Severity, title: String, detail: String, reviewer: String }`
  - `#[derive(Serialize, Deserialize, Clone)] enum ReviewStatus { Running, Completed, Failed { message: String } }`
  - `#[derive(Serialize, Deserialize, Clone)] struct ReviewReport { task_id, findings: Vec<Finding>, summary: String, status: ReviewStatus, created_at: i64 }`
  - `fn reviews_dir() -> PathBuf`（`~/.clawbox/reviews`）
  - `fn save_report(r: &ReviewReport) -> Result<(), String>`
  - `fn load_report(task_id: &str) -> Result<ReviewReport, String>`
  - `fn list_reports() -> Vec<ReviewReport>`（按 created_at 倒序）

- [ ] **Step 1：写失败测试**

```rust
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
```

- [ ] **Step 2：运行测试确认失败**

Run: `cd src-tauri && cargo test --lib acp::review`
Expected: FAIL。

- [ ] **Step 3：实现**

替换 `review.rs` 顶部占位，加数据模型 + 落盘（模式抄 `commands/feedback.rs`）：

```rust
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
```

- [ ] **Step 4：运行测试确认通过**

Run: `cd src-tauri && cargo test --lib acp::review`
Expected: PASS（2 tests）。

- [ ] **Step 5：Commit**

```bash
git add src-tauri/src/acp/review.rs
git commit -m "feat(acp): review data model + report persistence (~/.clawbox/reviews)"
```

---

### Task 8：findings JSON 解析（带降级）

**Files:**
- Modify: `src-tauri/src/acp/review.rs`

**Interfaces:**
- Consumes: `Finding`、`Severity`（Task 7）。
- Produces:
  - `fn parse_findings(reviewer: &str, agent_text: &str) -> Vec<Finding>`（从 agent 回复里抠出 JSON 数组；解析失败降级为单条纯文本 finding）
  - `fn extract_json_block(text: &str) -> Option<&str>`（找 ```json ... ``` 或裸 `[...]`）

- [ ] **Step 1：写失败测试**

```rust
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
```

- [ ] **Step 2：运行测试确认失败**

Run: `cd src-tauri && cargo test --lib acp::review::parse_tests`
Expected: FAIL。

- [ ] **Step 3：实现**

在 `review.rs` 追加：

```rust
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
```

- [ ] **Step 4：运行测试确认通过**

Run: `cd src-tauri && cargo test --lib acp::review`
Expected: PASS（Task7 的 2 + 本 task 的 3 = 5 tests）。

- [ ] **Step 5：Commit**

```bash
git add src-tauri/src/acp/review.rs
git commit -m "feat(acp): parse reviewer findings JSON with prose fallback"
```

---

### Task 9：审核编排 run_review

**Files:**
- Modify: `src-tauri/src/acp/review.rs`

**Interfaces:**
- Consumes: `AcpSession`、`PermissionPolicy::ReadOnly`（M1）；`ReviewTask`、`Finding`、`ReviewReport`、`save_report`、`parse_findings`（Task 7/8）。
- Produces:
  - `fn reviewer_prompt(scope: &ReviewScope) -> String`
  - `fn summarizer_prompt(findings: &[Finding]) -> String`
  - `async fn run_review(task: ReviewTask) -> Result<ReviewReport, String>`
  - `fn now_secs() -> i64`

- [ ] **Step 1：写失败测试（prompt 构造是纯逻辑）**

```rust
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
```

- [ ] **Step 2：运行测试确认失败**

Run: `cd src-tauri && cargo test --lib acp::review::orchestration_tests`
Expected: FAIL。

- [ ] **Step 3：实现**

在 `review.rs` 追加：

```rust
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
```

- [ ] **Step 4：运行测试确认通过 + 整体编译**

Run: `cd src-tauri && cargo test --lib acp::review && cargo check`
Expected: PASS（8 tests），编译通过。

- [ ] **Step 5：Commit**

```bash
git add src-tauri/src/acp/review.rs
git commit -m "feat(acp): review orchestration — reviewers + summarizer -> report"
```

---

### Task 10：M2 live smoke（对本仓库真实审核）

**Files:**
- Modify: `src-tauri/tests/smoke.rs`

**Interfaces:**
- Consumes: `clawbox_lib::acp::review::{run_review, ReviewTask, ReviewScope, RoleAssignment, now_secs}`。

- [ ] **Step 1：加 live 审核测试**

在 `tests/smoke.rs` 追加：

```rust
#[tokio::test]
async fn acp_review_produces_report() {
    use clawbox_lib::acp::adapters::find_adapter;
    use clawbox_lib::acp::review::*;

    if !find_adapter("claude-agent-acp").unwrap().is_installed() {
        eprintln!("skip: claude-agent-acp not installed");
        return;
    }

    let task = ReviewTask {
        id: format!("smoke_{}", now_secs()),
        project_path: env!("CARGO_MANIFEST_DIR").to_string(),
        scope: ReviewScope::GitDiff { base: "HEAD~1".into() },
        reviewers: vec![RoleAssignment { adapter_id: "claude-agent-acp".into(), model: None }],
        summarizer: RoleAssignment { adapter_id: "claude-agent-acp".into(), model: None },
        created_at: now_secs(),
    };

    let report = run_review(task).await.expect("run_review");
    assert!(matches!(report.status, ReviewStatus::Completed));
    assert!(!report.summary.is_empty());
    // Report was persisted:
    let reloaded = load_report(&report.task_id).expect("load");
    assert_eq!(reloaded.task_id, report.task_id);
}
```

- [ ] **Step 2：运行（可能耗时数分钟，冷启动 + 两次 session）**

Run: `cd src-tauri && cargo test --test smoke acp_review -- --nocapture`
Expected: PASS（产出报告，summary 非空，文件已落盘）。

- [ ] **Step 3：Commit**

```bash
git add src-tauri/tests/smoke.rs
git commit -m "test(acp): live smoke — run_review on this repo produces a persisted report"
```

---

## Milestone M3：命令层 + Review UI

### Task 11：Tauri 命令层

**Files:**
- Create: `src-tauri/src/commands/acp.rs`
- Modify: `src-tauri/src/commands/mod.rs`、`src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `acp::adapters::list_adapter_info`、`acp::review::{run_review, list_reports, load_report, ReviewTask, ReviewScope, RoleAssignment, ReviewReport, now_secs}`。
- Produces（Tauri 命令，均 async）:
  - `acp_list_adapters() -> Vec<AdapterInfo>`
  - `acp_install_adapter(id: String) -> Result<String, String>`（npm 装，模式抄 `install.rs`）
  - `review_run(project_path, scope, reviewers, summarizer) -> Result<ReviewReport, String>`
  - `review_list() -> Vec<ReviewReport>`
  - `review_get(task_id: String) -> Result<ReviewReport, String>`

- [ ] **Step 1：写命令层**

创建 `src-tauri/src/commands/acp.rs`：

```rust
use crate::acp::adapters::{list_adapter_info, find_adapter, AdapterInfo};
use crate::acp::review::{
    list_reports, load_report, now_secs, run_review, ReviewReport, ReviewScope, ReviewTask,
    RoleAssignment,
};
use std::process::Command;

#[tauri::command]
pub fn acp_list_adapters() -> Vec<AdapterInfo> {
    list_adapter_info()
}

#[tauri::command]
pub fn acp_install_adapter(id: String) -> Result<String, String> {
    let adapter = find_adapter(&id).ok_or_else(|| format!("unknown adapter: {}", id))?;
    // install_hint form: "npm install -g <pkg>" (possibly with --force)
    let parts: Vec<&str> = adapter.install_hint.split_whitespace().collect();
    if parts.is_empty() || parts[0] != "npm" {
        return Err(format!("unsupported install hint: {}", adapter.install_hint));
    }
    let out = Command::new(parts[0])
        .args(&parts[1..])
        .output()
        .map_err(|e| format!("failed to run npm: {}", e))?;
    if out.status.success() {
        Ok(format!("Installed {}", adapter.label))
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

#[tauri::command]
pub async fn review_run(
    project_path: String,
    scope: ReviewScope,
    reviewers: Vec<RoleAssignment>,
    summarizer: RoleAssignment,
) -> Result<ReviewReport, String> {
    if reviewers.is_empty() {
        return Err("at least one reviewer is required".into());
    }
    let task = ReviewTask {
        id: format!("review_{}", now_secs()),
        project_path,
        scope,
        reviewers,
        summarizer,
        created_at: now_secs(),
    };
    run_review(task).await
}

#[tauri::command]
pub fn review_list() -> Vec<ReviewReport> {
    list_reports()
}

#[tauri::command]
pub fn review_get(task_id: String) -> Result<ReviewReport, String> {
    load_report(&task_id)
}
```

- [ ] **Step 2：注册模块与命令**

修改 `src-tauri/src/commands/mod.rs`，加一行 `pub mod acp;`（按字母序放 aggregate 之后）。

修改 `src-tauri/src/lib.rs` 的 `generate_handler!`，在 logs 命令后追加：

```rust
            commands::acp::acp_list_adapters,
            commands::acp::acp_install_adapter,
            commands::acp::review_run,
            commands::acp::review_list,
            commands::acp::review_get,
```

- [ ] **Step 3：编译 + 全量测试**

Run: `cd src-tauri && cargo build && cargo test --lib`
Expected: 编译通过；所有单元测试 PASS。

- [ ] **Step 4：Commit**

```bash
git add src-tauri/src/commands/acp.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs
git commit -m "feat(acp): tauri command layer (adapters + review run/list/get)"
```

---

### Task 12：前端 API 封装 + i18n

**Files:**
- Create: `src/lib/api/acp.ts`
- Modify: `src/lib/i18n/en.json`、`src/lib/i18n/zh.json`

**Interfaces:**
- Produces（TS）:
  - `interface AdapterInfo { id, label, installed, version, install_hint }`
  - `type ReviewScope = 'whole_project' | { git_diff: { base: string } }` → 简化为 `{ whole_project?: null } | { git_diff: { base } }`；实现见下（对齐 serde snake_case）
  - `interface Finding { file, line, severity, title, detail, reviewer }`
  - `interface ReviewReport { task_id, findings, summary, status, created_at }`
  - `acp_list_adapters()`, `acp_install_adapter(id)`, `review_run(...)`, `review_list()`, `review_get(taskId)`

- [ ] **Step 1：写 TS 封装**

创建 `src/lib/api/acp.ts`：

```ts
import { invoke } from '@tauri-apps/api/core';

export interface AdapterInfo {
  id: string;
  label: string;
  installed: boolean;
  version: string | null;
  install_hint: string;
}

// serde: ReviewScope is either "whole_project" (unit) or { git_diff: { base } }.
// serde(rename_all="snake_case") on an enum with a struct variant serializes
// the unit variant as the string "whole_project".
export type ReviewScope = 'whole_project' | { git_diff: { base: string } };

export interface RoleAssignment {
  adapter_id: string;
  model: string | null;
}

export type Severity = 'info' | 'warning' | 'error';

export interface Finding {
  file: string;
  line: number | null;
  severity: Severity;
  title: string;
  detail: string;
  reviewer: string;
}

export type ReviewStatus =
  | { state: 'running' }
  | { state: 'completed' }
  | { state: 'failed'; message: string };

export interface ReviewReport {
  task_id: string;
  findings: Finding[];
  summary: string;
  status: ReviewStatus;
  created_at: number;
}

export async function acp_list_adapters(): Promise<AdapterInfo[]> {
  try {
    return await invoke<AdapterInfo[]>('acp_list_adapters');
  } catch {
    return [];
  }
}

export async function acp_install_adapter(id: string): Promise<string> {
  return await invoke<string>('acp_install_adapter', { id });
}

export async function review_run(
  projectPath: string,
  scope: ReviewScope,
  reviewers: RoleAssignment[],
  summarizer: RoleAssignment
): Promise<ReviewReport> {
  return await invoke<ReviewReport>('review_run', {
    projectPath,
    scope,
    reviewers,
    summarizer,
  });
}

export async function review_list(): Promise<ReviewReport[]> {
  try {
    return await invoke<ReviewReport[]>('review_list');
  } catch {
    return [];
  }
}

export async function review_get(taskId: string): Promise<ReviewReport> {
  return await invoke<ReviewReport>('review_get', { taskId });
}
```

- [ ] **Step 2：加 i18n（用脚本，保持有序）**

Run（在仓库根目录）:

```bash
python3 - <<'EOF'
import json, collections
keys = {
  'en': {"review": {
    "title": "Code Review", "projectPath": "Project path", "browse": "Browse",
    "scope": "Scope", "wholeProject": "Whole project", "gitDiff": "Git diff (base)",
    "reviewers": "Reviewers", "summarizer": "Summarizer", "run": "Run Review",
    "running": "Reviewing...", "history": "History", "empty": "No reviews yet.",
    "findings": "Findings", "summary": "Summary", "noAdapters": "No ACP agents installed.",
    "install": "Install"
  }},
  'zh': {"review": {
    "title": "代码审核", "projectPath": "项目路径", "browse": "浏览",
    "scope": "范围", "wholeProject": "整个项目", "gitDiff": "Git 差异（基准）",
    "reviewers": "审核者", "summarizer": "汇总者", "run": "开始审核",
    "running": "审核中...", "history": "历史", "empty": "还没有审核记录。",
    "findings": "问题", "summary": "摘要", "noAdapters": "未安装任何 ACP agent。",
    "install": "安装"
  }},
}
for lang, extra in keys.items():
    p = f'src/lib/i18n/{lang}.json'
    d = json.load(open(p), object_pairs_hook=collections.OrderedDict)
    d.update(extra)
    json.dump(d, open(p,'w'), ensure_ascii=False, indent=2); open(p,'a').write('\n')
    print(lang, 'ok')
EOF
```

- [ ] **Step 3：类型检查**

Run: `npm run check`
Expected: 0 errors（既有 warning 允许）。

- [ ] **Step 4：Commit**

```bash
git add src/lib/api/acp.ts src/lib/i18n/en.json src/lib/i18n/zh.json
git commit -m "feat(ui): ACP/review TS API bindings + i18n keys"
```

---

### Task 13：Review 页（改造 agents 页）

**Files:**
- Modify: `src/routes/agents/+page.svelte`（整体替换为 Review 页）

**Interfaces:**
- Consumes: `src/lib/api/acp.ts` 全部导出。
- 现有 App.svelte 通过 `currentPage === 'agents'` 渲染此页（无需改路由）。

- [ ] **Step 1：确认入口不变**

Run: `grep -n "agents" src/App.svelte src/lib/components/Sidebar.svelte`
Expected: 看到 `AgentsPage` 在 `currentPage === 'agents'` 分支渲染、Sidebar 有 agents 项。入口保持，只换页面内容。

- [ ] **Step 2：替换页面为 Review UI**

用以下内容整体替换 `src/routes/agents/+page.svelte`（保留 `<script lang="ts">` 结构，用 Svelte 5 runes，与仓库其它页一致）：

```svelte
<script lang="ts">
  import { _ } from 'svelte-i18n';
  import { onMount } from 'svelte';
  import { open } from '@tauri-apps/plugin-dialog';
  import {
    acp_list_adapters, acp_install_adapter, review_run, review_list,
    type AdapterInfo, type ReviewReport, type ReviewScope, type RoleAssignment,
  } from '$lib/api/acp';

  let adapters = $state<AdapterInfo[]>([]);
  let projectPath = $state('');
  let scopeKind = $state<'whole' | 'diff'>('diff');
  let diffBase = $state('main');
  let selectedReviewers = $state<Record<string, boolean>>({});
  let running = $state(false);
  let current = $state<ReviewReport | null>(null);
  let history = $state<ReviewReport[]>([]);
  let error = $state('');

  async function refresh() {
    adapters = await acp_list_adapters();
    history = await review_list();
  }

  async function pickDir() {
    const picked = await open({ directory: true, multiple: false });
    if (typeof picked === 'string') projectPath = picked;
  }

  async function install(id: string) {
    try { await acp_install_adapter(id); await refresh(); }
    catch (e) { error = e instanceof Error ? e.message : String(e); }
  }

  async function run() {
    error = '';
    const reviewers: RoleAssignment[] = adapters
      .filter((a) => a.installed && selectedReviewers[a.id])
      .map((a) => ({ adapter_id: a.id, model: null }));
    if (!projectPath || reviewers.length === 0) {
      error = 'Pick a project and at least one reviewer.';
      return;
    }
    const scope: ReviewScope = scopeKind === 'whole' ? 'whole_project' : { git_diff: { base: diffBase } };
    running = true;
    current = null;
    try {
      current = await review_run(projectPath, scope, reviewers, reviewers[0]);
      history = await review_list();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      running = false;
    }
  }

  onMount(refresh);
</script>

<div class="review-page">
  <h1>{$_('review.title')}</h1>

  {#if adapters.filter((a) => a.installed).length === 0}
    <div class="glass-card empty-adapters">
      <p>{$_('review.noAdapters')}</p>
      {#each adapters as a (a.id)}
        <div class="adapter-row">
          <span>{a.label}</span>
          <button class="neon-button" onclick={() => install(a.id)}>{$_('review.install')}</button>
        </div>
      {/each}
    </div>
  {:else}
    <div class="glass-card form">
      <label class="row">
        <span>{$_('review.projectPath')}</span>
        <input type="text" bind:value={projectPath} placeholder="/path/to/project" />
        <button class="neon-button" onclick={pickDir}>{$_('review.browse')}</button>
      </label>

      <div class="row">
        <span>{$_('review.scope')}</span>
        <select bind:value={scopeKind}>
          <option value="diff">{$_('review.gitDiff')}</option>
          <option value="whole">{$_('review.wholeProject')}</option>
        </select>
        {#if scopeKind === 'diff'}
          <input type="text" bind:value={diffBase} placeholder="main" />
        {/if}
      </div>

      <div class="row">
        <span>{$_('review.reviewers')}</span>
        <div class="reviewer-list">
          {#each adapters.filter((a) => a.installed) as a (a.id)}
            <label class="reviewer-chip">
              <input type="checkbox" bind:checked={selectedReviewers[a.id]} />
              {a.label}
            </label>
          {/each}
        </div>
      </div>

      {#if error}<div class="error">{error}</div>{/if}

      <button class="neon-button primary" onclick={run} disabled={running}>
        {running ? $_('review.running') : $_('review.run')}
      </button>
    </div>

    {#if current}
      <div class="glass-card report">
        <h2>{$_('review.summary')}</h2>
        <p class="summary">{current.summary}</p>
        <h2>{$_('review.findings')} ({current.findings.length})</h2>
        {#each current.findings as f}
          <div class="finding" data-sev={f.severity}>
            <span class="sev">{f.severity}</span>
            <span class="loc">{f.file}{f.line != null ? ':' + f.line : ''}</span>
            <span class="ftitle">{f.title}</span>
            <p class="fdetail">{f.detail}</p>
          </div>
        {/each}
      </div>
    {/if}

    <div class="glass-card history">
      <h2>{$_('review.history')}</h2>
      {#if history.length === 0}
        <p class="muted">{$_('review.empty')}</p>
      {:else}
        {#each history as r (r.task_id)}
          <button class="history-item" onclick={() => (current = r)}>
            <span>{r.task_id}</span>
            <span class="muted">{r.findings.length} findings · {new Date(r.created_at * 1000).toLocaleString()}</span>
          </button>
        {/each}
      {/if}
    </div>
  {/if}
</div>

<style>
  .review-page { max-width: 900px; margin: 0 auto; }
  .review-page h1 { color: var(--neon-cyan); text-shadow: var(--glow-cyan); margin-bottom: 1.5rem; }
  .glass-card { padding: 1.5rem; margin-bottom: 1.5rem; }
  .row { display: flex; align-items: center; gap: 0.75rem; margin-bottom: 1rem; }
  .row > span:first-child { min-width: 90px; color: var(--text-secondary); }
  .row input[type="text"], .row select {
    flex: 1; padding: 0.6rem 0.75rem; background: var(--bg-tertiary);
    border: 1px solid rgba(255,255,255,0.1); border-radius: 0.5rem; color: var(--text-primary);
  }
  .reviewer-list { display: flex; gap: 0.75rem; flex-wrap: wrap; }
  .reviewer-chip { display: flex; align-items: center; gap: 0.4rem; }
  .neon-button {
    background: var(--bg-tertiary); border: 1px solid var(--neon-cyan); color: var(--neon-cyan);
    padding: 0.6rem 1.2rem; border-radius: 0.5rem; cursor: pointer;
  }
  .neon-button.primary { background: linear-gradient(135deg, var(--neon-cyan), var(--neon-purple)); color: #001; }
  .neon-button:disabled { opacity: 0.5; cursor: not-allowed; }
  .error { color: var(--neon-pink); margin-bottom: 1rem; }
  .summary { color: var(--text-secondary); white-space: pre-wrap; }
  .finding { padding: 0.75rem 0; border-top: 1px solid rgba(255,255,255,0.08); }
  .finding .sev { font-size: 0.7rem; font-weight: 700; padding: 0.1rem 0.5rem; border-radius: 999px; margin-right: 0.5rem; }
  .finding[data-sev="error"] .sev { background: rgba(255,0,110,0.15); color: var(--neon-pink); }
  .finding[data-sev="warning"] .sev { background: rgba(255,136,0,0.15); color: var(--neon-orange); }
  .finding[data-sev="info"] .sev { background: rgba(0,245,255,0.15); color: var(--neon-cyan); }
  .finding .loc { font-family: monospace; color: var(--text-muted); margin-right: 0.5rem; }
  .fdetail { margin: 0.4rem 0 0; color: var(--text-secondary); }
  .history-item {
    display: flex; justify-content: space-between; width: 100%; text-align: left;
    padding: 0.6rem; background: none; border: none; border-top: 1px solid rgba(255,255,255,0.08);
    color: var(--text-primary); cursor: pointer;
  }
  .muted { color: var(--text-muted); }
  .empty-adapters .adapter-row { display: flex; justify-content: space-between; align-items: center; padding: 0.5rem 0; }
</style>
```

- [ ] **Step 3：确认 dialog 插件可用**

Run: `grep -n "plugin-dialog\|tauri-plugin-dialog" package.json src-tauri/Cargo.toml src-tauri/src/lib.rs`
Expected: 若无 `@tauri-apps/plugin-dialog`，需补依赖。若缺失，执行：

```bash
npm install --legacy-peer-deps @tauri-apps/plugin-dialog
```

并在 `src-tauri/Cargo.toml` 加 `tauri-plugin-dialog = "2"`，在 `src-tauri/src/lib.rs` 的 builder 链加 `.plugin(tauri_plugin_dialog::init())`。（若已存在则跳过。）

- [ ] **Step 4：类型检查**

Run: `npm run check`
Expected: 0 errors。

- [ ] **Step 5：Commit**

```bash
git add src/routes/agents/+page.svelte package.json package-lock.json src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/lib.rs
git commit -m "feat(ui): rebuild agents page as Code Review page (create/run/report/history)"
```

---

### Task 14：文档收尾 + 全量验证

**Files:**
- Modify: `ROADMAP.md`

**Interfaces:** 无。

- [ ] **Step 1：更新 ROADMAP**

修改 `ROADMAP.md`：把 Claude Code CLI / Codex CLI 从 "Planned for future iterations" 移到已支持区，注明经由 ACP 桥接入（`claude-agent-acp` / `codex-acp`），并补一句代码审核功能已落地（reviewer + summarizer，只读策略）。

- [ ] **Step 2：全量后端验证**

Run: `cd src-tauri && cargo build && cargo test`
Expected: 编译通过；所有单元测试 PASS；live smoke 在本机装有 claude-agent-acp 时 PASS（否则打印 skip）。

- [ ] **Step 3：全量前端验证**

Run: `npm run check`
Expected: 0 errors。

- [ ] **Step 4：Commit**

```bash
git add ROADMAP.md
git commit -m "docs: ROADMAP — ACP-based Claude/Codex support + code review shipped"
```

---

## Self-Review 结论

- **Spec 覆盖**：第1节(ACP子系统)=Task1-6；第2节(审核工作流)=Task7-10；第3节(UI+里程碑)=Task11-14。权限只读=Task4+Task9；报告落盘=Task7；findings 降级=Task8；冷启动风险=Task5 超时90s+smoke说明；codex-acp 未实测=注册表含但 live 只测 claude（符合 spec 风险条目）。
- **类型一致性**：`AcpSession::start/prompt`、`PromptResult`、`ReviewReport`、`run_review`、`parse_findings`、命令名（`acp_list_adapters`/`review_run`/...）在定义与消费处一致。
- **占位符**：无 TBD；每个代码步骤给出完整代码。Task5 Step3→3b 是刻意的"先骨架后收敛"，最终形态完整。
