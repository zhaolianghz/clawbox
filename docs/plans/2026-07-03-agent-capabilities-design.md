# Agent Capabilities Abstraction (Skills / MCP / Memory / Plugins / Tools / Hooks)

**Date:** 2026-07-03
**Status:** Approved (brainstorming)
**Scope:** Add a uniform capability-management layer over the existing Backend trait, covering the 6 "agent" surfaces exposed by both `openclaw` and `hermes`: **Skills, MCP, Memory, Plugins, Tools, Hooks**.

## Context

`openclaw` and `hermes` both expose the same conceptual surfaces for extending an agent:

| Capability | openclaw subcommands | hermes subcommands |
|---|---|---|
| Skills | `list, search, install, update, check, info` | `list, browse, search, install, update, check, inspect, audit, uninstall, reset, ...` |
| MCP | `list, set, show, unset, serve` | `add, remove, list, test, configure, login, serve` |
| Memory | `index, promote` (search/reindex) | `setup, status, off, reset` (provider config) |
| Plugins | `list, install, enable, disable, doctor, inspect, marketplace` | `list, install, update, remove, enable, disable` |
| Tools | _(no top-level tools subcommand — folded into `agents`)_ | `list, enable, disable` |
| Hooks | `check, enable, disable, info` | `list, test, revoke, doctor` |

The interfaces differ significantly: openclaw outputs JSON for most commands; hermes outputs plain text with varied formats; some commands exist on only one side (`tools` is hermes-only); some capabilities (memory) have non-alignable operations (`index/promote` vs `setup/status/reset`).

ClawBox currently exposes only `gateway` + `cron` + `stats`. The user wants the same Backend-trait pattern extended to the 6 capability surfaces, with a single unified UI page.

## Decisions

- **6 capability traits**, not one mega-Backend — each capability is its own trait; backends implement what they support.
- **Multi-impl on each backend struct** — `impl SkillsCapability for OpenClawBackend` + `impl SkillsCapability for HermesBackend` (etc.). Capabilities are opt-in via `Option<&'static dyn Capability>` in a `BackendEntry` registry.
- **Tools is hermes-only** — openclaw doesn't implement `ToolsCapability`; the UI tab shows only the hermes section.
- **Memory has a minimal common surface** — `memory_status()` is universal; `memory_index()` is openclaw-only; `memory_reset()` is hermes-only. Each backend returns a clear `Err` for unsupported operations.
- **Single Capabilities UI page** with 6 tabs (not 6 separate routes).
- **MVP covers all 6** — but each capability implements the minimum useful set: list + 1-2 mutations (enable/disable, install, remove). No registry search/browse in this iteration.

## Design

### Capability traits

`src-tauri/src/backends/capabilities/` — one file per capability, each defining the data struct + trait:

```rust
// capabilities/skills.rs
#[derive(Serialize, Clone, Debug)]
pub struct Skill {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub enabled: bool,
    pub raw: serde_json::Value,
}

pub trait SkillsCapability: Send + Sync {
    fn skills_list(&self) -> Result<Vec<Skill>, String>;
    fn skills_install(&self, id: &str) -> Result<String, String>;
    fn skills_uninstall(&self, id: &str) -> Result<String, String>;
    fn skills_set_enabled(&self, id: &str, enabled: bool) -> Result<String, String>;
}
```

Other capabilities:

- **mcp**: `McpServer { id, name, transport, status, raw }` + `mcp_list`, `mcp_add(name, config_json)`, `mcp_remove(name)`.
- **memory**: `MemoryStatus { provider: String, items: Option<u64>, raw }` + `memory_status`, `memory_index`, `memory_reset`. Each backend returns `Err("unsupported")` for the operations it doesn't have.
- **plugins**: `Plugin { id, name, version, enabled, raw }` + `plugins_list`, `plugins_install(source)`, `plugins_remove(id)`, `plugins_set_enabled(id, enabled)`.
- **tools**: `Tool { id, enabled, raw }` + `tools_list`, `tools_set_enabled(id, enabled)`.
- **hooks**: `Hook { id, name, event, enabled, raw }` + `hooks_list`, `hooks_set_enabled(id, enabled)`.

### Backend entry registry

`src-tauri/src/backends/mod.rs`:

