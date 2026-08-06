<div align="center">
  <img src="src-tauri/icons/icon.png" width="96" alt="ClawBox logo" />
  <h1>ClawBox</h1>
  <p><strong>AI Agent 统一配置中心</strong></p>
  <p>服务商、MCP、技能、记忆一处管理，一键下发到所有 agent。</p>
  <p>
    <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="MIT License" /></a>
    <a href="https://github.com/zhaolianghz/clawbox/releases"><img src="https://img.shields.io/github/v/release/zhaolianghz/clawbox" alt="最新版本" /></a>
    <a href="https://github.com/zhaolianghz/clawbox/issues"><img src="https://img.shields.io/github/issues/zhaolianghz/clawbox" alt="Issues" /></a>
  </p>
  <p>
    <a href="README.md">English</a> · <a href="README.zh.md">中文</a>
  </p>
</div>

---

## 什么是 ClawBox？

ClawBox 是一款桌面应用（macOS · Windows · Linux），为你的所有 AI 编程 agent 提供统一的控制面板——Claude Code、Codex、Hermes、OpenCode、OpenClaw、Kimi、CodeBuddy 等。

不再需要在五个不同目录里手动编辑配置文件，在 ClawBox 里配置一次，同步到所有 agent。

## 界面截图

![服务商](docs/screenshots/providers.png)

| MCP | 技能 |
|---|---|
| ![MCP](docs/screenshots/mcp.png) | ![技能](docs/screenshots/skills.png) |
| **记忆** | **Agent 管理** |
| ![记忆](docs/screenshots/memory.png) | ![Agent 管理](docs/screenshots/agents.png) |

## 功能

| 模块 | 功能说明 |
|---|---|
| **服务商** | 为任意 OpenAI 或 Anthropic 兼容服务商添加 API Key 和端点（内置 78 家，每家支持双端点）。每个 agent 独立选择服务商，选中即生效，编辑自动重新下发。 |
| **MCP** | 可视化编辑 MCP 服务器（表单或原始 JSON）。同步到所有支持 MCP 的 agent。内置 8 个精选服务器快速上手。 |
| **技能** | 统一技能库（`~/.agents/skills/`）。从 Git 仓库安装（Anthropic Skills、Superpowers 等），收编各 agent 现有技能，通过软链同步下发。 |
| **记忆** | 编辑统一的 `~/.agents/memory/MEMORY.md`，以托管区块形式注入每个 agent 的指令文件——区块外内容一字不动。 |
| **Agent 管理** | 在一个界面安装、升级、查看所有 AI CLI agent。 |

## 服务商兑底与漂移处理

**兑底链（Hermes）。** 给 agent 绑定一个主服务商 + 有序的兑底链。主服务商限流或报错时,Hermes 自动切到下一个——不需要网关、不常驻后台进程。拖拽调整优先级。其它 agent 运行时是单端点的;对它们而言“兑底”= 指向一个网关服务商。

**漂移处理（所有 agent）。** 当 agent 的配置文件与 ClawBox 管理的不一致（你手改了、或别的工具改了),ClawBox 绝不无声覆盖。而是用人话提示,给出两个一键动作:

![漂移处理](docs/screenshots/fallback-drift.png)

- **恢复** — 推回 ClawBox 的值（安全默认)。
- **就用现在的** — 把 agent 当前的值领养进 ClawBox（反向同步)。

没有 diff 表格或字段名——只显示服务商名和两个按钮。顶部还有「全部恢复」一键化解所有漂移 agent。

**从 agent 导入。** 把任意 agent 当前在用的服务商直接抓进 ClawBox（同步详情里的「从 agent 导入」)——适合你手动配过 agent 后想让 ClawBox 接管的场景。

## 支持的 Agent

| Agent | 服务商 | MCP | 技能 | 记忆 | 兑底链 |
|---|---|---|---|---|---|
| Claude Code | ✅ | ✅ | ✅ | ✅ | — |
| Codex | ✅ | ✅ | — | ✅ | — |
| Hermes | ✅ | ✅ | ✅ | ✅ | ✅ |
| OpenCode | ✅ | ✅ | ✅ | ✅ | — |
| OpenClaw | ✅ | ✅ | ✅ | ✅ | — |
| Kimi | ✅ | — | — | — | — |
| CodeBuddy | ✅ | ✅ | — | — | — |
| Cursor | — | ✅ | — | — | — |
| Qoder | — | — | — | — | — |

*漂移处理（恢复 / 导入)对上表所有 agent 的服务商维度生效;运行时兑底链目前仅 Hermes 支持。*

每项能力具体写哪个文件、适用哪些安全规则,见 **[透明说明](docs/TRANSPARENCY.zh.md)**。

## 安装

### 下载（推荐）

从 [Releases](https://github.com/zhaolianghz/clawbox/releases) 下载最新 `.dmg`（macOS）。

### 从源码构建

**前置条件：** Node.js ≥ 18、Rust ≥ 1.77、`npm`

```bash
git clone https://github.com/zhaolianghz/clawbox.git
cd clawbox
npm install
npm run tauri build
# 产物：src-tauri/target/release/bundle/
```

**开发模式：**

```bash
npm run tauri dev
```

## 快速上手

1. 打开 ClawBox → **服务商** → 点击服务商卡片 → 填入 API Key → 保存
2. 前往 **Agent 管理** → 为每个 agent 选择该服务商，选中即生效
3. 完成——Claude Code、Codex 等 agent 现在使用你的服务商

## 技术栈

- [Tauri v2](https://tauri.app)（Rust 后端 + WebView 前端）
- [Svelte 5](https://svelte.dev)（runes 模式）
- [svelte-i18n](https://github.com/kaisermann/svelte-i18n)（English / 中文）
- 服务商/agent 图标来自 [lobe-icons](https://github.com/lobehub/lobe-icons)（MIT）

## 贡献

欢迎提 Issue 和 PR。重大改动请先开 Issue 讨论。

## 许可证

[MIT](LICENSE) © 2026 ClawBox contributors
