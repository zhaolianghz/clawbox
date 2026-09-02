# Aider Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add Aider to ClawBox with macOS/Windows detection and installation plus reversible Provider synchronization through `~/.aider.conf.yml`.

**Architecture:** Add Aider to the canonical backend Agent registry, extend the existing install command builder with a `pipx` package variant, and implement a dedicated `ProviderAdapter` in `src-tauri/src/sync/providers.rs`. Reuse existing overview, binding, managed-key, and frontend card flows; do not add MCP, Skills, or Memory support in this phase.

**Tech Stack:** Rust, Tauri commands, serde_yaml, Svelte 5, TypeScript, svelte-i18n, Cargo tests, svelte-check, Vite.

## Global Constraints

- macOS and Windows support are required; Linux support is not required.
- Install package is exactly `aider-chat`, executable is exactly `aider`.
- Install command is `pipx install aider-chat`; never silently fall back to system pip.
- Configuration file is `~/.aider.conf.yml`.
- Preserve unrelated YAML keys; remove only keys recorded in `providers_managed`.
- Do not change Rust ProviderSpec or other backend data structures.
- Do not claim MCP, Skills, or Memory support for Aider in this phase.
- Keep the existing hidden list for Cursor, Qoder, Qwen Code, and Trae Agent.

---

### Task 1: Add Aider registry and pipx installation support

**Files:**
- Modify: `src-tauri/src/agents/mod.rs`
- Modify: `src-tauri/src/agents/install.rs`
- Test: inline tests in `src-tauri/src/agents/mod.rs` and `src-tauri/src/agents/install.rs`

**Interfaces:**
- Add `InstallMethod::Pipx { package: &'static str }`.
- `build_install_args(&AgentDef) -> Result<(String, Vec<String>), String>` returns `("pipx", ["install", "aider-chat"])` for Aider.
- Register `AgentDef { id: "aider", label: "Aider", binary: "aider", kind: NativeCli, install: Pipx { package: "aider-chat" }, check_probe: &["--version"], fallback_paths: &["~/.local/bin/aider"], depends_on: &[], docs_url: Some("https://aider.chat") }`.

- [ ] **Step 1: Write failing tests**

Add a registry assertion that `find_agent("aider")` returns the expected binary, kind, package, and fallback path. Add an install-builder test asserting:

```rust
let (cmd, args) = build_install_args(find_agent("aider").unwrap()).unwrap();
assert_eq!(cmd, "pipx");
assert_eq!(args, vec!["install", "aider-chat"]);
```

- [ ] **Step 2: Run focused tests and verify failure**

Run:

```bash
cd src-tauri && cargo test aider --lib
```

Expected: failure because the registry variant and Aider entry do not exist.

- [ ] **Step 3: Implement the minimal registry and command-builder changes**

Add the enum variant, add the `match` arm in `build_install_args`, and add the registry entry. Keep `run_install` unchanged so a missing `pipx` is surfaced by the existing process-launch error. Ensure the existing upgrade/version probe path can find `~/.local/bin/aider` through the declared fallback path.

- [ ] **Step 4: Run focused tests**

Run:

```bash
cd src-tauri && cargo test aider --lib
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/agents/mod.rs src-tauri/src/agents/install.rs
git commit -m "feat(agents): register aider with pipx installation"
```

### Task 2: Implement the Aider Provider adapter

**Files:**
- Modify: `src-tauri/src/sync/providers.rs`
- Test: inline provider adapter tests in `src-tauri/src/sync/providers.rs`

**Interfaces:**
- Add `AiderProviderAdapter` implementing the existing `ProviderAdapter` trait.
- `agent_id()` returns `"aider"`.
- `config_path(home)` returns `home.join(".aider.conf.yml")`.
- `plan` and `apply` use existing `ChangeItem`, `active_spec`, managed-key bookkeeping, YAML read/write, and validation conventions already present in this module.
- `deployed_names` returns the exact non-empty managed key names written for the active Provider.

- [ ] **Step 1: Write failing adapter tests**

Add tests covering these exact cases:

1. Missing config plans `add` for each non-empty desired key and apply creates valid YAML.
2. An OpenAI-compatible Provider writes `model`, `openai-api-base`, and `openai-api-key`.
3. An Anthropic Provider writes `model`, `anthropic-api-base`, and `anthropic-api-key`.
4. An existing unrelated YAML key survives apply.
5. Re-planning after apply returns `unchanged` changes and does not duplicate content.
6. Empty model or missing selected endpoint returns a skip/no-deployment result and does not write an incomplete config.
7. Malformed YAML returns an error and leaves the original bytes unchanged.
8. Removing/resetting a Provider removes only keys present in the managed list; a same-named un-managed user key is not removed.