```rust
pub struct BackendEntry {
    pub backend: &'static dyn Backend,
    pub skills:  Option<&'static dyn SkillsCapability>,
    pub mcp:     Option<&'static dyn McpCapability>,
    pub memory:  Option<&'static dyn MemoryCapability>,
    pub plugins: Option<&'static dyn PluginsCapability>,
    pub tools:   Option<&'static dyn ToolsCapability>,
    pub hooks:   Option<&'static dyn HooksCapability>,
}

pub fn entries() -> &'static [BackendEntry] {
    static INSTANCES: OnceLock<Vec<BackendEntry>> = OnceLock::new();
    INSTANCES.get_or_init(|| vec![
        BackendEntry {
            backend: &OpenClawBackend,
            skills:  Some(&OpenClawBackend),
            mcp:     Some(&OpenClawBackend),
            memory:  Some(&OpenClawBackend),
            plugins: Some(&OpenClawBackend),
            tools:   None,                            // openclaw has no top-level tools subcommand
            hooks:   Some(&OpenClawBackend),
        },
        BackendEntry {
            backend: &HermesBackend,
            skills:  Some(&HermesBackend),
            mcp:     Some(&HermesBackend),
            memory:  Some(&HermesBackend),
            plugins: Some(&HermesBackend),
            tools:   Some(&HermesBackend),
            hooks:   Some(&HermesBackend),
        },
    ]).as_slice()
}
```

### Multi-impl

`OpenClawBackend` gains 5 new impls; `HermesBackend` gains 6. Each impl reuses existing `openclaw_json`/`openclaw_run` or `run_hermes` helpers:

```rust
impl SkillsCapability for OpenClawBackend {
    fn skills_list(&self) -> Result<Vec<Skill>, String> {
        let raw = openclaw_json(&["skills", "list", "--json"])?;
        Ok(parse_skills_json(raw))  // JSON normaliser
    }
    fn skills_install(&self, id: &str) -> Result<String, String> {
        openclaw_run(&["skills", "install", id])
    }
    // ...
}

impl SkillsCapability for HermesBackend {
    fn skills_list(&self) -> Result<Vec<Skill>, String> {
        let text = hermes_run_capture(&["skills", "list"])?;
        Ok(parse_hermes_skills_text(&text))  // plain-text parser
    }
    // ...
}
```

Hermes plain-text parsers will be written **after capturing real CLI output** (same TDD discipline as the cron parser rewrite). Each parser gets 3-4 fixtures: empty, single entry, multiple entries, unknown-fields.

### Aggregate commands

`src-tauri/src/commands/aggregate.rs` — 6 list_all + ~12 actions:

```rust
#[derive(Serialize)]
pub struct Tagged<T> { pub backend: String, pub item: T }
#[derive(Serialize)]
pub struct TaggedListResult<T> {
    pub items: Vec<Tagged<T>>,
    pub errors: Vec<BackendError>,
}

// list_all
#[tauri::command] pub fn skills_list_all() -> TaggedListResult<Skill>;
#[tauri::command] pub fn mcp_list_all()    -> TaggedListResult<McpServer>;
#[tauri::command] pub fn memory_status_all() -> Vec<Tagged<MemoryStatus>>;
#[tauri::command] pub fn plugins_list_all() -> TaggedListResult<Plugin>;
#[tauri::command] pub fn tools_list_all()   -> TaggedListResult<Tool>;   // openclaw 缺席
#[tauri::command] pub fn hooks_list_all()   -> TaggedListResult<Hook>;

// actions
#[tauri::command] pub fn skills_install(backend, id) -> ...;
#[tauri::command] pub fn skills_uninstall(backend, id) -> ...;
#[tauri::command] pub fn skills_set_enabled(backend, id, enabled) -> ...;
#[tauri::command] pub fn mcp_add(backend, name, config_json) -> ...;
#[tauri::command] pub fn mcp_remove(backend, name) -> ...;
#[tauri::command] pub fn memory_index(backend) -> ...;
#[tauri::command] pub fn memory_reset(backend) -> ...;
#[tauri::command] pub fn plugins_install(backend, source) -> ...;
#[tauri::command] pub fn plugins_remove(backend, id) -> ...;
#[tauri::command] pub fn plugins_set_enabled(backend, id, enabled) -> ...;
#[tauri::command] pub fn tools_set_enabled(backend, id, enabled) -> ...;
#[tauri::command] pub fn hooks_set_enabled(backend, id, enabled) -> ...;
```

Each action's dispatch:

```rust
#[tauri::command]
pub fn skills_install(backend: String, id: String) -> Result<String, String> {
    let entry = entries().iter().find(|e| e.backend.id() == backend)
        .ok_or_else(|| format!("Unknown backend: {}", backend))?;
    let skills = entry.skills
        .ok_or_else(|| format!("{} does not support skills", backend))?;
    skills.skills_install(&id)
}
```

