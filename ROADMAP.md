# ClawBox Backend Roadmap

**Last updated:** 2026-07-03

ClawBox is a desktop GUI for managing multiple agent-runtime CLIs in parallel.
This file tracks the state of backend support.

## Currently supported

| Backend | Version | Module | Capability coverage |
|---|---|---|---|
| **OpenClaw** | 2026.4.11 (verified 2026-07-03) | `backends/openclaw.rs` | gateway, cron, stats, skills, MCP, memory (index-only), plugins, hooks |
| **Hermes** | v0.11.0 (verified 2026-07-03) | `backends/hermes.rs` | gateway, cron, skills, MCP, memory (status+reset), plugins, tools, hooks |

Both are wired through the `Backend` trait + `BackendEntry` registry
(see `docs/plans/2026-07-03-backend-trait-hermes-design.md` and
`2026-07-03-agent-capabilities-design.md`).

## Planned for future iterations

These CLIs are installed on the dev host today and confirmed as agent
runtimes with gateway / capability surfaces. Each will get its own backend
struct (`ClaudeCodeBackend`, `CodexBackend`, ...) implementing the same
`Backend` trait + capability sub-traits.

### Claude Code CLI (Anthropic)

- Binary: `claude` (Homebrew / `~/.local/bin/claude`)
- Version: 2.1.179
- Subcommands: primarily interactive; `--print` (-p) for non-interactive output
- Open questions to resolve before implementation:
  - Does it expose a long-running gateway/WebSocket service, or is it session-only?
    If session-only, the `Backend::gateway_*` methods need a sensible "not applicable" return shape.
  - Skills / MCP / Memory / Plugins / Tools / Hooks — which capabilities map to Claude Code concepts (plugins? sub-agents? permissions? CLAUDE.md?).
- Effort: 1-2 days to scope. Implementation will follow the same pattern as hermes (verify real CLI outputs, write parsers from captured fixtures, TDD).

### Codex CLI (OpenAI)

- Binary: `codex` (Homebrew)
- Version: 0.131.0
- Subcommands: `exec`, `review`, `mcp`, `plugin`, `mcp-server`, `app-server`, `update`, `doctor`, `sandbox`, `debug`, `apply`, `resume`, `fork`, `cloud`
- Strong overlap with openclaw/hermes surface — `mcp` and `plugin` map directly to existing capability traits.
- Likely less hermes-style CLI parsing work than Claude Code (subcommand structure is well-defined).
- Effort: 1 day.

## Out of scope (no plans, can revisit on request)

- `gemini` CLI (Google) — not installed on dev host; no current team request.
- `aider` — pair-programming tool, gateway-like model doesn't apply.
- Custom / third-party agent runtimes built on top of MCP — would only need a thin shim over the MCP capability.

## Future-proofing considerations

The current `BackendEntry { backend, skills, mcp, ... }` registry was
designed with multi-backend in mind. Adding a third backend is mostly:
1. Implement `Backend` + the relevant capability traits.
2. Add the entry to `backends::entries()`.
3. (No aggregate-layer changes — `collect_capability_*` loops are generic.)

If the capability surface itself needs to grow (e.g. a new "Agents" trait
for sub-agent management), add it to `capabilities/mod.rs` the same way
Skills / MCP / etc. were added.

## When adding a new backend

1. Capture real CLI output (`<cli> <command> --help` and a representative run) into a fixture file.
2. Implement the `Backend` trait first (versions + is_installed), TDD-style.
3. Implement capability sub-traits one at a time with parser tests against real fixtures.
4. Wire `BackendEntry` in `entries()` with the backend's capability pointers.
5. Add a smoke test in `tests/smoke.rs` that exercises the trait methods against the live binary.
6. No aggregate-layer changes needed (existing `*_list_all` already loop over `entries()`).
