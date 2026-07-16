# Agent 管理中心 + PATH 修复 + 删除日志模块 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 ClawBox 新增统一的 Agent 管理页(12 条注册表、检测/安装/升级),修复 GUI 进程 PATH 探测缺陷,并删除无用的日志模块。

**Architecture:** 新增顶层 `path_env` 模块在启动时用 login+interactive shell 解析真实 PATH 并 `set_var` 注入进程环境(所有 `Command::new` 零改动受益);新增 `agents` 模块作为唯一 agent 注册表(现有 ACP adapters 改为从中筛选);命令层暴露 `agents_list`/`agent_install`;前端新 Agent 管理页接管「代理」入口,原审核页迁至新「审核」入口;日志模块前后端整体删除。

**Tech Stack:** Tauri v2 (Rust, edition 2021, lib 名 `clawbox_lib`)、Svelte 5 runes、TypeScript、svelte-i18n。

**Spec:** `docs/superpowers/specs/2026-07-16-agent-management-center-design.md`

## Global Constraints

- 项目根:`/Users/skyzhao/orca/workspaces/clawbox/palolo`;Rust 代码在 `src-tauri/`,**所有 cargo 命令必须在 `src-tauri/` 目录下执行**。
- 所有 Tauri 命令必须是 `pub async fn`(同步命令阻塞主线程冻结 UI,已有前科)。子进程探测/安装必须包 `tauri::async_runtime::spawn_blocking`。
- 前端页面加载态必须 `try { ... } finally { isLoading = false }`(spinner 卡死已有前科)。
- 注册表共 **12 条**,id 以 spec 第 1 节表格为唯一权威。
- Script 安装(curl|bash)UI 必须先展示完整命令,用户显式确认后才执行。
- v1 不做:卸载、自定义二进制路径、npm 镜像选择、Hermes 自动安装。
- 每个任务结束:`cargo test --lib` 全绿(基线 79 通过)+ 提交。涉及前端的任务再加 `npm run check`(基线 0 errors / 55 warnings)。
- Svelte 5 语法:`$state`/`$props`/`onclick`(不是 `on:click`)。

---

### Task 1: path_env 模块 — login shell 解析 PATH 并注入进程环境

**Files:**
- Create: `src-tauri/src/path_env.rs`
- Modify: `src-tauri/src/lib.rs`(模块声明 + `run()` 首行调用)

**Interfaces:**
- Produces: `clawbox_lib::path_env::init()`(启动时调用一次,幂等);`path_env::parse_marker_output(&str) -> Option<String>` 与 `path_env::merge_paths(&str, &str) -> String`(pub 供测试)。
- 后续任务**不需要**显式使用本模块——`init()` 通过 `std::env::set_var("PATH", ...)` 让所有 `Command::new` 自动继承修复后的 PATH。

**背景:** 从 Finder/Dock 启动的 GUI 进程 PATH 不含 nvm/homebrew/`~/.local/bin`,导致已装的 agent 探测为「未安装」。注意 `zsh -l -c` 只加载 `.zprofile` **不加载 `.zshrc`**(nvm 通常在 `.zshrc`),所以必须用 `-i -l` 交互式 login shell;交互式 shell 可能输出提示符等噪声,用 marker 行提取(VS Code 同款做法)。

- [ ] **Step 1: 写失败的单元测试**

创建 `src-tauri/src/path_env.rs`,先只放测试:

```rust
//! Resolve the user's real PATH from an interactive login shell.
//!
//! GUI processes launched from Finder/Dock inherit a minimal PATH that misses
//! nvm/homebrew/~/.local/bin, so binary probes false-negative. `init()` runs
//! `$SHELL -ilc 'echo <MARKER>$PATH'` once and injects the merged result via
//! `std::env::set_var("PATH", ..)` — every subsequent `Command::new` in the
//! process (probes, installs, ACP bridge spawns) inherits it with zero
//! call-site changes.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_extracts_marker_line_amid_noise() {
        let out = "welcome banner\n__CLAWBOX_PATH__/usr/bin:/opt/homebrew/bin\n";
        assert_eq!(
            parse_marker_output(out).as_deref(),
            Some("/usr/bin:/opt/homebrew/bin")
        );
    }

    #[test]
    fn parse_returns_none_without_marker_or_empty() {
        assert_eq!(parse_marker_output("no marker here"), None);
        assert_eq!(parse_marker_output("__CLAWBOX_PATH__\n"), None);
    }

    #[test]
    fn parse_takes_last_marker_line() {
        // .zshrc echoing the marker string is pathological but cheap to defend:
        // the real `echo` runs last.
        let out = "__CLAWBOX_PATH__stale\nprompt noise\n__CLAWBOX_PATH__/real/bin\n";
        assert_eq!(parse_marker_output(out).as_deref(), Some("/real/bin"));
    }

    #[test]
    fn merge_dedupes_and_keeps_shell_path_first() {
        let merged = merge_paths("/a:/b:/usr/bin", "/usr/bin:/c");
        assert_eq!(merged, "/a:/b:/usr/bin:/c");
    }

    #[test]
    fn merge_skips_empty_segments() {
        assert_eq!(merge_paths("/a::/b", ""), "/a:/b");
    }
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cd src-tauri && cargo test --lib path_env`
Expected: 编译错误 `cannot find function parse_marker_output`

- [ ] **Step 3: 实现**

在测试模块**上方**补齐实现:

```rust
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

const MARKER: &str = "__CLAWBOX_PATH__";
const SHELL_TIMEOUT: Duration = Duration::from_secs(5);

/// Extract the PATH from marker-prefixed shell output. Interactive shells may
/// print banners/prompt noise; the last marker line wins.
pub fn parse_marker_output(out: &str) -> Option<String> {
    out.lines()
        .rev()
        .find_map(|l| l.trim().strip_prefix(MARKER))
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
}

/// Shell PATH first, then any current-PATH entries not already present.
pub fn merge_paths(shell_path: &str, current: &str) -> String {
    let mut seen = Vec::new();
    for seg in shell_path.split(':').chain(current.split(':')) {
        if !seg.is_empty() && !seen.iter().any(|s| s == seg) {
            seen.push(seg.to_string());
        }
    }
    seen.join(":")
}

/// Poll-wait with a deadline; std has no built-in child timeout.
fn wait_with_timeout(mut child: std::process::Child, dur: Duration) -> Option<std::process::Output> {
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return child.wait_with_output().ok(),
            Ok(None) if start.elapsed() > dur => {
                let _ = child.kill();
                return None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(_) => return None,
        }
    }
}

fn shell_path() -> Option<String> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    // -i -l: interactive login shell so BOTH .zprofile and .zshrc run (nvm
    // typically lives in .zshrc; a plain `-lc` would miss it).
    let child = Command::new(&shell)
        .args(["-ilc", &format!("echo {}$PATH", MARKER)])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let out = wait_with_timeout(child, SHELL_TIMEOUT)?;
    parse_marker_output(&String::from_utf8_lossy(&out.stdout))
}

/// Resolve and inject the real PATH. Idempotent; call once at startup BEFORE
/// any threads spawn (edition-2021 `set_var` is safe but not thread-safe).
/// On any failure the current PATH is left untouched (fail-open: dev launches
/// from a terminal already have a good PATH).
pub fn init() {
    static DONE: OnceLock<()> = OnceLock::new();
    DONE.get_or_init(|| {
        if let Some(sp) = shell_path() {
            let current = std::env::var("PATH").unwrap_or_default();
            std::env::set_var("PATH", merge_paths(&sp, &current));
        }
    });
}
```

