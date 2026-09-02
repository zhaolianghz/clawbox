# Aider Integration Design

## Goal

Add Aider to ClawBox's Agent management page with reliable installation detection and reversible Provider configuration synchronization on macOS and Windows. Linux support is not required.

## Scope

### Included

- Register Aider as a native CLI Agent with binary `aider`.
- Install Aider through the existing Agent installation flow using `pipx install aider-chat`.
- Detect the installed version through `aider --version`.
- Add a Provider adapter for `~/.aider.conf.yml`.
- Synchronize the bound Provider's model, endpoint, and credentials while preserving unrelated YAML fields.
- Remove only keys previously managed by ClawBox on unbind or reset.
- Expose Aider in the existing Agent and Provider overview UI.
- Add focused Rust tests and run existing frontend/backend checks.

### Excluded from this phase

- MCP synchronization for Aider.
- Skills synchronization for Aider.
- Memory synchronization for Aider.
- Changes to Rust Provider data structures.
- Linux-specific installation or support requirements.

## Aider Configuration Contract

The adapter manages the global YAML file:

```text
~/.aider.conf.yml
```

For an OpenAI-compatible Provider, the managed keys are:

```yaml
model: <provider model>
openai-api-base: <provider.openai_base_url>
openai-api-key: <provider.openai_api_key>
```

For an Anthropic Provider, the managed keys are:

```yaml
model: <provider model>
anthropic-api-base: <provider.anthropic_base_url>
anthropic-api-key: <provider.anthropic_api_key>
```

Only non-empty values are written. If a selected Provider has no usable endpoint or model, planning skips deployment rather than writing an incomplete configuration. Existing unrelated YAML keys and values must survive a merge. Managed-key bookkeeping must distinguish keys written by ClawBox from user-owned keys so unbind does not remove user configuration.

## Installation Contract

Extend `InstallMethod` with a Python package installation variant that runs:

```bash
pipx install aider-chat
```

The implementation must use the existing command execution and platform handling conventions. If `pipx` is unavailable, the install operation returns an actionable error referring to pipx/Aider installation documentation; it must not silently fall back to mutating the system Python environment.

## Architecture

- `src-tauri/src/agents/mod.rs` remains the single Agent registry and receives the Aider definition.
- `src-tauri/src/agents/install.rs` and related display helpers handle the pipx installation variant.
- `src-tauri/src/sync/providers.rs` receives `AiderProviderAdapter`, following the existing merge, plan, apply, validation, and managed-key conventions.
- The existing overview and frontend Agent page consume the new registry/adapter entry without new page-specific data structures.
- `AgentLogo.svelte` receives a stable Aider visual mapping. The existing unsupported-agent hiding list remains limited to Cursor, Qoder, Qwen Code, and Trae Agent.

## Data Flow

1. The backend registry detects `aider` and reports its version and installation state.
2. The frontend requests the existing sync overview and renders Aider using the existing Agent card.
3. When a Provider is bound, the provider sync planner reads `~/.aider.conf.yml`, computes add/update/remove/unchanged changes, and records the exact managed keys.
4. Apply writes a YAML document atomically according to existing adapter conventions.
5. Unbind/reset uses managed-key bookkeeping to remove only ClawBox-owned values.

## Error Handling

- Missing Aider config is treated as an empty document and can be created on apply.
- Malformed YAML produces an agent-scoped plan error and leaves the file untouched.
- Missing endpoint, missing model, or incompatible Provider slot produces a skip/no-deployment result, not a malformed file.
- Missing `pipx` produces an explicit install error.
- YAML values must be serialized safely; API keys must not be emitted in user-facing error details.

## Testing

Add tests for:

- Aider registry metadata and pipx command formatting.
- Empty-file creation and idempotent re-planning.
- Preservation of unrelated YAML fields/comments where supported by the existing YAML strategy.
- OpenAI-compatible and Anthropic Provider mappings.
- Empty endpoint/model skip behavior.
- Malformed YAML safety.
- Unbind cleanup limited to ClawBox-managed keys.
- Adapter/Agent registry consistency.

Verification commands:

```bash
npm run check
npm run build
cd src-tauri && cargo test
cd src-tauri && cargo test --test smoke

git diff --check
```

## Acceptance Criteria

- Aider appears in the Agent management page when installed or available for installation.
- Aider can be installed using the existing UI on supported macOS/Windows environments when pipx is present.
- Binding an eligible Provider produces a valid `~/.aider.conf.yml` without deleting unrelated user settings.
- Repeating sync is idempotent.
- Unbinding removes only fields managed by ClawBox.
- No MCP, Skills, or Memory controls are shown as supported for Aider in this phase.
- All existing tests and the new focused tests pass.