Use the existing `TempHome`, Provider fixture helpers, and plan/apply assertions in the surrounding test module rather than creating a second test harness.

- [ ] **Step 2: Run focused tests and verify failure**

Run:

```bash
cd src-tauri && cargo test aider --lib
```

Expected: failure because the adapter and registry entry do not exist.

- [ ] **Step 3: Implement the adapter using the existing YAML merge pattern**

Use the Provider's normalized slots:

- Prefer the OpenAI slot when `openai_base_url` is non-empty; write Aider's `openai-api-base` and `openai-api-key`.
- Otherwise use the Anthropic slot when `anthropic_base_url` is non-empty; write `anthropic-api-base` and `anthropic-api-key`.
- Always require a non-empty `default_model` before deployment and write `model`.
- Only include credential keys when their value is non-empty.
- Keep key names exactly hyphenated as Aider expects.
- Parse an absent file as an empty YAML mapping, reject malformed YAML before any write, preserve keys outside the managed set, and use existing snapshots/validation behavior.
- Ensure default-provider reset invokes the adapter's remove path and leaves unrelated keys intact.

- [ ] **Step 4: Register the adapter and run focused tests**

Add `Box::new(AiderProviderAdapter)` to `providers::adapters()` near the other native CLI adapters. Run:

```bash
cd src-tauri && cargo test aider --lib
```

Expected: PASS.

- [ ] **Step 5: Run provider registry tests**

Run:

```bash
cd src-tauri && cargo test provider_adapter_registry --lib
```

Expected: PASS with Agent and Provider adapter registries aligned.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/sync/providers.rs
git commit -m "feat(sync): add aider provider adapter"
```

### Task 3: Wire Aider into the frontend presentation

**Files:**
- Modify: `src/lib/components/AgentLogo.svelte`
- Modify: `src/routes/agents/+page.svelte` only if the new ID is explicitly filtered
- Modify: `src/routes/providers/+page.svelte` only if its static display-name map requires Aider
- Modify: `src/routes/capabilities/+page.svelte` only if its static display-name map requires Aider

**Interfaces:**
- Aider's backend label remains `Aider`.
- Existing `AgentStatus` and `AgentSyncOverview` types remain unchanged.
- Aider must not be added to `PROVIDER_UNSUPPORTED_AGENTS`.

- [ ] **Step 1: Add the logo mapping**

Add an `aider` entry to `AgentLogo.svelte` using the component's existing mapping shape and a restrained warm orange/amber color, without adding a new icon dependency.

- [ ] **Step 2: Add static labels only where required**

Search the three listed Svelte files for hard-coded ID-to-label maps. Add `aider: 'Aider'` only to maps that are used for rendering backend-provided Aider entries; do not alter unrelated labels or hide logic.

- [ ] **Step 3: Run frontend checks**

Run:

```bash
npm run check
npm run build
```

Expected: both commands pass with no Svelte diagnostics.

- [ ] **Step 4: Commit**

```bash
git add src/lib/components/AgentLogo.svelte src/routes/agents/+page.svelte src/routes/providers/+page.svelte src/routes/capabilities/+page.svelte
git commit -m "feat(ui): show aider in agent management"
```

Only include files that actually changed.

### Task 4: Full verification and integration review

**Files:**
- Modify: none unless verification reveals a focused defect

- [ ] **Step 1: Run the full backend suite**

Run:

```bash
cd src-tauri && cargo test
```

Expected: all unit tests pass and the existing ignored test remains explicitly ignored.

- [ ] **Step 2: Run smoke tests**

Run:

```bash
cd src-tauri && cargo test --test smoke
```

Expected: all runnable smoke tests pass.

- [ ] **Step 3: Run frontend and diff validation**

Run:

```bash
npm run check
npm run build
git diff --check
git status --short
```

Expected: frontend checks/build pass, no whitespace errors, and only intended Aider/design commits or files are present.

- [ ] **Step 4: Review behavior against acceptance criteria**

Confirm the final diff shows: Aider registry detection; pipx install command; `~/.aider.conf.yml` adapter; OpenAI/Anthropic mappings; managed-key-safe reset; preserved unrelated YAML; no MCP/Skills/Memory support claim; and unchanged hiding of Cursor/Qoder/Qwen Code/Trae Agent.

- [ ] **Step 5: Commit any verification-only fix**

If a focused fix is needed, add a regression test first, rerun the affected command, then commit with a specific message such as:

```bash
git add <changed-files>
git commit -m "fix(aider): <specific correction>"
```