修改 `src-tauri/src/lib.rs`:模块声明区加 `pub mod path_env;`(与 `pub mod backends;` 并列),并在 `run()` 函数体**第一行**(`tauri::Builder::default()` 之前)加:

```rust
    // Must run before the builder spawns any threads: set_var is process-wide.
    path_env::init();
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cd src-tauri && cargo test --lib path_env`
Expected: 5 passed

Run: `cd src-tauri && cargo test --lib`
Expected: 84 passed(79 基线 + 5 新增),0 failed

- [ ] **Step 5: 手工验证 login shell 解析真实生效**

Run: `cd src-tauri && cargo test --lib path_env -- --nocapture` 通过后,再跑一个一次性验证(不留文件):

```bash
cd src-tauri && cat > /tmp/pe_check.rs <<'EOF'
fn main() {
    clawbox_lib::path_env::init();
    let p = std::env::var("PATH").unwrap();
    assert!(p.contains(".nvm") || p.contains("homebrew"), "PATH not enriched: {}", p);
    println!("OK PATH head: {}", &p[..p.len().min(120)]);
}
EOF
mkdir -p examples && cp /tmp/pe_check.rs examples/pe_check.rs && cargo run --example pe_check; rm examples/pe_check.rs
```

Expected: 输出 `OK PATH head: ...`(含 .nvm 或 homebrew 路径)

- [ ] **Step 6: 提交**

```bash
git add src-tauri/src/path_env.rs src-tauri/src/lib.rs
git commit -m "feat(path): resolve real PATH from interactive login shell at startup"
```

---

### Task 2: agents 统一注册表 + 状态检测

**Files:**
- Create: `src-tauri/src/agents/mod.rs`
- Modify: `src-tauri/src/lib.rs`(加 `pub mod agents;`)

**Interfaces:**
- Produces(后续任务依赖,签名逐字):
  - `pub struct AgentDef { pub id: &'static str, pub label: &'static str, pub binary: &'static str, pub kind: AgentKind, pub install: InstallMethod, pub check_probe: &'static [&'static str], pub depends_on: &'static [&'static str], pub docs_url: Option<&'static str> }`
  - `pub enum AgentKind { NativeCli, AcpBridge, Runtime, Gateway }`(derive `Serialize, Clone, Copy, PartialEq`,`#[serde(rename_all = "snake_case")]`)
  - `pub enum InstallMethod { Npm { package: &'static str, force: bool }, Script { url: &'static str }, PlatformPkg, DetectOnly }`
  - `pub fn agents() -> &'static [AgentDef]`、`pub fn find_agent(id: &str) -> Option<&'static AgentDef>`
  - `impl AgentDef { pub fn version(&self) -> Option<String>; pub fn is_installed(&self) -> bool; }`
  - `pub struct AgentStatus { pub id: String, pub label: String, pub kind: AgentKind, pub installed: bool, pub version: Option<String>, pub deps_satisfied: bool, pub missing_deps: Vec<String>, pub install_command: Option<String>, pub docs_url: Option<String> }`(derive `Serialize`)
  - `pub fn install_command_display(def: &AgentDef) -> Option<String>`
  - `pub fn list_agent_status() -> Vec<AgentStatus>`(内部 rayon 并行探测)

- [ ] **Step 1: 写失败的单元测试**

创建 `src-tauri/src/agents/mod.rs`,先只放测试:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_exactly_12_unique_entries() {
        let ids: Vec<_> = agents().iter().map(|a| a.id).collect();
        assert_eq!(ids.len(), 12);
        let mut dedup = ids.clone();
        dedup.sort();
        dedup.dedup();
        assert_eq!(dedup.len(), 12, "duplicate agent ids");
    }

    #[test]
    fn registry_covers_spec_table() {
        for id in [
            "node", "claude-code", "codex", "openclaw", "opencode", "codebuddy",
            "cursor-agent", "kimi", "qodercli", "claude-agent-acp", "codex-acp", "hermes",
        ] {
            assert!(find_agent(id).is_some(), "missing agent: {}", id);
        }
    }

    #[test]
    fn depends_on_references_are_valid_ids() {
        for a in agents() {
            for dep in a.depends_on {
                assert!(find_agent(dep).is_some(), "{} depends on unknown {}", a.id, dep);
            }
        }
    }

    #[test]
    fn bridge_depends_on_claude_code() {
        let bridge = find_agent("claude-agent-acp").unwrap();
        assert!(bridge.depends_on.contains(&"claude-code"));
        assert!(matches!(bridge.kind, AgentKind::AcpBridge));
    }

    #[test]
    fn hermes_is_detect_only_with_no_install_command() {
        let h = find_agent("hermes").unwrap();
        assert!(matches!(h.install, InstallMethod::DetectOnly));
        assert_eq!(install_command_display(h), None);
    }

    #[test]
    fn install_command_display_formats_each_method() {
        assert_eq!(
            install_command_display(find_agent("claude-code").unwrap()).as_deref(),
            Some("npm install -g @anthropic-ai/claude-code")
        );
        assert_eq!(
            install_command_display(find_agent("codex-acp").unwrap()).as_deref(),
            Some("npm install -g --force @agentclientprotocol/codex-acp")
        );
        assert_eq!(
            install_command_display(find_agent("cursor-agent").unwrap()).as_deref(),
            Some("curl -fsSL https://cursor.com/install | bash")
        );
        // node = platform package manager
        let node_cmd = install_command_display(find_agent("node").unwrap()).unwrap();
        assert!(node_cmd.contains("brew") || node_cmd.contains("winget"));
    }

    #[test]
    fn npm_agents_implicitly_require_node_in_status() {
        // Registry-level check: effective_deps injects "node" for Npm installs.
        let deps = effective_deps(find_agent("claude-code").unwrap());
        assert!(deps.contains(&"node"));
        // ...but node itself must not depend on node.
        assert!(effective_deps(find_agent("node").unwrap()).is_empty());
    }
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cd src-tauri && cargo test --lib agents`
Expected: 编译错误(`agents` 模块不存在于 lib)——先在 `src-tauri/src/lib.rs` 模块声明区加 `pub mod agents;` 再跑,得到 `cannot find function agents` 类编译错误。

- [ ] **Step 3: 实现注册表**

在测试模块上方补齐:

```rust
//! Unified agent registry — the single authority for every agent CLI ClawBox
//! can detect/install. Spec: docs/superpowers/specs/2026-07-16-agent-management-center-design.md
//! ACP bridge entries here replace the old acp/adapters.rs registry.

