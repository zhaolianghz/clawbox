# ACP Agent 接入 + 代码审核工作流 设计

**日期**: 2026-07-15
**状态**: 已获用户认可
**参考**: [FreeBuddy](https://github.com/maojindao55/freebuddy)（Electron 版同类产品）、[Agent Client Protocol](https://agentclientprotocol.com)

## 背景与目标

ClawBox 已通过 `Backend` trait 管理 OpenClaw / Hermes 两个 agent 运行时。原 ROADMAP 计划为
Claude Code、Codex 各写一套 CLI 输出解析器（各 1-2 天）。本设计以 **ACP（Agent Client
Protocol）** 替代该路线：一套协议接入所有 ACP 兼容 agent，并在其上构建**多 agent 代码审核**
功能（本项目的核心新需求）。

## Spike 验证结论（2026-07-15，本机实测）

1. `claude-agent-acp` 0.59.0（npm 全局安装）：stdio JSON-RPC `initialize` 握手成功，
   返回能力集（loadSession、session resume/fork/list/close/delete、prompt image/embeddedContext、
   MCP http/sse）；`session/new` 返回真实 sessionId。首次冷启动约 40s，后续快。
2. 官方 Rust 生态存在：crates.io 上 `agent-client-protocol = 1.2.0`（协议类型与 trait）、
   `agent-client-protocol-tokio`（tokio 工具）。协议 schema 源码即 Rust。无需手写协议层。
3. 协议内置权限流：agent 执行任何工具前必须发 `session/request_permission`，由客户端决定
   allow/reject（AllowOnce/AllowAlways/RejectOnce/RejectAlways）。**"只读审核"可在协议层硬保证**。
4. `session/prompt` 返回 `stopReason`（end_turn/max_tokens/max_turn_requests/refusal/cancelled），
   过程中通过 `session/update` 通知流式推送消息块、工具调用、计划。

## 第 1 节：ACP 接入架构

**决策：ACP 作为独立子系统，不塞进 `Backend` trait。**

理由：`Backend` trait 是 gateway/cron 形状（管理常驻运行时进程）；ACP agent 是会话形状
（spawn 桥进程 → session → prompt → 流式更新）。硬套会产生大量 "not applicable" 方法
（原 ROADMAP 已预见此问题）。FreeBuddy 同样将 adapter/member 与 workflow 分层。

新增 `src-tauri/src/acp/`：

### adapters.rs — 适配器注册表

```rust
struct AcpAdapter {
    id: &'static str,          // "claude-agent-acp" | "codex-acp"
    label: &'static str,       // "ClaudeCode" | "Codex"
    binary: &'static str,      // 可被用户配置覆盖
    install_hint: &'static str,// npm install -g ...
    check_probe: &'static [&'static str], // 版本探测参数，如 ["--cli","--version"]
}
```

v1 内置两个适配器：`claude-agent-acp`、`codex-acp`。提供检测（installed/version）与
安装引导（复用 InstallWizard 模式）。

### session.rs — AcpSessionManager

- tokio + `agent-client-protocol` crate。
- 职责：spawn 桥进程（每 session 一个子进程）、`initialize` 握手、`session/new`、
  `session/prompt`、`session/cancel`、进程回收。
- `session/update` 通知流通过 **Tauri events**（`acp://session/<id>/update`）推给前端。
- 会话表：`HashMap<SessionId, SessionHandle>`，Tauri managed state。

### permission.rs — 权限策略

回应 `session/request_permission` 的策略对象：

- `ReadOnly`：写类工具调用（write/edit/bash 修改类）一律 `RejectOnce`；读类放行。
  v1 审核全部使用此策略。
- `AskUser`：转发到前端弹窗（对应现有 UI 模式），供后续聊天场景使用。

**不改动**：`Backend` trait、OpenClaw/Hermes 后端、现有 capabilities 体系。

## 第 2 节：审核工作流（v1 刻意收窄）

借鉴 FreeBuddy 的 role/policy/gate 模型，v1 砍到最小可用：

### 数据模型

```rust
struct ReviewTask {
    id: String,
    project_path: PathBuf,
    scope: ReviewScope,          // WholeProject | GitDiff { base: String }
    reviewers: Vec<RoleAssignment>, // 1..N，可不同 agent 并行
    summarizer: RoleAssignment,  // 恰好 1 个
    created_at: i64,
}

struct RoleAssignment { adapter_id: String, model: Option<String> }

struct ReviewReport {
    task_id: String,
    findings: Vec<Finding>,      // { file, line, severity, title, detail, reviewer }
    summary: String,
    status: ReviewStatus,        // Running | Completed | Failed { message }
    created_at: i64,
}
```

### 执行流程

1. 每个 reviewer 开一个 ACP session（`cwd = project_path`，**ReadOnly 策略**），
   发送角色 prompt 模板（含 scope 描述），要求输出 JSON findings。
2. reviewer 全部结束后，summarizer session 接收各家 findings，去重合并、产出结构化报告。
3. 报告落盘 `~/.clawbox/reviews/<task_id>.json`，历史可回看。
4. 过程状态通过 Tauri events 流式推送前端。

### v1 明确不做（v2 候选）

写权限角色（implementer）、审批门禁（manual_approval gate）、maxLoops、
cron 定时审核、团队模板市场、BYOK 配置。

## 第 3 节：UI 与里程碑

现有 `agents` 页为无后端占位 UI（AgentFlow mock），**改造为 Review 页**：

- 建任务：选项目目录（Tauri dialog）+ 选 scope + 勾选 reviewer agents；
- 运行中：流式进度（各 reviewer 状态、当前动作）；
- 完成后：报告渲染（findings 按严重级别分组、文件定位）+ 历史任务列表。

### 里程碑（每个独立可交付）

| 里程碑 | 内容 | 验收 |
|---|---|---|
| **M1** | `acp/` 模块：adapters + AcpSessionManager + 权限策略 | 对真实 claude-agent-acp 的 smoke test：握手→session→prompt→拿到回复 |
| **M2** | 审核引擎：ReviewTask/角色 prompt/只读策略/报告生成与落盘 | 对本仓库跑一次真实审核，产出结构化报告 |
| **M3** | Review UI：任务创建/流式进度/报告渲染/历史 | 全流程在桌面应用内可操作 |

### 新增 Tauri 命令（预计）

`acp_list_adapters`、`acp_install_adapter`、`review_create`、`review_run`、
`review_cancel`、`review_list`、`review_get_report`。

### 测试策略

沿用仓库惯例：协议消息解析/权限策略判定用单元测试（fixture 驱动，TDD）；
`tests/smoke.rs` 增加 gated live 测试（本机装有 claude-agent-acp 才跑）。

## 风险与开放问题

- **冷启动 40s**：首次 spawn 桥进程慢。缓解：UI 显示预热状态；可选进程预热池（v2）。
- **codex-acp 未实测**：spike 只验证了 claude-agent-acp。M1 中补 codex-acp 探测，
  若行为差异大，v1 可只 ship ClaudeCode reviewer。
- **findings JSON 可靠性**：agent 输出未必是合法 JSON。缓解：prompt 强约束 +
  解析失败时降级为纯文本 finding。
