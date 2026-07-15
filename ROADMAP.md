# ClawBox Backend Roadmap

**Last updated:** 2026-07-15

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

## Supported via ACP (Agent Client Protocol)

Claude Code and Codex were originally planned as per-CLI backends
(`ClaudeCodeBackend`, `CodexBackend` implementing the `Backend` trait).
That approach was superseded: they are now integrated through ACP bridges
spoken over stdio JSON-RPC — no CLI-output parsing needed.

| Agent | ACP bridge | Status |
|---|---|---|
| **Claude Code** (Anthropic) | `claude-agent-acp` | Supported; live-verified against the real bridge |
| **Codex** (OpenAI) | `codex-acp` | Supported via ACP (bridge installable, not yet live-verified) |

The ACP subsystem lives in `src-tauri/src/acp/` (adapters, jsonrpc,
permission, session, review). Adding another ACP-compatible agent is a
registry entry in `acp/adapters.rs`, not a new parser.

Shipped on top of ACP: a read-only multi-agent **code review** feature —
reviewer + summarizer roles, read-only enforced at the ACP permission
layer, reports persisted to `~/.clawbox/reviews/` — exposed via Tauri
commands and the Review page.

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