use serde::Serialize;
use std::process::Command;

#[derive(Serialize, Clone, Copy, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AgentKind {
    NativeCli,
    AcpBridge,
    Runtime,
    Gateway,
}

pub enum InstallMethod {
    Npm { package: &'static str, force: bool },
    Script { url: &'static str },
    /// Platform package manager (node: brew on macOS / winget on Windows).
    PlatformPkg,
    /// No auto-install; detection + docs link only (hermes: python venv).
    DetectOnly,
}

pub struct AgentDef {
    pub id: &'static str,
    pub label: &'static str,
    pub binary: &'static str,
    pub kind: AgentKind,
    pub install: InstallMethod,
    pub check_probe: &'static [&'static str],
    pub depends_on: &'static [&'static str],
    pub docs_url: Option<&'static str>,
}

static AGENTS: &[AgentDef] = &[
    AgentDef {
        id: "node", label: "Node.js", binary: "node",
        kind: AgentKind::Runtime, install: InstallMethod::PlatformPkg,
        check_probe: &["--version"], depends_on: &[],
        docs_url: Some("https://nodejs.org"),
    },
    AgentDef {
        id: "claude-code", label: "Claude Code", binary: "claude",
        kind: AgentKind::NativeCli,
        install: InstallMethod::Npm { package: "@anthropic-ai/claude-code", force: false },
        check_probe: &["--version"], depends_on: &[],
        docs_url: Some("https://docs.anthropic.com/en/docs/claude-code"),
    },
    AgentDef {
        id: "codex", label: "Codex", binary: "codex",
        kind: AgentKind::NativeCli,
        install: InstallMethod::Npm { package: "@openai/codex", force: false },
        check_probe: &["--version"], depends_on: &[],
        docs_url: Some("https://developers.openai.com/codex/cli"),
    },
    AgentDef {
        id: "openclaw", label: "OpenClaw", binary: "openclaw",
        kind: AgentKind::Gateway,
        install: InstallMethod::Npm { package: "openclaw", force: false },
        check_probe: &["--version"], depends_on: &[],
        docs_url: Some("https://www.npmjs.com/package/openclaw"),
    },
    AgentDef {
        id: "opencode", label: "OpenCode", binary: "opencode",
        kind: AgentKind::NativeCli,
        install: InstallMethod::Npm { package: "opencode-ai", force: false },
        check_probe: &["--version"], depends_on: &[],
        docs_url: Some("https://opencode.ai"),
    },
    AgentDef {
        id: "codebuddy", label: "CodeBuddy", binary: "codebuddy",
        kind: AgentKind::NativeCli,
        install: InstallMethod::Npm { package: "@tencent-ai/codebuddy-code", force: false },
        check_probe: &["--version"], depends_on: &[],
        docs_url: Some("https://www.codebuddy.ai"),
    },
    AgentDef {
        id: "cursor-agent", label: "Cursor", binary: "cursor-agent",
        kind: AgentKind::NativeCli,
        install: InstallMethod::Script { url: "https://cursor.com/install" },
        check_probe: &["--version"], depends_on: &[],
        docs_url: Some("https://cursor.com/cli"),
    },
    AgentDef {
        id: "kimi", label: "Kimi", binary: "kimi",
        kind: AgentKind::NativeCli,
        install: InstallMethod::Script { url: "https://code.kimi.com/kimi-code/install.sh" },
        check_probe: &["--version"], depends_on: &[],
        docs_url: Some("https://code.kimi.com"),
    },
    AgentDef {
        id: "qodercli", label: "Qoder", binary: "qodercli",
        kind: AgentKind::NativeCli,
        install: InstallMethod::Script { url: "https://qoder.com/install" },
        check_probe: &["--version"], depends_on: &[],
        docs_url: Some("https://qoder.com"),
    },
    AgentDef {
        id: "claude-agent-acp", label: "ClaudeCode ACP 桥", binary: "claude-agent-acp",
        kind: AgentKind::AcpBridge,
        install: InstallMethod::Npm { package: "@agentclientprotocol/claude-agent-acp", force: false },
        check_probe: &["--version"], depends_on: &["claude-code"],
        docs_url: Some("https://www.npmjs.com/package/@agentclientprotocol/claude-agent-acp"),
    },
    AgentDef {
        id: "codex-acp", label: "Codex ACP 桥", binary: "codex-acp",
        kind: AgentKind::AcpBridge,
        install: InstallMethod::Npm { package: "@agentclientprotocol/codex-acp", force: true },
        check_probe: &["--version"], depends_on: &[],
        docs_url: Some("https://www.npmjs.com/package/@agentclientprotocol/codex-acp"),
    },
    AgentDef {
        id: "hermes", label: "Hermes", binary: "hermes",
        kind: AgentKind::Gateway, install: InstallMethod::DetectOnly,
        check_probe: &["--version"], depends_on: &[],
        docs_url: None,
    },
];

pub fn agents() -> &'static [AgentDef] {
    AGENTS
}

pub fn find_agent(id: &str) -> Option<&'static AgentDef> {
    AGENTS.iter().find(|a| a.id == id)
}

impl AgentDef {
    /// Probe the binary; same contract as the old AcpAdapter::version.
    pub fn version(&self) -> Option<String> {
        let out = Command::new(self.binary).args(self.check_probe).output().ok()?;
        if !out.status.success() {
            return None;
        }
        let s = String::from_utf8_lossy(&out.stdout).trim().lines().next()?.to_string();
        if s.is_empty() { None } else { Some(s) }
    }

