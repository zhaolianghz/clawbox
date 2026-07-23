<!-- English version: TRANSPARENCY.md -->

# 透明说明:ClawBox 写什么、写哪里、怎么写

ClawBox 是本地工具,绝不把你的密钥、端点、记忆传到任何地方——只在你自己机器上读写配置文件。本文列出**每项能力精确改动哪个文件、拥有哪些键、以及适用于每次写入的安全规则**。

## 安全规则(每次写入都适用)

1. **写前备份**——改动前先把目标文件复制到 `~/.clawbox/backups/<时间戳>/`。回滚只需把文件复制回去。
2. **合并写,绝不整体覆盖**——ClawBox 只碰它管理的那几个键或那段标记块,文件里其它内容逐字节保留。
3. **托管追踪**——记录上次下发给每个 agent 的内容(`providers_managed` / `mcp_managed`),移除只动它自己建的条目,绝不动你的。
4. **有歧义则拒绝**——如果托管块的标记损坏或重复,ClawBox 拒绝修改该文件,而不是瞎猜。
5. **密钥只在本地**——API 密钥存于 `~/.clawbox/config.json` 并写入各 agent 自己的配置,绝不出现在同步预览或日志里。

## 中央真源(“库”)

| 能力 | 真源 |
|---|---|
| 服务商 / MCP / 密钥 | `~/.clawbox/config.json` |
| 技能 | `~/.agents/skills/`(每个技能一个目录,内含 `SKILL.md`) |
| 记忆 | `~/.agents/memory/MEMORY.md` |

## 服务商

把**每个 agent 绑定的服务商**的端点 + 密钥 + 模型下发到该 agent 的原生配置。在 Agent 管理页为每个 agent 独立选择服务商,选中即生效;保存对服务商的编辑会自动重新下发到绑定它的所有 agent。`claude-code` / `codex` / `codebuddy` / `hermes` 是**单激活**(切换即替换值);`opencode` / `openclaw` 接收**完整**服务商列表。

| Agent | 文件 | ClawBox 拥有的部分 |
|---|---|---|
| claude-code | `~/.claude/settings.json` | `env`:`ANTHROPIC_BASE_URL` / `ANTHROPIC_AUTH_TOKEN` / `ANTHROPIC_MODEL` |
| codebuddy | `~/.codebuddy/settings.json` | `env`:`CODEBUDDY_BASE_URL` / `CODEBUDDY_API_KEY` / `CODEBUDDY_MODEL` |
| codex | `~/.codex/config.toml` + `~/.codex/auth.json` | `[model_providers.clawbox]` 表 + auth.json 里的 `OPENAI_API_KEY` |
| hermes | `~/.hermes/config.yaml` + `~/.hermes/.env` | `model.*` 键 + `.env` 里的 `CUSTOM_PROVIDER_<ID>_KEY` 行 |
| opencode | `~/.config/opencode/opencode.json` | `provider` 节(完整列表) |
| openclaw | `~/.openclaw/openclaw.json` | `models.providers` 节(完整列表) |

## MCP 服务器

把你的 MCP 列表(配置里的 `mcp_servers`)翻译成各 agent 的原生格式。`openclaw` 和 `hermes` 通过其**自带 CLI**(`mcp add` / `mcp remove`)写入,不直接改文件。

| Agent | 目标 | 键 / 机制 |
|---|---|---|
| claude-code | `~/.claude.json` | `mcpServers` |
| codex | `~/.codex/config.toml` | `[mcp_servers.<名称>]` 表 |
| opencode | `~/.config/opencode/opencode.json` | `mcp` |
| codebuddy | `~/.codebuddy/mcp.json` | `mcpServers` |
| cursor-agent | `~/.cursor/mcp.json` | `mcpServers` |
| openclaw | (经 CLI) | `openclaw mcp add/remove` |
| hermes | (经 CLI) | `hermes mcp add/remove` |
| kimi、qoder | — | 暂不支持 |

## 技能

技能只在 `~/.agents/skills/` 存一份,以**符号链接**(非拷贝)下发到各支持的 agent,因此更新库即更新所有 agent。

| Agent | 技能目录 |
|---|---|
| claude-code | `~/.claude/skills/` |
| openclaw | `~/.openclaw/skills/` |
| opencode | `~/.config/opencode/skills/` |
| hermes | `~/.hermes/skills/` |

其余 agent:暂不支持。

## 记忆

你的 `~/.agents/memory/MEMORY.md` 以**托管块**形式注入各 agent 的指令文件。只有标记之间的块属于 ClawBox;标记之外是你的内容,绝不触碰。

```
<!-- CLAWBOX_START -->
(你的 MEMORY.md 的镜像)
<!-- CLAWBOX_END -->
```

| Agent | 指令文件 |
|---|---|
| claude-code | `~/.claude/CLAUDE.md` |
| codex | `~/.codex/AGENTS.md` |
| opencode | `~/.config/opencode/AGENTS.md` |
| hermes | `~/.hermes/memories/MEMORY.md` |
| openclaw | `~/.openclaw/workspace/MEMORY.md` |

其余 agent:暂不支持。

## 备份与回滚

每次写入前都会在 `~/.clawbox/backups/` 下生成带时间戳的备份。撤销一次同步,只需把最新备份目录里的文件复制回原位置。技能是符号链接——删链接不影响库本身。
