# Security Policy

## Reporting a vulnerability

Please **do not** open a public issue for security vulnerabilities.

Instead, report it privately via [GitHub Security Advisories](https://github.com/zhaolianghz/clawbox/security/advisories/new), or email the maintainer. We aim to respond within 72 hours.

## Scope & design notes

ClawBox is a **local desktop tool**. It reads and writes configuration files on your own machine and never transmits your API keys, provider endpoints, or memory content to any external service.

Safety properties relevant to security review:

- **Merge-write only** — sync engines modify only the specific keys or the `<!-- CLAWBOX_START -->…<!-- CLAWBOX_END -->` block that ClawBox manages; content outside is left byte-for-byte unchanged.
- **Backups before every write** — target files are copied to `~/.clawbox/backups/<timestamp>/` before modification.
- **Refuse-on-ambiguity** — if a managed block's markers are broken or duplicated, ClawBox refuses to modify the file rather than guess.
- **API keys** are stored in the user's local `~/.clawbox/config.json` and written into each agent's own config files. Key material is never rendered into sync previews or logs.

A known follow-up (tracked for a future release): API keys are currently stored in plaintext in the local config. OS keychain integration is planned.