    pub fn is_installed(&self) -> bool {
        self.version().is_some()
    }
}

/// Explicit depends_on plus the implicit node dependency for Npm installs.
pub fn effective_deps(def: &AgentDef) -> Vec<&'static str> {
    let mut deps: Vec<&'static str> = def.depends_on.to_vec();
    if matches!(def.install, InstallMethod::Npm { .. }) && !deps.contains(&"node") {
        deps.push("node");
    }
    deps
}

/// Human-readable install command (shown in UI before the user confirms).
/// None for DetectOnly.
pub fn install_command_display(def: &AgentDef) -> Option<String> {
    match def.install {
        InstallMethod::Npm { package, force } => Some(if force {
            format!("npm install -g --force {}", package)
        } else {
            format!("npm install -g {}", package)
        }),
        InstallMethod::Script { url } => Some(format!("curl -fsSL {} | bash", url)),
        InstallMethod::PlatformPkg => Some(match std::env::consts::OS {
            "windows" => "winget install --id OpenJS.NodeJS.LTS --silent".to_string(),
            _ => "brew install node".to_string(),
        }),
        InstallMethod::DetectOnly => None,
    }
}

#[derive(Serialize)]
pub struct AgentStatus {
    pub id: String,
    pub label: String,
    pub kind: AgentKind,
    pub installed: bool,
    pub version: Option<String>,
    pub deps_satisfied: bool,
    pub missing_deps: Vec<String>,
    pub install_command: Option<String>,
    pub docs_url: Option<String>,
}

/// Probe every agent in parallel (each probe shells out, ~0.1-1s serial cost).
pub fn list_agent_status() -> Vec<AgentStatus> {
    use rayon::prelude::*;
    // Probe installed-state for all 12 first so dep checks reuse results.
    let installed: std::collections::HashMap<&str, Option<String>> = AGENTS
        .par_iter()
        .map(|a| (a.id, a.version()))
        .collect();
    AGENTS
        .iter()
        .map(|a| {
            let version = installed.get(a.id).cloned().flatten();
            let missing: Vec<String> = effective_deps(a)
                .into_iter()
                .filter(|dep| installed.get(*dep).map(|v| v.is_none()).unwrap_or(true))
                .map(|s| s.to_string())
                .collect();
            AgentStatus {
                id: a.id.to_string(),
                label: a.label.to_string(),
                kind: a.kind,
                installed: version.is_some(),
                version,
                deps_satisfied: missing.is_empty(),
                missing_deps: missing,
                install_command: install_command_display(a),
                docs_url: a.docs_url.map(|s| s.to_string()),
            }
        })
        .collect()
}
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cd src-tauri && cargo test --lib agents`
Expected: 7 passed

Run: `cd src-tauri && cargo test --lib`
Expected: 91 passed(84 + 7),0 failed

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/agents/mod.rs src-tauri/src/lib.rs
git commit -m "feat(agents): unified 12-entry agent registry with status detection"
```

---

### Task 3: ACP adapters 并入注册表 + 安装执行逻辑

**Files:**
- Create: `src-tauri/src/agents/install.rs`
- Modify: `src-tauri/src/agents/mod.rs`(加 `pub mod install;` 一行,置于文件顶部 use 之后)
- Modify: `src-tauri/src/acp/adapters.rs`(全文重写,从注册表筛选)
- Modify: `src-tauri/src/acp/session.rs:79` 附近(字段类型 String 适配)
- Modify: `src-tauri/src/commands/acp.rs`(`acp_install_adapter` 改走统一安装逻辑)

**Interfaces:**
- Consumes: Task 2 的 `AgentDef`/`InstallMethod`/`find_agent`/`install_command_display`。
- Produces:
  - `agents::install::build_install_args(def: &AgentDef) -> Result<(String, Vec<String>), String>`(纯函数,可单测;`PlatformPkg`/`DetectOnly` 返回 Err)
  - `agents::install::run_install(def: &AgentDef) -> Result<String, String>`(阻塞执行,调用方负责 spawn_blocking)
  - `acp::adapters::AcpAdapter` 字段改为 `String`/`Vec<String>` 类型,`adapters()`/`find_adapter()`/`list_adapter_info()`/`AdapterInfo` 签名不变(返回引用仍 `&'static`,内部 `OnceLock<Vec<_>>`)。

- [ ] **Step 1: 写失败的安装参数构造测试**

创建 `src-tauri/src/agents/install.rs`:

```rust
//! Install execution for registry agents. Pure command construction is split
//! from execution so it can be unit-tested without touching the system.

use super::{AgentDef, InstallMethod};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::find_agent;

    #[test]
    fn npm_install_args() {
        let (cmd, args) = build_install_args(find_agent("claude-code").unwrap()).unwrap();
        assert_eq!(cmd, "npm");
        assert_eq!(args, vec!["install", "-g", "@anthropic-ai/claude-code"]);
    }

    #[test]
    fn npm_force_install_args() {
        let (cmd, args) = build_install_args(find_agent("codex-acp").unwrap()).unwrap();
        assert_eq!(cmd, "npm");
        assert_eq!(args, vec!["install", "-g", "--force", "@agentclientprotocol/codex-acp"]);
    }

    #[test]
    fn script_install_pipes_curl_to_bash() {
        let (cmd, args) = build_install_args(find_agent("kimi").unwrap()).unwrap();
        assert_eq!(cmd, "bash");
        assert_eq!(args, vec![
            "-c".to_string(),
            "curl -fsSL https://code.kimi.com/kimi-code/install.sh | bash".to_string(),
        ]);
    }

    #[test]
    fn platform_pkg_and_detect_only_are_not_buildable() {
        assert!(build_install_args(find_agent("node").unwrap()).is_err());
        assert!(build_install_args(find_agent("hermes").unwrap()).is_err());
    }
}
```

- [ ] **Step 2: 运行确认失败**

先在 `src-tauri/src/agents/mod.rs` 顶部(`use serde::Serialize;` 之前)加一行 `pub mod install;`。

Run: `cd src-tauri && cargo test --lib agents::install`
Expected: 编译错误 `cannot find function build_install_args`

- [ ] **Step 3: 实现**

在 `install.rs` 测试模块上方补齐:

