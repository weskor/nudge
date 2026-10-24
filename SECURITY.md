# Security

nudge handles webhook credentials and can send terminal output to a remote webhook, so security reports are taken seriously.

## Reporting a vulnerability

Please report vulnerabilities privately through [GitHub's private vulnerability reporting](https://github.com/weskor/nudge/security/advisories/new), not in a public issue. Include steps to reproduce and the nudge version (`nudge --version`).

## Scope

In scope:

- Leaking the Authorization header or other secrets: into logs, error messages, terminal output, or files with loose permissions.
- Terminal output reaching the webhook when the rules say it shouldn't (auto-notify off, focused pane, cooldown), or redaction missing a common credential format.
- The install step accepting a binary that doesn't match its release checksum.

Out of scope:

- What the automation behind your webhook does with an event. It is your own automation; see [docs/webhook-instructions.md](docs/webhook-instructions.md) for safe instructions.
- Secrets in formats that redaction doesn't claim to cover. Redaction is documented as best-effort.