`is_installed()` is checked inside the list_all loops (matches today's pattern). Uninstalled backends are silently dropped from results.

### Frontend

`src/lib/api/capabilities/{skills,mcp,memory,plugins,tools,hooks}.ts`:

```typescript
export interface Skill { id; name; version; description; enabled }
export interface TaggedSkill { backend: BackendId; skill: Skill }
export interface TaggedListResult<T> { items: Tagged<T>[]; errors: BackendError[] }
export async function skills_list_all(): Promise<TaggedListResult<Skill>>
export async function skills_install(backend: BackendId, id: string): Promise<void>
// uninstall + set_enabled similarly
```

`src/routes/capabilities/+page.svelte` — single page with 6 tabs (Skills / MCP / Memory / Plugins / Tools / Hooks). Each tab:

- Header row with backend sections (OpenClaw / Hermes), badge-style.
- Per backend, a list of capability items with enable/disable/remove/install buttons.
- Actions automatically carry `job.backend` so the right backend gets dispatched (same pattern as Tasks page).
- Missing backend shows "not installed" empty state.

**Sidebar**: add "Capabilities" nav entry in `+layout.svelte` (icon ⚙️).

**i18n keys**:
```
capabilities.title
capabilities.tab.{skills,mcp,memory,plugins,tools,hooks}
capabilities.section.{openclaw,hermes}
capabilities.notInstalled
capabilities.{skills,mcp,...}.{install,uninstall,enable,disable,...}
```

### WIP cleanup

- `src/lib/api/skills.ts` (mock) — **rewritten** to wrap the new tagged backend API.
- `src/routes/skills/+page.svelte` — replaced by `/capabilities?tab=skills`. Old route removed or kept as a redirect.

## Error handling

- **Missing backend** (CLI not installed): silently omitted from list_all results (matches current `list_backends` behaviour).
- **Per-backend failure**: one `BackendError` per failing backend, surfaced inline in the UI like Tasks page does.
- **Unsupported operation** (e.g. `tools_set_enabled` for openclaw): clear `Err("openclaw does not support tools")` returned to the UI as a toast.
- **Bad user input**: validation at the aggregate layer (e.g. empty id → `Err("id is required")`).
- **Parse failures**: parser returns `Err`; per-backend errors propagate normally.

## Testing

### Unit tests (per parser / normaliser)

Hermes text parsers follow the same pattern as `parse_hermes_cron_text`:

- `parse_hermes_skills_text`: empty / single / multiple / unknown-fields.
- `parse_hermes_mcp_text`: same.
- `parse_hermes_plugins_text`: same.
- `parse_hermes_tools_text`: same.
- `parse_hermes_hooks_text`: same.
- `parse_openclaw_skills_json`, etc. for openclaw JSON normalisers.

Openclaw arg-mapping helpers (similar to `openclaw_create_args`) get at least 1 test per action.

### Integration tests (`tests/capabilities_smoke.rs`)

Following the existing `tests/smoke.rs` pattern:

- `skills_list_all_discovers_both_backends` — both backends installed → tagged list contains both.
- `skills_install_dispatches_to_correct_backend` — verify routing.
- `mcp_list_handles_openclaw_missing` — openclaw gateway down → BackendError surfaced, hermes still works.
- `tools_list_excludes_openclaw` — openclaw absent from tools section.
- `memory_index_returns_unsupported_on_hermes` — clear error message.

### Manual verification

1. `npm run tauri dev` → click Capabilities nav → 6 tabs render.
2. Skills tab: OpenClaw + Hermes sections both render.
3. Create/remove a skill from one section → list updates.
4. MCP tab: both backends list their configured servers.
5. Plugins tab: disable a plugin → reload → still disabled.
6. Move `hermes` binary aside → restart → Hermes sections show "not installed", OpenClaw unaffected.

## Definition of done

- `cargo test` + `cargo check` green (≥ 25 unit tests + ≥ 5 integration tests).
- `npx tsc --noEmit` green (no new errors beyond pre-existing chat.ts warnings).
- `npm run build` succeeds.
- All 6 tabs in `/capabilities` render with real backend data (or graceful empty state).
- All 6 manual verification steps pass.

## Out of scope (later iterations)

- Skill registry search/browse/install from network (only local list + enable/disable).
- Memory provider setup flow (only display status + reset).
- Tools per-platform configuration (only global enable/disable).
- Hook synthetic-payload testing UI (only list + enable/disable).
- Plugin marketplace browsing (only list + install from source URL).
- Capability export/import between backends.