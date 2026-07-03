# ClawBox: Backend Trait Abstraction + Hermes Co-Management

**Date:** 2026-07-03
**Status:** Approved (brainstorming)
**Scope:** Finish current WIP (cron / stats / config / gateway refactor) AND introduce Hermes as a second managed backend. MVP covers `gateway` + `cron`; `stats` and `config` will follow in later iterations.

## Context

ClawBox is a Tauri + Svelte desktop GUI that manages the `openclaw` CLI. Working
tree currently has an unfinished refactor (`commands/openclaw.rs`, `cron.rs`,
`stats.rs`, plus config/gateway restructuring and frontend wiring through
`App.svelte`, `monitor/+page.svelte`, `tasks/+page.svelte`). The user wants to:

1. Finish the in-progress refactor.
2. Add `hermes` (Hermes Agent CLI) as a **second managed backend running in
   parallel**, surfaced through the same UI.

Hermes has the same conceptual surface (`gateway`, `cron`, `status`) but with
significant command-level differences:

| Concern | OpenClaw | Hermes |
|---|---|---|
| Cron listing | `openclaw cron list --json` | `hermes cron list` (plain text) |
| Cron create | `openclaw cron add --name X --cron Y --message Z` | `hermes cron create <schedule> [prompt] --name X` |
| Cron remove | `openclaw cron rm <id>` | `hermes cron remove <job_id>` |
| Cron enable / disable | `enable` / `disable` verbs | `resume` / `pause` verbs |
| Gateway lifecycle | detached process on port 18789 (`lsof` / `kill`) | launchd service (`hermes gateway start` / `stop`) |
| Version / health | `--version`, `openclaw health` | `--version`, `hermes gateway status` |

These differences make a "shared helper with one CLI flag" approach painful.

## Decisions

- **WIP first, then Hermes** — land the openclaw refactor before layering
  the abstraction on top.
- **Two backends running in parallel** — both visible in the same UI, no
  global switch.
- **Merge + tag** — UI shows a single list per surface, every row carries a
  `backend` tag, errors per backend reported separately.
- **Per-backend dispatch** — every action carries the backend identifier;
  the Rust layer routes to the matching backend implementation.
- **Backend trait abstraction** — chosen over parallel modules + `if/else`
  because the CLI-shape differences are large enough that `if/else` would
  duplicate across every command.
- **MVP scope** — only `gateway` + `cron` get the hermes implementation;
  `stats` and `config` stay openclaw-only in this iteration.

## Design

### Backend trait (Rust)

`src-tauri/src/backends/mod.rs`:

```rust
pub trait Backend: Send + Sync {
    fn id(&self) -> &'static str;           // "openclaw" | "hermes"
    fn display_name(&self) -> &'static str;
    fn version(&self) -> String;
    fn is_installed(&self) -> bool;

    fn gateway_status(&self) -> Result<GatewayStatus, String>;
    fn gateway_start(&self) -> Result<String, String>;
    fn gateway_stop(&self) -> Result<String, String>;

    fn cron_list(&self) -> Result<Vec<CronJob>, String>;
    fn cron_create(&self, params: NewCron) -> Result<String, String>;
    fn cron_remove(&self, id: &str) -> Result<String, String>;
    fn cron_set_enabled(&self, id: &str, enabled: bool) -> Result<String, String>;
    fn cron_run(&self, id: &str) -> Result<String, String>;
}
```

Implementations:

- `OpenClawBackend` (`backends/openclaw.rs`) — absorbs current
  `commands/openclaw.rs`, `commands/cron.rs`, `commands/stats.rs`,
  `commands/gateway.rs`. Calls the CLI via existing helpers, JSON output
  parsed directly.
- `HermesBackend` (`backends/hermes.rs`) — calls `hermes` CLI, parses
  plain-text cron listing, maps `set_enabled(true)` → `cron resume`,
  `set_enabled(false)` → `cron pause`. Uses launchd for gateway control,
  not raw port probing.

A `fn backends() -> &'static [Box<dyn Backend>]` returns a static slice of
both implementations; `is_installed()` is checked at call time so missing
binaries degrade gracefully.

### Unified data model

`CronJob` is shared across backends. Each backend implementation normalises
its CLI output into this shape; the original CLI blob is kept on `raw` for
forward-compat and debugging.

