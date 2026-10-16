# nudge verification map

This directory is the maintained source for verifying the user-facing behavior of nudge: the `nudge` CLI, the
herdr plugin entrypoints herdr runs (`nudge plugin …`), and the stdio MCP server. Read the index before driving,
then use the matching feature file as the recipe. `VN=.cursor/skills/verify-nudge/bin/verify-nudge`.

## Baseline preconditions

- `$VN launch` printed `verify-nudge: ready session=<id>` and `$VN doctor` exits 0 for the binary you mean to test
  (`NUDGE_BIN`, default `bin/nudge`).
- The sandbox starts empty: no config, no state, fake herdr not installed, empty clipboard.
- Standard seed when a journey needs desks: `$VN seed-config` → `desk` (default, accepts), `research` (accepts),
  `badauth` (401), `nohook` (404), `broken` (500), `offline` (connection refused), `signed` (URL `/signed`,
  `signing_secret = "sign-secret-0123456789abcdef"`; the fake desk accepts only validly signed requests).
  `--append TOML` adds an `[auto]` / `[scope]` table.
- The good Authorization value is `Bearer good-token-0123456789abcdef`; anything else is rejected by `/hook`.
- Every notification body carries `id` (UUID v4) and `sent_at` (`YYYY-MM-DDTHH:MM:SSZ`); records show them as
  `<uuid>` / `<sent_at>` and list malformed ones in `contract_problems`.
- Never drive a session this run did not launch, and never run `nudge` outside `$VN` (real config, herdr, clipboard).

## Driving conventions

- Commands: `$VN run [--stdin T] [--env K=V] [--cwd D] [--expect N] -- <nudge args>`.
- Terminal journeys: `$VN pty (--expect RE | --send TEXT | --clip TEXT)... [--expect-exit N] -- <nudge args>`;
  always `--expect` the prompt (ends with `›`) before `--send`.
- MCP: `$VN mcp --call TOOL 'JSON' ...`.
- herdr plugin entrypoints run with `--cwd home` and herdr's variables: `HERDR_PLUGIN_EVENT_JSON`,
  `HERDR_PLUGIN_CONTEXT_JSON`, `HERDR_PANE_ID`, `HERDR_PLUGIN_ID`.
- Treat commands and prompt strings as literal. Re-seed (or `$VN cleanup && $VN launch`) between journeys that
  need a different starting state.

## Proof and skip reporting

- Each drive writes `.verify/<id>/NNN-*.json` with argv, exit, stdout, stderr, webhook requests, herdr calls,
  clipboard calls and file state; cite those files.
- A delivery claim needs the `webhook` entry (headers + body). A "nothing saved/sent" claim needs `files.config: null`
  or `webhook: []`. A secrecy claim needs `secret_leaks: []` and the token absent from `output`. A signing claim needs
  the request's `signature: "valid:…"` (and no `x-nudge-*` headers on requests to unsigned desks).
- Mutations get a read-only second view: `$VN run -- list`, `$VN files`.
- For cross-build equivalence use `$VN parity` on both builds and `diff -u`; the scenario names map to the
  sub-feature IDs below.
- Report an unreachable path with the attempted command and the unmet precondition; never report one entry
  point as verified through another.

## Feature entry contract

Each feature file starts with an H1 title and one paragraph describing the user-visible behavior, then exactly
four H2 sections in this order: `Sub-features`, `How to get to it (user POV)`, `Driving it with verify-nudge`
(starting with `Preconditions:`), `Gotchas`.

## Features

- [Notify, test and list](./notify.md): send a notification or test ping to a desk; agent, git and stdin detection; id/sent_at; request signing; desk errors; listing desks.
- [Setup](./setup.md): connect a desk scripted (flags/stdin, `--signing-secret`) or interactively in a terminal (clipboard, typing, hidden header, re-asks, Ctrl-C).
- [Doctor](./doctor.md): health report for config, scope, webhook ping, herdr plugin, auto-notify, command link, clipboard.
- [Plugin auto-notify](./plugin-auto-notify.md): `pane.agent_status_changed` events become desk notifications under the turn/focus/cooldown rules (`[auto]` settings), with redaction and a "Told the desk" toast.
- [Plugin actions and startup](./plugin-actions.md): toggle/enable/disable, ask-desk, setup/status overlays, scope actions, startup command-link repoint.
- [Scope](./scope.md): `[scope]` limits which workspaces/agents may notify (events, notify, MCP); only-workspace / only-agent / all-agents actions; doctor's scope line.
- [MCP server](./mcp.md): `nudge mcp` with `list_desks` and `notify_desk`.
