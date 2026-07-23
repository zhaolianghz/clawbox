# Changelog

All notable changes to ClawBox are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/), and versions adhere to
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Changed
- **服务商改为 per-agent 绑定** — 在 Agent 管理页为每个 agent 独立选择服务商,
  选中即生效;编辑服务商自动重新下发到绑定它的 agent。

### Removed
- Providers 页的全局默认(星标)与「同步到 Agent」面板,由 per-agent 绑定取代。
  旧的星标配置(`active_provider_id`)在加载时自动迁移为绑定。

## [0.2.0] - 2026-07-22

### Added
- **Import providers from cc-switch** — read the local `~/.cc-switch` config and
  merge its providers (Anthropic + OpenAI slots) into ClawBox in one step.
- **Light / dark / system theme switching** — persisted to `localStorage`,
  applied before first paint to avoid a flash. The native window background now
  tracks the theme so the transparent macOS title bar matches (no more dark bar
  in light mode).

### Changed
- **Feedback now files a GitHub Issue** instead of writing to a local file.
  Submitting opens a pre-filled issue (title, body with app version + platform,
  category label) in the default browser — zero backend, nothing stored locally.
- **Anthropic endpoint connectivity test** falls back to probing `POST /v1/messages`
  when `GET /v1/models` returns 404. Gateways that only implement the Messages API
  (e.g. Aliyun Bailian) now test as reachable instead of failing with
  "Endpoint not found".

### Removed
- Local feedback storage (`~/.clawbox/feedback.json`) and its "Previous Feedback"
  list, superseded by the GitHub Issue flow.

## [0.1.0] - 2026-07-20

- Initial release: unified configuration center for AI agents — providers, MCP,
  skills and memory in one place, synced to every agent.