```rust
pub struct CronJob {
    pub id: String,
    pub name: String,
    pub schedule: String,            // "0 2 * * *" or "30m" / "every 2h"
    pub enabled: bool,
    pub last_run: Option<String>,
    pub next_run: Option<String>,
    pub agent: Option<String>,
    pub message: Option<String>,
    pub raw: serde_json::Value,
}
```

Tagged envelope returned by aggregate commands:

```rust
pub struct TaggedCronJob { pub backend: String, pub job: CronJob }
pub struct BackendError { pub backend: String, pub message: String }
```

### Aggregate Tauri commands

`src-tauri/src/commands/aggregate.rs`:

```rust
#[tauri::command]
pub fn list_backends() -> Vec<BackendInfo>;

#[tauri::command]
pub fn gateway_status_all() -> Vec<GatewayStatus>;          // one per backend

#[tauri::command]
pub fn gateway_start(backend: String) -> Result<String, String>;
#[tauri::command]
pub fn gateway_stop(backend: String)  -> Result<String, String>;

#[tauri::command]
pub fn cron_list_all() -> CronListAllResult;                // tagged + errors

#[tauri::command]
pub fn cron_create(backend: String, params: NewCron) -> Result<String, String>;
#[tauri::command]
pub fn cron_remove(backend: String, id: String) -> Result<String, String>;
#[tauri::command]
pub fn cron_set_enabled(backend: String, id: String, enabled: bool) -> Result<String, String>;
#[tauri::command]
pub fn cron_run(backend: String, id: String) -> Result<String, String>;
```

Aggregate reads use `rayon::par_iter` (or `std::thread::scope`) to query
both backends concurrently. Per-backend failures are isolated: a failing
backend returns `[]` for its slice and one `BackendError` is appended to
the result. Action commands are NOT parallel — they target a single backend
chosen by the `backend` parameter.

### Hermes parsing strategy

`hermes cron list` currently emits plain text ("No scheduled jobs." or a
table). `parse_hermes_cron_text(&str) -> Vec<CronJob>` will:

1. Detect the empty-state marker.
2. For populated output, match `job_id:`, `name:`, `schedule:`, `next_run:`,
   `last_run:` patterns line by line. If a future Hermes release changes
   format, fall back to the closest readable representation and keep the
   raw blob on `CronJob::raw`.
3. Be unit-tested against: empty, single job, multiple jobs, jobs with
   prompt / skill / workdir / repeat.

`hermes cron create <schedule> [prompt] --name X` uses positional args;
`message` from `NewCron` becomes the optional `prompt`. `schedule` is
forwarded as-is (Hermes accepts `30m`, `every 2h`, `0 9 * * *`).

`hermes gateway start` / `stop` / `status` are passed through verbatim.
The launchd plumbing (plist presence, process state) is reported through
the same `GatewayStatus { status, version, pid }` shape.

### Frontend (Svelte / TS)

`src/lib/api/backends.ts` (new):

```typescript
export type BackendId = 'openclaw' | 'hermes';
export interface BackendInfo {
  id: BackendId;
  displayName: string;
  version: string;
  installed: boolean;
}
export async function list_backends(): Promise<BackendInfo[]>;
```

`src/lib/api/cron.ts` (rewrite on top of WIP):

```typescript
export interface CronJob {
  id: string; name: string; schedule: string; enabled: boolean;
  lastRun?: string; nextRun?: string; agent?: string; message?: string;
}
export interface TaggedCronJob { backend: BackendId; job: CronJob; }
export interface BackendError  { backend: BackendId; message: string; }
export interface CronListAllResult { jobs: TaggedCronJob[]; errors: BackendError[]; }

export async function list_cron_all(): Promise<CronListAllResult>;
export async function add_cron(backend: BackendId, params: NewCron): Promise<void>;
export async function remove_cron(backend: BackendId, id: string): Promise<void>;
export async function set_cron_enabled(backend: BackendId, id: string, enabled: boolean): Promise<void>;
export async function run_cron(backend: BackendId, id: string): Promise<void>;
```

`src/lib/api/gateway.ts` (rewrite):

```typescript
export interface GatewayStatus {
  backend: BackendId;
  status: 'running' | 'stopped';
  version: string;
  pid?: number;
}
export async function list_gateway_statuses(): Promise<GatewayStatus[]>;
export async function start_gateway(backend: BackendId): Promise<string>;
export async function stop_gateway(backend: BackendId): Promise<string>;
```