```rust
pub fn build_install_args(def: &AgentDef) -> Result<(String, Vec<String>), String> {
    match def.install {
        InstallMethod::Npm { package, force } => {
            let mut args = vec!["install".to_string(), "-g".to_string()];
            if force {
                args.push("--force".to_string());
            }
            args.push(package.to_string());
            Ok(("npm".to_string(), args))
        }
        InstallMethod::Script { url } => Ok((
            "bash".to_string(),
            vec!["-c".to_string(), format!("curl -fsSL {} | bash", url)],
        )),
        InstallMethod::PlatformPkg => Err(format!(
            "{} installs via the platform package manager (handled by install_nodejs)",
            def.id
        )),
        InstallMethod::DetectOnly => Err(format!(
            "{} cannot be auto-installed; install it manually",
            def.id
        )),
    }
}

/// Blocking install; callers wrap in spawn_blocking. PATH was already fixed
/// process-wide by path_env::init(), so npm/bash resolve like a user shell.
pub fn run_install(def: &AgentDef) -> Result<String, String> {
    let (cmd, args) = build_install_args(def)?;
    let out = std::process::Command::new(&cmd)
        .args(&args)
        .output()
        .map_err(|e| format!("failed to run {}: {}", cmd, e))?;
    if out.status.success() {
        Ok(format!("Installed {}", def.label))
    } else {
        // npm/installer errors (incl. EACCES permission hints) go back verbatim.
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}
```

Run: `cd src-tauri && cargo test --lib agents`
Expected: 11 passed(7 + 4)

- [ ] **Step 4: 重写 adapters.rs 从注册表筛选**

`src-tauri/src/acp/adapters.rs` 全文替换为:

```rust
//! Adapter registry — ACP bridges, now sourced from the unified agent
//! registry (crate::agents) so bridges are registered exactly once.

use crate::agents::{self, install_command_display, AgentKind};
use serde::Serialize;
use std::sync::OnceLock;

pub struct AcpAdapter {
    pub id: String,
    pub label: String,
    pub binary: String,
    pub install_hint: String,
    pub check_probe: Vec<String>,
}

impl AcpAdapter {
    fn def(&self) -> &'static agents::AgentDef {
        // Invariant: every AcpAdapter is built from a registry entry.
        agents::find_agent(&self.id).expect("adapter id present in agent registry")
    }

    pub fn is_installed(&self) -> bool {
        self.def().is_installed()
    }

    pub fn version(&self) -> Option<String> {
        self.def().version()
    }
}

static ADAPTERS: OnceLock<Vec<AcpAdapter>> = OnceLock::new();

pub fn adapters() -> &'static [AcpAdapter] {
    ADAPTERS.get_or_init(|| {
        agents::agents()
            .iter()
            .filter(|a| a.kind == AgentKind::AcpBridge)
            .map(|a| AcpAdapter {
                id: a.id.to_string(),
                label: a.label.to_string(),
                binary: a.binary.to_string(),
                install_hint: install_command_display(a)
                    .expect("bridges are npm-installable"),
                check_probe: a.check_probe.iter().map(|s| s.to_string()).collect(),
            })
            .collect()
    })
}

pub fn find_adapter(id: &str) -> Option<&'static AcpAdapter> {
    adapters().iter().find(|a| a.id == id)
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
    adapters()
        .iter()
        .map(|a| {
            let version = a.version();
            AdapterInfo {
                id: a.id.clone(),
                label: a.label.clone(),
                installed: version.is_some(),
                version,
                install_hint: a.install_hint.clone(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_claude_and_codex() {
        let ids: Vec<_> = adapters().iter().map(|a| a.id.as_str()).collect();
        assert!(ids.contains(&"claude-agent-acp"));
        assert!(ids.contains(&"codex-acp"));
        assert_eq!(ids.len(), 2, "exactly the bridge entries from the registry");
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

    #[test]
    fn install_hint_comes_from_registry() {
        assert_eq!(
            find_adapter("codex-acp").unwrap().install_hint,
            "npm install -g --force @agentclientprotocol/codex-acp"
        );
    }
}
```

- [ ] **Step 5: 适配两个消费点**

`src-tauri/src/acp/session.rs:81`(`AcpSession::start` 内)现为:

```rust
        let (client, inbound) = JsonRpcClient::spawn(adapter.binary, &[], cwd).await?;
```

`adapter.binary` 由 `&'static str` 变 `String`,改为:

```rust
        let (client, inbound) = JsonRpcClient::spawn(&adapter.binary, &[], cwd).await?;
```

(`JsonRpcClient::spawn(binary: &str, ...)` 签名不变;若还有其他编译错误按同样模式加借用。)

`src-tauri/src/commands/acp.rs` 的 `acp_install_adapter` 整体替换为走统一安装逻辑(删除 install_hint 字符串解析 hack 和文件顶部 `use std::process::Command;`):

```rust
#[tauri::command(async)]
pub async fn acp_install_adapter(id: String) -> Result<String, String> {
    // `npm install -g` can take minutes; keep it off the main thread.
    tauri::async_runtime::spawn_blocking(move || {
        let def = crate::agents::find_agent(&id)
            .filter(|a| a.kind == crate::agents::AgentKind::AcpBridge)
            .ok_or_else(|| format!("unknown adapter: {}", id))?;
        crate::agents::install::run_install(def)
    })
    .await
    .map_err(|e| e.to_string())?
}
```

- [ ] **Step 6: 全量测试**

Run: `cd src-tauri && cargo test --lib`
Expected: 96 passed(91 + 4 install + 1 新增 adapters 测试;原 adapters 3 个保留),0 failed。若数字有出入,以「无 failed、新增测试都在」为准。

Run: `cd src-tauri && cargo test --test smoke -- --list`
Expected: 正常列出(编译通过)

- [ ] **Step 7: 提交**

```bash
git add src-tauri/src/agents/ src-tauri/src/acp/adapters.rs src-tauri/src/acp/session.rs src-tauri/src/commands/acp.rs
git commit -m "refactor(acp): source bridge adapters from unified agent registry"
```

---

### Task 4: agents 命令层(agents_list / agent_install)

**Files:**
- Create: `src-tauri/src/commands/agents.rs`
- Modify: `src-tauri/src/commands/mod.rs`(加 `pub mod agents;`)
- Modify: `src-tauri/src/lib.rs`(generate_handler 注册 2 个命令)

**Interfaces:**
- Consumes: Task 2 `list_agent_status`/`AgentStatus`/`find_agent`;Task 3 `agents::install::run_install`;现有 `commands::install::install_nodejs`。
- Produces(前端 Task 6 依赖):invoke 命令 `agents_list() -> AgentStatus[]`、`agent_install(id: string) -> string`(错误为 string)。

- [ ] **Step 1: 创建命令文件**

`src-tauri/src/commands/agents.rs`:

