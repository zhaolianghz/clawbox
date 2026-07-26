# Changelog

All notable changes to ClawBox are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/), and versions adhere to
[Semantic Versioning](https://semver.org/).

## [0.3.2] - 2026-07-24

### Added
- **Codex model catalog** — when a provider bound to Codex lists models,
  ClawBox now writes `~/.codex/clawbox-model-catalog.json` and points
  `model_catalog_json` at it in `config.toml`, so Codex's desktop model
  picker surfaces the models you configured instead of only the built-in
  ones. `defaultModel` is included even when it isn't in the models list, so
  the `model =` line always resolves. Removing the binding deletes the file
  and key, but leaves any `model_catalog_json` you set yourself untouched.
- **Startup reconciliation** — on launch, ClawBox re-checks every agent
  binding and silently re-deploys when the agent's config has drifted from
  what the binding expects (e.g. a ClawBox upgrade changed the deployed
  format, or the file was hand-edited). No drift means no writes and no
  backups; bindings to disabled providers are left alone.

### Fixed
- **Codex `wire_api` set to `responses`** — Codex 0.5x removed chat
  completions; writing `wire_api = "chat"` made Codex exit on startup.

## [0.3.1] - 2026-07-24

### Fixed
- **Light theme visibility on the Agents page** — the provider selector looked
  like plain text (invisible border, no dropdown arrow, no hover cue); it now
  has a themed background, border, chevron and hover highlight, with a muted
  "Select a provider" placeholder. Buttons, sync chips and teal accents on the
  page now use theme-aware tokens (`--border-strong`, `--border-subtle`,
  `--accent-teal`) instead of hard-coded white/teal values.

## [0.3.0] - 2026-07-24

### Changed
- **Per-agent provider binding** — pick a provider for each agent independently
  on the Agents page; the selection takes effect immediately, and editing a
  provider automatically re-deploys it to every agent bound to it.
- macOS bundle identifier renamed from `com.clawbox.app` to
  `com.clawbox.desktop` (the `.app` suffix conflicts with the macOS bundle
  extension). macOS treats this build as a new app; window state and
  permissions do not carry over.

### Removed
- The global default (star) and the "Sync to agents" panel on the Providers
  page, superseded by per-agent binding. Legacy star configs
  (`active_provider_id`) are migrated to bindings automatically on load.
- The "Not managed by ClawBox" unbind option in the agent provider selector.
  Unbound agents now show a disabled "Select a provider" placeholder; once
  bound, an agent stays managed by ClawBox.

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