`src/lib/api/stats.ts` stays openclaw-only for this iteration (no hermes
insights command wiring).

### UI behaviour

- **Tasks / Cron page** — list grouped by backend. Section header
  `OpenClaw (3 jobs)` / `Hermes (1 job)` with a colour chip per backend.
  Each row carries a backend badge; every action button uses
  `job.backend` to dispatch. Create dialog adds a `Backend` dropdown.
  If `hermes` is not installed, that section renders an empty-state card.
- **Monitor / Dashboard** — replace the single stats card with one card
  per installed backend, side-by-side. Gateway status cards are also one
  per backend with independent start / stop controls.
- **i18n** — new keys: `backend.openclaw`, `backend.hermes`,
  `backend.notInstalled`, `gateway.action.start.openclaw`, etc.

### WIP landing

Current working-tree changes are folded into this design:

- `commands/openclaw.rs`, `commands/cron.rs`, `commands/stats.rs` —
  absorbed into `backends/openclaw.rs`.
- `commands/gateway.rs` — absorbed into `backends/openclaw.rs`.
- `commands/mod.rs` — replaced with `backends` + `commands::aggregate`.
- `lib.rs` — register `aggregate::*` commands only.
- `src-tauri/src/commands/config.rs` — Vec model kept; backend-agnostic
  (ClawBox's own config, not the CLI's).
- Frontend `lib/api/cron.ts` and `lib/api/stats.ts` — current shape
  discarded; rewritten as `cron_list_all` etc.
- `App.svelte`, `monitor/+page.svelte`, `tasks/+page.svelte` — updated
  to consume tagged shapes and group by backend.

## Error handling

- A failing backend in an aggregate read returns `[]` for that backend's
  slice plus one `BackendError`. The UI renders the section header but
  shows an inline error message; the other backend is unaffected.
- Action commands surface errors verbatim so the UI can show a toast.
- Missing CLI (`is_installed() == false`) — backend is omitted from the
  aggregate result; the UI shows a "not installed" empty state instead
  of an error.
- Hermes installed but unconfigured — CLI errors pass through unchanged.

## Testing

### Rust unit tests

- `parse_hermes_cron_text` — empty, single, multiple, with prompt / skill /
  workdir / repeat.
- `OpenClawBackend::cron_list` normalises mock CLI JSON to `CronJob`.
- `HermesBackend::cron_set_enabled(true)` → `["cron", "resume", id]`;
  `(false)` → `["cron", "pause", id]`.
- `HermesBackend::cron_create` maps `NewCron { schedule, message, name }`
  to `["cron", "create", schedule, "--name", name, message]`.
- Process spawning is mocked so tests run without the real CLIs.

### Rust integration tests (`tests/aggregate.rs`)

- `cron_list_all` with both backends failing returns empty `jobs` and two
  `errors`.
- `cron_list_all` with only openclaw failing returns hermes jobs + one
  error.
- `cron_create("nonexistent", ...)` returns `Err`.

### Frontend

- `cron.ts` normaliser extracted + vitest unit test mirroring the Rust
  fixtures.
- No e2e harness exists today; manual verification checklist covers the
  rest.

### Manual verification

1. `npm run tauri dev` — Dashboard renders one card per installed backend.
2. Tasks page: OpenClaw section shows existing jobs; Hermes section is
   empty (or shows real jobs if any).
3. Create a Hermes cron from the UI; reload; it persists.
4. Pause a Hermes cron; UI shows it disabled.
5. Move `hermes` binary aside; restart ClawBox; Hermes section shows
   "not installed"; OpenClaw section is unaffected.
6. Stop OpenClaw gateway; OpenClaw status flips to `stopped`; Hermes
   card unaffected.

## Definition of done

- `cargo check` + `cargo test` green.
- `npm run check` (svelte-check) green.
- WIP + Hermes in the same PR.
- Design doc committed.
- All 6 manual verification steps pass.

## Out of scope (later iterations)

- `stats` for Hermes (`hermes insights`).
- `config` for Hermes (`hermes config`).
- Logs / skills / chat / agents surfaces for Hermes.
- Per-backend UI themes; for now backend is communicated via small
  colour chip + display name.
- Settings screen for disabling a backend (today: derived from
  `is_installed()`).