```rust
//! Tauri command layer for the unified agent registry.

use crate::agents::{self, AgentStatus, InstallMethod};

#[tauri::command]
pub async fn agents_list() -> Vec<AgentStatus> {
    // 12 parallel binary probes — keep them off the main thread.
    tauri::async_runtime::spawn_blocking(agents::list_agent_status)
        .await
        .unwrap_or_default()
}

#[tauri::command]
pub async fn agent_install(id: String) -> Result<String, String> {
    let def = agents::find_agent(&id).ok_or_else(|| format!("unknown agent: {}", id))?;
    match def.install {
        // node reuses the existing brew/winget flow.
        InstallMethod::PlatformPkg => super::install::install_nodejs().await,
        InstallMethod::DetectOnly => Err(format!(
            "{} must be installed manually",
            def.label
        )),
        _ => tauri::async_runtime::spawn_blocking(move || agents::install::run_install(def))
            .await
            .map_err(|e| e.to_string())?,
    }
}
```

注:`def` 是 `&'static AgentDef`,move 进 spawn_blocking 无生命周期问题。

- [ ] **Step 2: 注册**

`src-tauri/src/commands/mod.rs` 加 `pub mod agents;`(按字母序放在 `pub mod acp;` 之后)。

`src-tauri/src/lib.rs` 的 `generate_handler![` 列表中(`commands::acp::acp_list_adapters,` 之前)加:

```rust
            commands::agents::agents_list,
            commands::agents::agent_install,
```

- [ ] **Step 3: 编译 + 全量测试**

Run: `cd src-tauri && cargo build && cargo test --lib`
Expected: build 无 error;测试全绿(与 Task 3 结束时同数)

- [ ] **Step 4: 提交**

```bash
git add src-tauri/src/commands/agents.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs
git commit -m "feat(commands): agents_list + agent_install commands"
```

---

### Task 5: 删除日志模块(前后端 + i18n)

**Files:**
- Delete: `src-tauri/src/commands/logs.rs`、`src/lib/api/logs.ts`、`src/routes/logs/+page.svelte`
- Modify: `src-tauri/src/commands/mod.rs`(删 `pub mod logs;`)
- Modify: `src-tauri/src/lib.rs`(删 `commands::logs::get_log_files,` 与 `commands::logs::get_log_content,` 两行)
- Modify: `src/App.svelte`(删 line 7 `import LogsPage ...`;删 `{:else if currentPage === 'logs'}` 与其下 `<LogsPage />` 两行)
- Modify: `src/lib/components/Sidebar.svelte`(navItems 删 `{ id: 'logs', label: 'Logs' },`;删 `{:else if item.id === 'logs'}` 的 svg 分支块;`t()` 回退 labels map 删 `logs: 'Logs',`)
- Modify: `src/lib/i18n/en.json`、`src/lib/i18n/zh.json`(删 `nav.logs` 键与顶层 `logs` 节)

**Interfaces:**
- Consumes: 无。Produces: 无(纯删除)。

- [ ] **Step 1: 删除文件与引用**

```bash
git rm src-tauri/src/commands/logs.rs src/lib/api/logs.ts src/routes/logs/+page.svelte
```

然后按上面 Files 清单逐一删除引用行。i18n 删除用编辑器/jq 均可,注意保持 JSON 合法(逗号)。

- [ ] **Step 2: 确认无残留引用**

Run: `grep -rn "LogsPage\|get_log_files\|get_log_content\|'logs'\|\"logs\"" src src-tauri/src | grep -v node_modules`
Expected: 无输出(或仅剩与日志模块无关的匹配——逐一确认后放行)

- [ ] **Step 3: 全量验证**

Run: `cd src-tauri && cargo build && cargo test --lib`
Expected: 无 error,测试全绿

Run: `npm run check`
Expected: 0 errors(warnings 可能比基线 55 略少——日志页自身的 warnings 消失)

- [ ] **Step 4: 提交**

```bash
git add -A
git commit -m "feat(ui)!: remove unused logs module (backend commands + page + i18n)"
```

---

### Task 6: 前端 — Agent 管理页 + 导航理顺 + i18n

**Files:**
- Move: `src/routes/agents/+page.svelte` → `src/routes/review/+page.svelte`(`git mv src/routes/agents src/routes/review`)
- Create: `src/lib/api/agents.ts`
- Create: `src/routes/agents/+page.svelte`(新管理页)
- Modify: `src/App.svelte`(import 改路径 + 加 review 路由分支)
- Modify: `src/lib/components/Sidebar.svelte`(navItems 加 review 项 + 图标)
- Modify: `src/lib/i18n/en.json`、`src/lib/i18n/zh.json`(`nav.review`、`agents.*` 新节;原 review.* 节保留不动)

**Interfaces:**
- Consumes: Task 4 的 invoke 命令 `agents_list`/`agent_install`。
- Produces: `src/lib/api/agents.ts` 导出 `AgentStatus` 接口与 `agents_list()`/`agent_install(id)`。

- [ ] **Step 1: 迁移审核页**

```bash
git mv src/routes/agents src/routes/review
```

`src/App.svelte`:
- line 9 `import AgentsPage from './routes/agents/+page.svelte';` 改为两行:

```ts
  import ReviewPage from './routes/review/+page.svelte';
  import AgentHubPage from './routes/agents/+page.svelte';
```

- 路由块 `{:else if currentPage === 'agents'}` 下的 `<AgentsPage />` 改为 `<AgentHubPage />`,并紧随其后新增:

```svelte
      {:else if currentPage === 'review'}
        <ReviewPage />
```

- [ ] **Step 2: api/agents.ts**

```ts
import { invoke } from '@tauri-apps/api/core';

export type AgentKind = 'native_cli' | 'acp_bridge' | 'runtime' | 'gateway';

export interface AgentStatus {
  id: string;
  label: string;
  kind: AgentKind;
  installed: boolean;
  version: string | null;
  deps_satisfied: boolean;
  missing_deps: string[];
  install_command: string | null;
  docs_url: string | null;
}

export function agents_list(): Promise<AgentStatus[]> {
  return invoke<AgentStatus[]>('agents_list');
}

export function agent_install(id: string): Promise<string> {
  return invoke<string>('agent_install', { id });
}
```

- [ ] **Step 3: 新管理页**

`src/routes/agents/+page.svelte`(样式沿用项目 glass-card 风格;结构完整如下):

