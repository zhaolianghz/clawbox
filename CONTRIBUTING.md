# Contributing to ClawBox

Thanks for your interest in improving ClawBox! This document covers how to get set up and the conventions we follow.

This project follows a [Code of Conduct](CODE_OF_CONDUCT.md). By participating, you are expected to uphold it.

## Prerequisites

- **Node.js** ≥ 18
- **Rust** ≥ 1.77 (install via [rustup](https://rustup.rs))
- Platform build dependencies for [Tauri v2](https://tauri.app/start/prerequisites/)

## Getting started

```bash
git clone https://github.com/zhaolianghz/clawbox.git
cd clawbox
npm install
npm run tauri dev
```

## Project layout

| Path | What lives there |
|---|---|
| `src/` | Svelte 5 frontend (routes, components, i18n, data catalogs) |
| `src-tauri/src/commands/` | Tauri command handlers (thin wrappers) |
| `src-tauri/src/sync/` | Sync engines — providers / MCP / skills / memory |
| `src-tauri/src/backends/` | Per-agent CLI adapters |
| `src/lib/i18n/` | `en.json` / `zh.json` — keep both in sync |

## Before you open a PR

Run the full check suite — CI runs the same:

```bash
npm run check                              # svelte-check: must be 0 errors, 0 warnings
cargo test --lib --manifest-path src-tauri/Cargo.toml   # Rust unit tests
```

### Conventions

- **i18n**: every user-facing string goes through `svelte-i18n`. Add the key to **both** `en.json` and `zh.json` — the key sets must match exactly.
- **Tests never touch real config**: backend tests run against an isolated temp `$HOME`. Never read or write the developer's real `~/.claude`, `~/.codex`, etc.
- **Merge-write, never overwrite**: sync adapters only touch the keys/blocks ClawBox manages and back up before writing. User content stays byte-for-byte intact.
- **Commit messages**: conventional style (`feat:`, `fix:`, `docs:`, `refactor:`, `chore:`).

## Adding a provider to the catalog

Provider entries live in `src/lib/data/providers.ts`. Include the exact API base URL, brand color, and a bilingual `description` (`{ en, zh }`). Verify the endpoint against the provider's official docs before submitting.

## Reporting bugs & requesting features

Open an issue using the templates. For security issues, see [SECURITY.md](SECURITY.md).
