# Agent 管理中心 + PATH 修复 + 删除日志模块 设计

**日期**: 2026-07-16
**状态**: 已获用户认可
**参考**: [FreeBuddy](https://github.com/maojindao55/freebuddy) 的 Settings → Coding Agents

## 背景与目标

ClawBox 定位是「agent 集合管理平台」,但目前无法安装/管理 agent CLI 本体——安装能力
碎片化(仅首启向导装 Node+OpenClaw、Review 页空态装两个 ACP 桥),且没有一个统一入口
查看「哪些 agent 装了、什么版本、怎么装/升级」。本设计新增一个 **Agent 管理页**(对齐
FreeBuddy 的 Coding Agents),配套修复一个全局性的 **GUI 进程 PATH 探测缺陷**,并顺带
**删除无用的日志模块**。

## 现状核对(2026-07-16 本机实测)

已装:`claude`(~/.local/bin)、`codex`(/opt/homebrew/bin)、`openclaw`(homebrew)、
`hermes`(~/.local/bin → Python venv)、`opencode`(~/.opencode/bin)、`node`(nvm)。
未装:`cursor-agent`、`kimi`、`qodercli`、`codebuddy`。
npm 包版本:`@anthropic-ai/claude-code` 2.1.210、`@openai/codex` 0.144.4、
`opencode-ai` 1.18.2、`@tencent-ai/codebuddy-code` 2.122.0。
**关键事实**:Hermes 由 Python venv 安装(不是 npm),证明注册表必须支持多种安装方式。

## 第 1 节:统一 Agent 注册表

新增 `src-tauri/src/agents/mod.rs`(或 `agents.rs`),泛化现有 `AcpAdapter` 模式。每条目:

```rust
struct AgentDef {
    id: &'static str,
    label: &'static str,
    binary: &'static str,
    kind: AgentKind,          // NativeCli | AcpBridge | Runtime | Gateway
    install: InstallMethod,
    check_probe: &'static [&'static str],  // 版本探测参数
    depends_on: &'static [&'static str],   // 依赖的其他 agent id
    docs_url: &'static str,
}

enum AgentKind { NativeCli, AcpBridge, Runtime, Gateway }

enum InstallMethod {
    Npm { package: &'static str },
    Script { url: &'static str },   // curl -fsSL <url> | bash
    DetectOnly,                     // 只检测,不自动装(给文档链接)
}
```

**v1 内置 12 个条目**(对齐 FreeBuddy 全量):

| id | label | kind | install |
|---|---|---|---|
| node | Node.js | Runtime | 平台包管理器(复用现有 install_nodejs:brew/winget) |
| claude-code | Claude Code | NativeCli | Npm `@anthropic-ai/claude-code` |
| codex | Codex | NativeCli | Npm `@openai/codex` |
| openclaw | OpenClaw | Gateway | Npm `openclaw` |
| opencode | OpenCode | NativeCli | Npm `opencode-ai` |
| codebuddy | CodeBuddy | NativeCli | Npm `@tencent-ai/codebuddy-code` |
| cursor-agent | Cursor | NativeCli | Script `https://cursor.com/install` |
| kimi | Kimi | NativeCli | Script `https://code.kimi.com/kimi-code/install.sh` |
| qodercli | Qoder | NativeCli | Script `https://qoder.com/install` |
| claude-agent-acp | ClaudeCode ACP 桥 | AcpBridge | Npm `@agentclientprotocol/claude-agent-acp`,depends_on=[claude-code] |
| codex-acp | Codex ACP 桥 | AcpBridge | Npm `@agentclientprotocol/codex-acp` |
| hermes | Hermes | Gateway | DetectOnly(Python venv 装,仅检测 + 文档链接) |

> 本表为唯一权威来源,共 12 条。现有 `acp/adapters.rs` 的两个 ACP 桥条目并入本
> 注册表,`adapters.rs` 改为从统一注册表筛选 `kind == AcpBridge` 的条目
> (避免两份重复登记)。

**依赖**:`claude-agent-acp` depends_on `claude-code`;所有 Npm 类隐式依赖 `node`。
UI 在依赖未满足时,安装按钮引导先装依赖。

## 第 2 节:PATH 修复(全局生效)

**问题**:从 Finder/Dock 启动的 GUI 进程继承的 PATH 不含 nvm/homebrew/~/.local/bin 等
目录,导致 `Command::new("claude")` 找不到已装的二进制,检测误报「未安装」。现有
openclaw/hermes backend、ACP adapters、install.rs 全有此隐患(打包后尤其明显)。

**方案**:新增 `src-tauri/src/agents/path_env.rs`:

```rust
// 首次调用时跑一次 login shell 拿真实 PATH,缓存进 OnceLock。
pub fn resolved_path() -> &'static str;         // 完整 PATH 字符串
pub fn command(bin: &str) -> std::process::Command;  // 预置了 PATH env 的 Command
```

实现:`$SHELL -l -c 'echo $PATH'`(SHELL 未设时回退 `/bin/zsh`);解析失败则回退到
当前 `std::env::var("PATH")`。**所有 subprocess 探测/安装点统一改用 `path_env::command()`**:
- `agents/` 注册表检测与安装
- `acp/adapters.rs`、`acp/jsonrpc.rs`(spawn 桥)
- `backends/openclaw.rs`、`backends/hermes.rs`
- `commands/install.rs`、`commands/chat.rs`

一次修复,检测 + 安装 + ACP 会话 spawn 全部受益。

## 第 3 节:命令 + UI

**后端命令**(`src-tauri/src/commands/agents.rs`,均 async):
- `agents_list() -> Vec<AgentStatus>`:全量检测。`AgentStatus { id, label, kind, installed, version, deps_satisfied, install_method, docs_url }`
- `agent_install(id) -> Result<String, String>`:按 InstallMethod 执行。Npm 走
  `npm install -g <pkg>`(重装即升级);Script 走 `bash -c "curl -fsSL <url> | bash"`;
  DetectOnly 返回 Err 提示手动安装 + 文档链接。全部经 `path_env::command()`。

**UI**:
- **新增独立「Agent 管理」页**,接管侧边栏 —— 每个 agent 一行卡片:类型徽章、
  版本 or「未安装」、安装/升级按钮(依赖未满足时禁用并提示)、文档链接、失败错误。
- **导航理顺**:现在侧边栏「代理(Agents)」点进去是 ACP 审核页,语义错位。改为:
  - 「代理(Agents)」→ 新的 Agent 管理页
  - 新增「审核(Review)」侧边栏项 → 现有 ACP 审核页(`routes/agents/+page.svelte`
    迁移/重命名为 review 页;或保留文件、调整 App.svelte 路由与 Sidebar 标签)
- 新增 i18n `agents.*` 键(中英)。

## 第 4 节:删除日志模块

日志功能无实际用途,整体移除:
- 后端:删 `src-tauri/src/commands/logs.rs`;`commands/mod.rs` 去掉 `pub mod logs;`;
  `lib.rs` 从 `generate_handler!` 移除 `get_log_files`/`get_log_content`。
- 前端:删 `src/lib/api/logs.ts`、`src/routes/logs/+page.svelte`;
  `Sidebar.svelte` 移除日志项;`App.svelte` 移除 `currentPage === 'logs'` 分支与 import。
- i18n:移除 `logs.*` 键(中英)。

## 第 5 节:里程碑与测试

| 里程碑 | 内容 | 验收 |
|---|---|---|
| **M1** | path_env 解析 + 统一注册表 + agents_list 检测命令;acp/adapters 并入注册表 | 单测:注册表完整性(12 条、依赖引用有效)、安装命令构造、PATH 解析回退;live smoke:本机真实检测(claude/codex/openclaw/opencode/hermes/node 装,cursor/kimi/qoder/codebuddy 未装) |
| **M2** | agent_install 安装执行;所有探测点切到 path_env | 单测:各 InstallMethod 的命令构造;live smoke gated 跑一个 npm 类安装(不污染:装一个已装的做幂等验证,或用 `npm view` 干跑) |
| **M3** | Agent 管理页 + 导航理顺 + i18n;删除日志模块 | `cargo test --lib` + `npm run check` 全绿;真实窗口:Agent 页列出全部、检测态正确、日志入口消失 |

## 风险与开放问题

- **Script 安装(curl|bash)安全性**:UI 必须先展示完整命令,用户显式点击才执行;
  不静默跑。失败输出完整回显。
- **Script 类无法在本机验证全部**:cursor/kimi/qoder 未装,live 测试只验证命令构造 +
  一个可得的 npm 安装;Script 执行路径靠 UI 手动验证。
- **PATH login shell 开销**:首次约数百 ms,缓存后零成本;放启动预热。
- **npm 全局安装权限**:某些环境 `npm install -g` 需要 sudo。v1 不处理提权,失败时
  回显 npm 的错误(含权限提示),用户自行处理。
- **v1 不做**:卸载、自定义二进制路径、模型/环境变量配置、npm 镜像源选择、Hermes 自动安装。