```svelte
<script lang="ts">
  import { onMount } from 'svelte';
  import { _ } from 'svelte-i18n';
  import { agents_list, agent_install, type AgentStatus } from '../../lib/api/agents';

  let agents = $state<AgentStatus[]>([]);
  let isLoading = $state(true);
  let installing = $state<Record<string, boolean>>({});
  let confirming = $state<string | null>(null); // script 类两步确认:当前展开确认的 agent id
  let errors = $state<Record<string, string>>({});

  async function refresh() {
    isLoading = true;
    try {
      agents = await agents_list();
    } catch (e) {
      console.error('agents_list failed', e);
    } finally {
      isLoading = false;
    }
  }

  onMount(refresh);

  function isScript(a: AgentStatus): boolean {
    return a.install_command?.startsWith('curl') ?? false;
  }

  async function install(a: AgentStatus) {
    // Script 安装第一击只展开命令确认,第二击才执行
    if (isScript(a) && confirming !== a.id) {
      confirming = a.id;
      return;
    }
    confirming = null;
    installing = { ...installing, [a.id]: true };
    errors = { ...errors, [a.id]: '' };
    try {
      await agent_install(a.id);
      await refresh();
    } catch (e) {
      errors = { ...errors, [a.id]: String(e) };
    } finally {
      installing = { ...installing, [a.id]: false };
    }
  }

  function kindLabel(k: AgentStatus['kind']): string {
    return $_(`agents.kind.${k}`);
  }
</script>

<div class="agents-page">
  <header class="page-header">
    <h1>{$_('agents.title')}</h1>
    <p class="subtitle">{$_('agents.subtitle')}</p>
    <button class="refresh-btn" onclick={refresh} disabled={isLoading}>
      {$_('agents.refresh')}
    </button>
  </header>

  {#if isLoading && agents.length === 0}
    <div class="loading glass-card"><span class="spinner"></span> {$_('agents.loading')}</div>
  {:else}
    <div class="agent-list">
      {#each agents as a (a.id)}
        <div class="glass-card agent-row">
          <div class="agent-main">
            <span class="agent-label">{a.label}</span>
            <span class="kind-badge kind-{a.kind}">{kindLabel(a.kind)}</span>
            {#if a.installed}
              <span class="version">{a.version}</span>
            {:else}
              <span class="not-installed">{$_('agents.notInstalled')}</span>
            {/if}
          </div>
          <div class="agent-actions">
            {#if a.install_command}
              <code class="install-cmd">{a.install_command}</code>
              {#if !a.deps_satisfied}
                <span class="deps-hint">{$_('agents.missingDeps')}: {a.missing_deps.join(', ')}</span>
              {:else if confirming === a.id}
                <button class="btn danger" onclick={() => install(a)}>
                  {$_('agents.confirmRun')}
                </button>
                <button class="btn" onclick={() => (confirming = null)}>{$_('agents.cancel')}</button>
              {:else}
                <button class="btn primary" onclick={() => install(a)} disabled={installing[a.id]}>
                  {#if installing[a.id]}
                    <span class="spinner small"></span>
                  {:else}
                    {a.installed ? $_('agents.upgrade') : $_('agents.install')}
                  {/if}
                </button>
              {/if}
            {:else}
              <span class="detect-only">{$_('agents.detectOnly')}</span>
            {/if}
            {#if a.docs_url}
              <a class="docs-link" href={a.docs_url} target="_blank" rel="noreferrer">{$_('agents.docs')}</a>
            {/if}
          </div>
          {#if errors[a.id]}
            <pre class="install-error">{errors[a.id]}</pre>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .agents-page { padding: 1.5rem; display: flex; flex-direction: column; gap: 1rem; }
  .page-header { display: flex; align-items: baseline; gap: 1rem; }
  .page-header h1 { margin: 0; }
  .subtitle { opacity: 0.6; flex: 1; }
  .agent-list { display: flex; flex-direction: column; gap: 0.75rem; }
  .agent-row { padding: 1rem; display: flex; flex-direction: column; gap: 0.5rem; }
  .agent-main { display: flex; align-items: center; gap: 0.75rem; }
  .agent-label { font-weight: 600; }
  .kind-badge { font-size: 0.7rem; padding: 0.1rem 0.5rem; border-radius: 999px; background: rgba(94, 234, 212, 0.15); color: #5eead4; }
  .version { font-family: monospace; font-size: 0.8rem; opacity: 0.7; }
  .not-installed { font-size: 0.8rem; color: #fbbf24; }
  .agent-actions { display: flex; align-items: center; gap: 0.75rem; flex-wrap: wrap; }
  .install-cmd { font-size: 0.75rem; opacity: 0.55; }
  .deps-hint { font-size: 0.75rem; color: #fbbf24; }
  .detect-only { font-size: 0.8rem; opacity: 0.5; }
  .docs-link { font-size: 0.8rem; color: #5eead4; }
  .install-error { font-size: 0.75rem; color: #f87171; white-space: pre-wrap; margin: 0; }
  .btn { padding: 0.3rem 0.9rem; border-radius: 6px; border: 1px solid rgba(255,255,255,0.15); background: transparent; color: inherit; cursor: pointer; }
  .btn.primary { background: rgba(94, 234, 212, 0.15); border-color: #5eead4; color: #5eead4; }
  .btn.danger { background: rgba(248, 113, 113, 0.15); border-color: #f87171; color: #f87171; }
  .btn:disabled { opacity: 0.5; cursor: default; }
  .loading { padding: 2rem; display: flex; justify-content: center; gap: 0.5rem; }
  .spinner { width: 16px; height: 16px; border: 2px solid rgba(94,234,212,0.3); border-top-color: #5eead4; border-radius: 50%; animation: spin 0.8s linear infinite; display: inline-block; }
  .spinner.small { width: 12px; height: 12px; }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
```

- [ ] **Step 4: Sidebar + i18n**

`Sidebar.svelte`:
- navItems 在 `{ id: 'agents', ... }` 后加 `{ id: 'review', label: 'Review' },`
- 图标分支:在 agents 图标块后加(放大镜+勾图标):

```svelte
        {:else if item.id === 'review'}
          <svg class="nav-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <circle cx="11" cy="11" r="8"/>
            <line x1="21" y1="21" x2="16.65" y2="16.65"/>
            <polyline points="8 11 10.5 13.5 14.5 9"/>
          </svg>
```

- `t()` 回退 labels map 加 `review: 'Review',`

i18n:两个 json 的 `nav` 节加 `"review": "Review"` / `"review": "审核"`;新增顶层 `agents` 节:

en.json:

