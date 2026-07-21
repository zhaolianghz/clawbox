<!-- 中文版见 TRANSPARENCY.zh.md -->

# Transparency: what ClawBox writes, where, and how

ClawBox is a local tool. It never sends your keys, endpoints, or memory anywhere — it only reads and writes config files on your own machine. This document lists **exactly** which file each capability touches, which keys it owns, and the safety rules that apply to every write.

## Safety rules (apply to every write)

1. **Backup before write** — the target file is copied to `~/.clawbox/backups/<timestamp>/` before any modification. Roll back by copying the file back.
2. **Merge-write, never overwrite** — ClawBox only touches the specific keys or the marked block it manages. Everything else in the file is preserved byte-for-byte.
3. **Managed tracking** — ClawBox records what it last deployed to each agent (`providers_managed` / `mcp_managed`). Removal only touches entries it created, never yours.
4. **Refuse on ambiguity** — if a managed block's markers are broken or duplicated, ClawBox refuses to modify the file rather than guess.
5. **Keys stay local** — API keys live in `~/.clawbox/config.json` and are written into each agent's own config. They are never rendered into sync previews or logs.

## Central sources (the "library")

| Capability | Source of truth |
|---|---|
| Providers / MCP / keys | `~/.clawbox/config.json` |
| Skills | `~/.agents/skills/` (one folder per skill, each with `SKILL.md`) |
| Memory | `~/.agents/memory/MEMORY.md` |

## Providers

Pushes your active provider's endpoint + key + model into each agent's native config. `claude-code` / `codex` / `codebuddy` / `hermes` are **single-active** (switching swaps the values); `opencode` / `openclaw` receive the **full** provider list.

| Agent | File | What ClawBox owns |
|---|---|---|
| claude-code | `~/.claude/settings.json` | `env`: `ANTHROPIC_BASE_URL` / `ANTHROPIC_AUTH_TOKEN` / `ANTHROPIC_MODEL` |
| codebuddy | `~/.codebuddy/settings.json` | `env`: `CODEBUDDY_BASE_URL` / `CODEBUDDY_API_KEY` / `CODEBUDDY_MODEL` |
| codex | `~/.codex/config.toml` + `~/.codex/auth.json` | `[model_providers.clawbox]` table + `OPENAI_API_KEY` in auth.json |
| hermes | `~/.hermes/config.yaml` + `~/.hermes/.env` | `model.*` keys + `CUSTOM_PROVIDER_<ID>_KEY` line in `.env` |
| opencode | `~/.config/opencode/opencode.json` | `provider` section (full list) |
| openclaw | `~/.openclaw/openclaw.json` | `models.providers` section (full list) |

## MCP servers

Translates your MCP server list (`mcp_servers` in the config) into each agent's native format. `openclaw` and `hermes` are written through their **own CLI** (`mcp add` / `mcp remove`), not by editing files directly.

| Agent | Target | Key / mechanism |
|---|---|---|
| claude-code | `~/.claude.json` | `mcpServers` |
| codex | `~/.codex/config.toml` | `[mcp_servers.<name>]` tables |
| opencode | `~/.config/opencode/opencode.json` | `mcp` |
| codebuddy | `~/.codebuddy/mcp.json` | `mcpServers` |
| cursor-agent | `~/.cursor/mcp.json` | `mcpServers` |
| openclaw | (via CLI) | `openclaw mcp add/remove` |
| hermes | (via CLI) | `hermes mcp add/remove` |
| kimi, qoder | — | not supported yet |

## Skills

Skills live once in `~/.agents/skills/` and are deployed to each supported agent as **symlinks** (no copies), so updating the library updates every agent.

| Agent | Skills directory |
|---|---|
| claude-code | `~/.claude/skills/` |
| openclaw | `~/.openclaw/skills/` |
| opencode | `~/.config/opencode/skills/` |
| hermes | `~/.hermes/skills/` |

Other agents: not supported yet.

## Memory

Your `~/.agents/memory/MEMORY.md` is injected as a **managed block** into each agent's instruction file. Only the block between the markers is ClawBox's; everything outside is yours and is never touched.

```
<!-- CLAWBOX_START -->
(mirror of your MEMORY.md)
<!-- CLAWBOX_END -->
```

| Agent | Instruction file |
|---|---|
| claude-code | `~/.claude/CLAUDE.md` |
| codex | `~/.codex/AGENTS.md` |
| opencode | `~/.config/opencode/AGENTS.md` |
| hermes | `~/.hermes/memories/MEMORY.md` |
| openclaw | `~/.openclaw/workspace/MEMORY.md` |

Other agents: not supported yet.

## Backups & rollback

Every write is preceded by a timestamped backup under `~/.clawbox/backups/`. To undo a sync, copy the file from the latest backup folder back to its original location. Skills are symlinks — deleting the link leaves the library intact.
