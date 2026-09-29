# Changelog

All notable changes to ClawBox are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/), and versions adhere to
[Semantic Versioning](https://semver.org/).

## [0.6.7] - 2026-09-29

### Fixed
- **Claude-code subagents died instantly ("0 tool uses · 0 tokens") against
  third-party gateways** — spawned subagent processes don't unconditionally
  inherit `ANTHROPIC_MODEL`: which tier a request lands on is decided by
  `ANTHROPIC_DEFAULT_{HAIKU,SONNET,OPUS,FABLE}_MODEL` and
  `CLAUDE_CODE_SUBAGENT_MODEL`. Left unset, the CLI requests the literal
  Anthropic model id from the gateway (400/503); left stale after switching
  providers, they carry the previous provider's model name with the same
  result — main session fine, every subagent dead. The env-settings adapter
  now manages these as `model_slots`, written and cleaned up together with the
  MODEL key (claude-code's five tier keys; codebuddy has no such concept).
- **Pi card showed a permanent false "config drift"** — Pi enriches the
  provider node ClawBox writes into `~/.pi/agent/models.json` with
  `name`/`input`/`contextWindow`/`cost` (and enriches each `models[]` entry),
  but the adapter's `plan` compared the whole node for equality, so the node
  never matched the desired state and the drift banner reappeared right after a
  successful sync. Comparison is now a projection over only the keys ClawBox
  deploys (`baseUrl`, `api`, `apiKey`, `models[].id`) — the same approach the
  dsh adapter already used.
- **Re-syncing Pi wiped Pi's enriched fields** — `apply` now merges the managed
  keys into the existing node instead of replacing it wholesale, keeping Pi's
  additions and its enriched `models[]` entries (matched by id).
- **Drift banner read like a name mismatch when the names matched** — the
  banner only ever shows the same binding the provider dropdown shows, so
  "settings inconsistent; ClawBox remembers X" looked like a false alarm next to
  a dropdown reading X. Both locales now state that it is the config *content*
  that doesn't match what ClawBox deployed; the variant shown when the agent's
  current provider can't be read points at the dropdown explicitly.

## [0.6.6] - 2026-09-28

### Fixed
- **Model listing on the Anthropic slot always empty** — third-party Anthropic
  gateways rarely implement a list-models endpoint (every candidate path 404s
  on e.g. Xiaomi MiMo's `/anthropic`), and the old code then fell back to a
  reachability probe whose model list is always empty: "Test connection" showed
  0 models and "Fetch models" reported a reachable endpoint that returned none.
  The 404 branch now retries the same host's `/v1/models` (OpenAI convention,
  commonly mounted on the same host, path prefix preserved) before giving up.
  The frontend also fetches the OpenAI slot first — the Anthropic slot was the
  only one that couldn't list — and the empty-result message now says the
  endpoint is reachable but offers no model list.

## [0.6.5] - 2026-09-23

### Fixed
- **codex / hermes reported as "not installed" although installed** (GUI
  launches only) — two independent causes. hermes: `hermes --version` took
  ~10.6s on a machine stuck in an interrupted update (every invocation re-runs
  install recovery) while the version-probe watchdog was 10s, so the process
  was killed just before returning. The probe now uses the named constant
  `VERSION_PROBE_TIMEOUT = 30s`. codex: it ships inside the ChatGPT desktop app
  bundle (`/Applications/ChatGPT.app/Contents/Resources/codex`), which is
  neither on `PATH` nor an npm global install, so every `resolve()` step missed
  it. codex gains a `fallback_paths` entry, and `expand_home` — which only
  understood a `~` prefix although `fallback_paths` is documented to accept
  absolute paths, so absolute entries silently did nothing — is renamed
  `expand_path` and passes absolute paths through.

## [0.6.4] - 2026-09-22

### Added
- Failover design doc for the 12 visible agents (relay sidecar + native chain),
  recording the reuse candidates ruled out by measurement so the investigation
  is not repeated.

### Fixed
- **Codex 401 against custom providers** — the custom
  `[model_providers.clawbox]` block lacked `requires_openai_auth`, so codex
  sent no `Authorization` header at all; and even with it, codex switched to a
  ChatGPT OAuth token as soon as `auth.json` still said
  `auth_mode = "chatgpt"`. `apply` / `desired_state` now write
  `requires_openai_auth = true`, writing `OPENAI_API_KEY` also sets
  `auth_mode = "apikey"` (other keys preserved), and `auth_mode` is part of
  the `current_state` comparison so existing configs self-repair.
- **Codex unbind left our `OPENAI_API_KEY` behind** — the built-in openai
  provider then overrode the ChatGPT login with that third-party key and sent
  it to `api.openai.com` (observed: `401 Incorrect API key`). Unbind now
  removes the key together with the `auth_mode` we wrote, restores `auth.json`
  even when our table in `config.toml` is gone, and returns non-zero on any
  change so the post-write verification is not skipped.
- **Usage price-table stale banner fired 15 days early** —
  `PricingMeta::snapshot` compared `age > days_until` with
  `days_until = 30 - age`, i.e. the banner appeared at `age > 15`. It now
  reuses the already-correct `PricedModel::is_stale` (`age > 30`); two
  docstrings saying 90 days were corrected to 30.
- **CI** — three `api_key` assertions had been changed to `"***"` while the
  fixture value is `sk-secret-123`, so they could never pass; `AgentLogo`
  declared a `size` prop that was never read and never passed (deleted); the
  qoder usage test hardcoded the macOS `Library/Application Support/Qoder`
  layout while `db_path()` branches per platform, so on Linux CI it scanned
  `~/.config/Qoder` and found 0 events. `cargo test --lib` 355 passed /
  0 failed; `svelte-check` 0 errors.

## [0.6.3] - 2026-09-07

### Fixed
- **Claude Code usage counted API-error placeholder rows** — on auth failure,
  rate limit or invalid request Claude Code appends an `isApiErrorMessage` row
  whose `message.model` is `"<synthetic>"` with all-zero usage, and it was
  counted as a real model. Such rows are now skipped at parse time (counted in
  `lines_skipped`), and `aggregate.refresh` runs an idempotent
  `store::prune_synthetic_models` so buckets persisted by earlier versions are
  cleared as well.

## [0.6.2] - 2026-09-07

### Fixed
- **Hermes scan never incremented `lines_total`** — with 300 real rows
  `events_matched=300` but `lines_total=0`, so `matched_ratio` had a zero
  denominator and fell into the "empty file" branch returning 1.0; the yellow
  low-coverage banner could never appear. The other 10 real-scanning adapters
  counted correctly.
- **`aggregate.refresh` wrote `last_scan_at` only when `providers_meta` was
  non-empty** — callers passing an empty meta map left the bucket timestamp at
  the previous scan, so the UI showed a "format may have changed" banner.
  `last_scan_at` now updates unconditionally; the `agent_to_provider` snapshot
  keeps the original conditional path and does not clobber existing bindings.

## [0.6.1] - 2026-09-06

### Added
- **14 more usage adapters** — aider, cline, codebuddy, cursor_agent, dsh,
  gemini, hermes, kimi, openclaw, opencode, pi, qoder, qwen_code and
  trae_agent (16 agents total alongside Claude Code and Codex), each with
  desensitized golden fixtures.

### Fixed
- **Critical token-tracking bugs in the new adapters** — qoder agent id
  aligned with the agents module (`qoder` → `qodercli`); gemini gained
  `lines_total` / `lines_skipped` counters, skips events with a missing
  `event_id` and counts whole-file OTel JSON array parse failures; openclaw
  scans every db (sorted by name) into a single `UsageScan`; qwen_code,
  codebuddy and gemini got an event-id collision guard (a file-level `HashMap`
  adds a sequence number for repeated turns sharing `(session, ts, model)`).

## [0.6.0] - 2026-09-03

### Added
- **Aider support** — provider integration writing `~/.aider.conf.yml`
  (`model`, plus `openai-api-base` / `openai-api-key` or the Anthropic pair).
- **Usage dashboard redesign** — the `/usage` page was slimmed down to the
  essentials (≈530 lines removed) and the token timeline gained a per-series
  hover tooltip (input / cache_read / cache_creation / output).

### Fixed
- **Provider model sync contract restored** — ClawBox no longer scrubs
  cross-family model strings before writing them, which had left
  `ANTHROPIC_MODEL` / `CODEBUDDY_MODEL` unwritten and the agent status bar
  showing a model other than the configured one. Whatever is in the provider
  `default_model` field now goes straight into the agent env; choosing a
  matching `base_url` + model pair is the user's responsibility.
- Agents without a provider configuration are hidden from the Agents page.

## [0.5.6] - 2026-08-31

### Added
- **Usage page (`/usage`)** — roadmap item #1. Per-day × agent × model
  aggregation of the real token consumption of every local agent CLI,
  persisted to ClawBox-owned storage (`~/.clawbox/usage/`) and therefore
  independent of the raw JSONL files, which Claude Code prunes after 30 days.
- **`UsageProvider` trait** — one adapter per agent (Claude Code, Codex v1),
  per-line fault tolerance, a yellow UI banner when `matched_ratio` falls below
  80%, and per-adapter revision + failure isolation so a format change loses at
  most the data written after the change.
- **Codex cumulative-delta semantics** — `token_count` events only carry the
  cumulative `total_token_usage`; a per-file `last_total` snapshot yields the
  turn delta (output includes reasoning).
- **Official price tables** — 9 vendors × 80+ models (Claude / GPT / Gemini /
  DeepSeek / GLM / Kimi / MiniMax / Qwen Bailian / Doubao), bundled statically
  with no network dependency; every row carries `verified_at` and the UI shows
  a three-state freshness banner. `ProviderSpec` gained `model_aliases` +
  pricing override with a three-tier lookup (override → alias → builtin) that
  maps relay model names onto official prices.
- **Cost surfaces** — 30-day summary card and a per-model cost column on
  `/usage` ("—" when no official price), a "Usage" entry in the Agents page
  header plus a current-month consumption bar per agent card
  (model · share %), and per-provider monthly tokens on the Providers page.
- Three Tauri commands: `usage_summary` / `usage_refresh` /
  `usage_provider_summary`. Spec: `docs/superpowers/specs/2026-08-29-token-usage-design.md`,
  plan: `docs/superpowers/plans/2026-08-29-token-usage.md`.

### Fixed
- **Monthly buckets counted the same events twice** — buckets gained a
  `seen_events` dedup key and `append_events_batch`, and `aggregate::refresh`
  now writes one batch per month instead of one write per event.

## [0.5.5] - 2026-08-29

### Fixed
- **Provider names are saved exactly as entered** — editing a built-in entry no
  longer auto-prefixes "Custom-" (the 0.5.4 auto-rename is reverted).
- **Agents reported "no models"** when a provider had models checked but no
  default model selected: both the load and the save path now fall back to the
  first known model (affects agents keyed on the default model, e.g. kimi).

## [0.5.4] - 2026-08-28

### Added
- **"Official default" provider** — a virtual entry at the top of each agent's
  provider selector; binding and syncing it restores that agent to its official
  default configuration. The entry is not persisted and not editable, and the
  binding stays visible and reversible.
- **Provider search rework** — ignores the current category and free-tier
  filters and scans the full catalog; space-tokenized AND matching
  (`zhipu GLM` = `zhipu` matches id/alias + `GLM` matches description); catalog
  entries expose `keywords` aliases.
- Spanish README, plus the Gemini / Cline / Pi / DeepSeek Harness / Qwen Code
  rows in the trilingual support matrix.

### Changed
- Editing the base URL of a built-in provider auto-renames it to
  `Custom-<name>` when the name has not been customized, signalling that it is
  no longer an official direct connection.

### Fixed
- **Full-table save was rejected** — the virtual "Official default" injected at
  read time was echoed back in the payload and tripped the guard, blocking any
  provider add or edit; it is now silently stripped server-side and filtered
  client-side.
- **Duplicate endpoints hid entries** — when several provider configurations
  pointed at the same catalog endpoint only the first was rendered; the first
  now merges into the catalog card and the rest remain standalone custom cards.
- Spanish README machine-translation glitches.

## [0.5.3] - 2026-08-25

### Fixed
- **DeepSeek Harness provider-binding selector did not render** — `dsh` was
  missing from the frontend agent slots.
- **dsh rejected its credentials file** — missing top-level `version: 1` key.

## [0.5.2] - 2026-08-25

### Fixed
- **nvm-managed npm-global agents misdetected as not installed** in GUI mode
  (Dock/Finder launch): the resolution chain now falls back to scanning nvm
  directories instead of relying on the shell `PATH`.

## [0.5.1] - 2026-08-24

### Added
- **DeepSeek Harness (dsh) support** — one-click install, provider routing
  written into `settings.yaml` (credentials at `0600`), dual-protocol endpoints
  and an automatic model catalog. Unbinding removes only the ClawBox routes and
  credential references, leaving hand-written config intact.

## [0.5.0] - 2026-08-24

### Added
- **One-click Doctor checkup** — the Agents page summarizes local health
  (PATH, dependencies, orphan bindings, config drift, provider endpoint probe
  and gateway check), colour-coded by severity with fix hints.

### Changed
- Custom skill-repository installation moved into the "Mine" toolbar next to
  import and scan.

## [0.4.0] - 2026-08-15

### Added
- **Configuration snapshots and rollback** — a snapshot is taken before every
  sync (providers / fallback / MCP / skills / memory, 20 kept per agent), with
  a history panel on the Agents page for browsing and one-click restore. A
  safety snapshot precedes every restore, so even a bad restore can be undone.
  All four `apply` paths now route through the snapshot layer (which replaces
  the older `backup_target`), and restore includes path-escape protection and
  bookkeeping cleanup.

### Removed
- Copilot CLI (no custom model support); its MCP adapter and detection
  definition were deleted along with it.

## [0.3.9] - 2026-08-09

### Fixed
- **Windows: agent detection regression from 0.3.8 and `PATH` corruption** —
  the 0.3.8 fallback rewrote the user `PATH`; it no longer does.
- **OpenClaw gateway probe on Windows**, plus a false positive where a client
  socket was mistaken for the gateway.

## [0.3.8] - 2026-08-07

### Fixed
- **Installed agents shown as "Not installed"** when `PATH` probing timed out
  on heavy shell setups (nvm / conda): detection no longer relies on the shell
  `PATH` alone. (#3)

## [0.3.7] - 2026-08-06

### Added
- Structured free-tier quota display for providers, with a dedicated filter
  toggle.

## [0.3.6] - 2026-08-06

### Added
- **Hermes fallback chain** — automatically switch to backup providers on rate
  limits or errors, with drag-to-reorder priority; two-way sync adopts the
  provider an agent is currently using into ClawBox in one click.
- **Drift handling** — hand-edited agent configs are never silently
  overwritten: ClawBox shows a visual diff and a one-click restore, and
  verifies every write after the fact.
- Skills page reworked into "Market / Mine" tabs, matching MCP.

## [0.3.5] - 2026-08-05

### Added
- **Configuration import / export** — providers, MCP servers and skills bundled
  into a single `.clawbox.json` to share, with a per-item preview
  (add / merge / overwrite / skip) before applying; existing API keys are never
  overwritten. (#2)

## [0.3.4] - 2026-08-03

### Fixed
- Hermes provider sync, and the version shown on the About page.
- **Startup white flash** — the window is now created hidden and shown from Rust
  once the page has loaded; the provider selector no longer appears late.
- Memory and skills pages are no longer blocked by native memory probing:
  collapsed sections load lazily when expanded.

## [0.3.3] - 2026-07-31

### Added
- **MCP marketplace** — curated catalog with card browsing, category filters,
  search, one-click add and add-with-parameters, plus a "Configured" tab.
- **Six new agents** — Gemini CLI, Cline, Pi, Qwen Code, Copilot CLI (npm
  install) and Trae Agent (detection only), with provider binding for
  gemini / cline / pi and Kimi adapted to the new Kimi Code directory and
  schema.
- **Three UI themes** — cyberpunk neon, clean light and liquid glass,
  switchable from the About page; agent icons switched to real brand marks via
  lobe-icons, with the ClawBox logo on the About page.
- More curated skill sources on the Capabilities page.

### Fixed
- Agent name mapping on the Providers page was missing the six new agents.
- The upgrade badge now compares versions as strict semver.

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