```json
"agents": {
  "title": "Agents",
  "subtitle": "Detect, install and upgrade agent CLIs",
  "refresh": "Refresh",
  "loading": "Detecting agents...",
  "notInstalled": "Not installed",
  "install": "Install",
  "upgrade": "Upgrade",
  "confirmRun": "Run command",
  "cancel": "Cancel",
  "missingDeps": "Requires",
  "detectOnly": "Manual install only",
  "docs": "Docs",
  "kind": {
    "native_cli": "CLI",
    "acp_bridge": "ACP Bridge",
    "runtime": "Runtime",
    "gateway": "Gateway"
  }
}
```

zh.json:

```json
"agents": {
  "title": "Agent 管理",
  "subtitle": "检测、安装与升级 agent CLI",
  "refresh": "刷新",
  "loading": "正在检测 agent...",
  "notInstalled": "未安装",
  "install": "安装",
  "upgrade": "升级",
  "confirmRun": "执行命令",
  "cancel": "取消",
  "missingDeps": "依赖",
  "detectOnly": "仅支持手动安装",
  "docs": "文档",
  "kind": {
    "native_cli": "CLI",
    "acp_bridge": "ACP 桥",
    "runtime": "运行时",
    "gateway": "网关"
  }
}
```

注意:若 json 已有顶层 `agents` 节(旧审核页可能占用)——实际检查:审核页用的是 `review.*` 节,`agents` 节应不存在;若存在冲突键,保留双方所有键合并。

- [ ] **Step 5: 验证**

Run: `npm run check`
Expected: 0 errors

Run: `cd src-tauri && cargo build`
Expected: 无 error(前端改动不影响,但确认整树可编译)

- [ ] **Step 6: 提交**

```bash
git add -A
git commit -m "feat(ui): agent management hub page; move ACP review to its own nav entry"
```

---

### Task 7: live smoke 检测 + 收尾全量验证

**Files:**
- Modify: `src-tauri/tests/smoke.rs`(追加 agents 检测 smoke 测试)

**Interfaces:**
- Consumes: `clawbox_lib::agents::{agents, list_agent_status}`、`clawbox_lib::path_env::init`。

- [ ] **Step 1: 追加 smoke 测试**

在 `src-tauri/tests/smoke.rs` 末尾追加:

```rust
// ---- unified agent registry smoke (real binaries on this host) ----

#[test]
fn agent_registry_detection_is_consistent_with_direct_probes() {
    clawbox_lib::path_env::init();
    let statuses = clawbox_lib::agents::list_agent_status();
    assert_eq!(statuses.len(), 12);

    // Every status must agree with probing the binary directly.
    for s in &statuses {
        let def = clawbox_lib::agents::find_agent(&s.id).unwrap();
        let direct = std::process::Command::new(def.binary)
            .args(def.check_probe)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        assert_eq!(
            s.installed, direct,
            "detection mismatch for {} (status={}, direct={})",
            s.id, s.installed, direct
        );
    }
}

#[test]
fn node_is_detected_when_present() {
    // node is a hard prerequisite of this repo's own toolchain; if the dev
    // machine has it, the registry must see it (PATH fix regression guard).
    clawbox_lib::path_env::init();
    let has_node = std::process::Command::new("node")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !has_node {
        eprintln!("skip: node not on this host");
        return;
    }
    let statuses = clawbox_lib::agents::list_agent_status();
    let node = statuses.iter().find(|s| s.id == "node").unwrap();
    assert!(node.installed, "registry failed to detect node");
    assert!(node.version.is_some());
}

#[test]
fn npm_package_names_exist_on_registry() {
    // Guards against typo'd package names in the agent registry: `npm view`
    // resolves each against the live npm registry. Needs network + npm;
    // skipped when npm is absent (matches this file's skip convention).
    let npm_ok = std::process::Command::new("npm")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !npm_ok {
        eprintln!("skip: npm not on this host");
        return;
    }
    for def in clawbox_lib::agents::agents() {
        if let clawbox_lib::agents::InstallMethod::Npm { package, .. } = def.install {
            let out = std::process::Command::new("npm")
                .args(["view", package, "version"])
                .output()
                .expect("npm view runs");
            assert!(
                out.status.success(),
                "npm package for {} not found: {}",
                def.id,
                package
            );
        }
    }
}

#[test]
#[ignore = "mutates global npm state; run explicitly: cargo test --test smoke -- --ignored"]
fn gated_npm_install_is_idempotent() {
    // Reinstall an already-installed bridge (idempotent upgrade path) to
    // exercise run_install end-to-end. Ignored by default: it hits the
    // network and rewrites the global npm bin links.
    clawbox_lib::path_env::init();
    let def = clawbox_lib::agents::find_agent("claude-agent-acp").unwrap();
    if !def.is_installed() {
        eprintln!("skip: claude-agent-acp not installed; not installing fresh");
        return;
    }
    let result = clawbox_lib::agents::install::run_install(def);
    assert!(result.is_ok(), "reinstall failed: {:?}", result.err());
    assert!(def.is_installed(), "binary vanished after reinstall");
}
```

注:`InstallMethod` 需要能被 smoke 测试匹配——它已是 `pub enum`(Task 2),无需改动。

- [ ] **Step 2: 运行 smoke**

Run: `cd src-tauri && cargo test --test smoke`
Expected: 全绿,1 ignored(本机:node/claude/codex/openclaw/opencode/hermes 检出已装,cursor-agent/kimi/qodercli/codebuddy 检出未装,一致性断言通过;npm view 验证 5 个 Npm 包名真实存在)

Run: `cd src-tauri && cargo test --test smoke -- --ignored`
Expected: gated 重装测试通过(claude-agent-acp 幂等重装,binary 仍在)

- [ ] **Step 3: 全量收尾验证**

```bash
cd src-tauri && cargo test --lib && cargo test --test smoke
cd .. && npm run check
```

Expected: 全绿 / 0 errors

- [ ] **Step 4: 提交**

```bash
git add src-tauri/tests/smoke.rs
git commit -m "test(smoke): agent registry detection against real host binaries"
```

---

## 计划外验收(执行完成后由主会话完成,不派 subagent)

1. `npm run tauri dev` 启动真实窗口,computer-use 验证:侧边栏无「日志」、有「代理」与「审核」;代理页 12 行、已装/未装状态与本机一致(PATH 修复后 GUI 进程应能看到 nvm 下的 claude 等);审核页功能不回退。
2. 若「代理」页在 GUI 里检出数与终端一致 → PATH 修复达成(此前 GUI 进程会漏检 nvm/~/.local/bin 下的二进制)。
