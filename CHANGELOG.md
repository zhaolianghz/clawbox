# Changelog

All notable changes to ClawBox are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/), and versions adhere to
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Changed
- **Per-agent provider binding** — pick a provider for each agent independently
  on the Agents page; the selection takes effect immediately, and editing a
  provider automatically re-deploys it to every agent bound to it.

### Removed
- The global default (star) and the "Sync to agents" panel on the Providers
  page, superseded by per-agent binding. Legacy star configs
  (`active_provider_id`) are migrated to bindings automatically on load.

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
