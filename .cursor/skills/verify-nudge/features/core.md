# herdr-core backend

`nudge core watch` is one long-running watcher per machine for herdr-core terminals. It takes an advisory lock on
`watch.lock` in the state dir (a second watcher prints `nudge core watch is already running (pid N)` and exits 0),
runs `hc events --json`, and reads its first line as a snapshot: each terminal's status is recorded (a working
terminal's turn starts at its `status_ms`) without notifying. Each later `status` event becomes a `Transition` for the
shared pipeline (the same rules, scope, routes, redaction and switch as the TUI plugin): `blocked` and `error` notify
(an error is sent as `status: "blocked"` with the lead `Agent reported an error (auto-detected by herdr; …)`), `done`
after the shortest turn notifies, the cooldown applies. `finished` / `removed` drop the terminal's state; `missed`
reconciles like the snapshot. Only this core's own terminals (`t7`) are handled unless `[core] watch = "connected"`
(then `box/t7` too). A terminal counts as watched (skipped with `you are looking at this pane`) only when the app
context (`nudge core context`) names its address, `window_active` is true and `at` is under 90 s old; away overrides.
When the
stream ends it reconnects with backoff (1, 2, 5, 10, 30 s, then every 30 s), forever; `--once` exits when the stream
ends instead (exit 1 if no snapshot came). It writes `watch.json` (pid, start, how started, connected, last seq, the
terminals with their space and webhook, the last 20 decisions; no secrets) after every line. Payloads from core have
`source: {backend: "core", host, pane: "<address>"}` and `reply_hint: reply with: hc send <address> "..."`.

`nudge core send <terminal> [--blocked]` sends a core terminal's output to its webhook now (question, or blocked),
following its route, ignoring scope. `nudge notify` inside a core terminal (`$HC_TERMINAL_ID`, and no
`$HERDR_PANE_ID`) is from that terminal: `hc get <id> --json` names it (`--agent` / `$NUDGE_AGENT` rename it), routes
and scope match it by name, address or space id, and `self_notified_at` is recorded under `core:<id>` so the watcher
doesn't send a duplicate.

`nudge core context '<json>'` parses `{terminal, window_active, spaces: [{id, name}], at}` (unknown fields ignored)
and writes `context.json` in the state dir (mode 600); malformed JSON is exit 2 (`core context takes JSON …`) and
nothing is written. The watcher, `nudge core send` and `nudge notify` inside a core terminal name a terminal's space
from it, so routes and `[scope]` match the space name as well as its id.

`nudge status --json` prints one JSON object: `version`, `host`, `on`, `auto`, `away`, `config_error`,
`default_webhook`, `webhooks` (`name`, `host`, `signed`), `routes`, and `core`: `watcher` (`running` only while
`watch.lock` names a live pid; the rest from `watch.json`), `service` (`installed`, `running`, `path`), `context`
(with `fresh`), `spaces` and `terminals` (each with `webhook` and `via`: `pane`, `workspace` or `default`) and
`decisions`. Without `--json` it's exit 2.

## Sub-features

- `core-watch` snapshot silence, done/blocked/error notify, another machine's terminal skipped, a malformed line
  logged (`skipped a line from hc events: not JSON`) and skipped, `hc events ended` on stream end.
- `core-watch-connected` `[core] watch = "connected"` handles `box/t7` too.
- `core-watch-routes` a pane route by terminal name, a workspace route by space id, a `backend = "tui"` route that
  doesn't apply to core.
- `core-watch-scope`, `core-watch-off`, `core-watch-auto-off` skip with `skipped (outside nudge's scope)`,
  `skipped (nudge is off)`, `skipped (auto-notify is off)`.
- `core-watch-no-core` `--once` with no snapshot: exit 1, `hc events ended before its first snapshot: is herdr-core
  running?`.
- `core-send` question / `--blocked`, an unknown terminal (exit 1), nudge off (exit 1).
- `core-notify` notify and test inside a core terminal; an unknown `$HC_TERMINAL_ID` still counts as that terminal.
- `core-cli` usage errors (exit 2), `core context` included.
- `core-context` a context is stored (`files.app_context`); malformed JSON, a wrong type, a missing `at` and an
  option are exit 2 and leave it as it was.
- `core-watch-focus` the focused terminal in an active window with a fresh context is skipped (`t1 done: skipped
  (you are looking at this pane)`); `-stale` (91 s old), `-inactive` and `-away` notify it.
- `core-watch-space-names` workspace routes by the app's space names; `core-watch-space-scope` `[scope]` by space
  name; `core-notify-space-name` `nudge notify` in a core terminal and `nudge core send` route by space name.
- `status-json` before and after `core watch --once` (watcher not running, its terminals and decisions kept, space
  names, `via`), no URL; plain `status` exit 2. `status-json-service` the installed, running login service.
  `status-json-no-config`, `status-json-broken-config` (`config_error` names the problem).
- `core-service` (Linux: a systemd user unit, through fake `systemctl` / `loginctl`) `status` says `Login service:
  not installed`; `install` writes `~/.config/systemd/user/nudge-watch.service` (`ExecStart="<nudge>" core watch`,
  `Restart=on-failure`, `RestartSec=10`, `Environment="PATH=<absolute PATH entries at install>"`,
  `WantedBy=default.target`), runs `systemctl --user daemon-reload`, `enable nudge-watch.service` and `restart
  nudge-watch.service` (so a reinstall restarts a running watcher on the new unit), and with `$USER` set asks
  `loginctl show-user <user> --property=Linger --value`: `no` prints `! Lingering is off for <user> …
  loginctl enable-linger <user>`; `status` then asks `systemctl --user is-active` (`installed, running` /
  `installed, not running`); a reinstall with `PATH=…::.:bin:…` keeps only the absolute entries; `remove` runs
  `disable --now`, deletes the unit and reloads; removing again says `The login service isn't installed.`
- `core-service-command-link` with `~/.local/bin/nudge` present, the unit runs that link instead of the binary.
- `core-service-fails` / `core-service-no-systemd` a failing `systemctl` step is exit 1 naming the command.
- `core-service-cli` usage errors.
- Unit-tested only: the lock (second watcher), the backoff schedule, `missed` reconcile, `removed` forgetting,
  `watch.json` contents, the launchd agent (plist text, `launchctl bootout` / `bootstrap gui/<uid>` / `print`), a
  live watcher's pid in `status --json`.

## How to get to it (user POV)

- `nudge core service install` (or `nudge core watch` in a core terminal or by hand); `nudge core send t7`;
  `nudge notify` from an agent in a core terminal.

## Driving it with verify-nudge

Preconditions:

- `$VN seed-config`; `$VN seed-state '{"auto": true}'`.
- A fake `hc` fixture at `<RUN>/hc.json` (same rule shape as herdr's; parity uses the `hc(...)` op). `hc events --json`
  is one rule whose `stdout` is the snapshot line and the event lines joined by newlines; add `read <t> --lines 200`
  and `get <t> --json` rules as needed. Calls are logged to `<RUN>/hc-calls.jsonl` and appear as `hc_calls`.

- **Watch.** `$VN run -- core watch --once`: stderr `[nudge] t1 done: notified webhook (HTTP 200)` lines; `webhook[]`
  bodies with `source.backend: "core"`; `files.state_dir_entries` include `watch.json` and `watch.lock`.
- **Send.** `$VN run -- core send t1`: `Sent t1 to the webhook (HTTP 200).`
- **Notify.** `$VN run --env HC_TERMINAL_ID=t1 -- notify hi`: `source.pane: "t1"`, state `core:t1.self_notified_at`.
- **Proof.** `webhook` bodies (status, from_agent, reply_hint, source, webhook), stderr decisions, `hc_calls`.
- **Service.** `$VN run --env USER=tester -- core service install` with `systemctl.json` / `loginctl.json` fixtures in
  `<RUN>` (no fixture: the fake exits 1, so the real service managers are never reached); `files.login_service` is
  the unit's text while it exists; `systemctl_calls` / `loginctl_calls` the commands.
- **Context.** `$VN run -- core context '{"terminal":"t1","window_active":true,"spaces":[],"at":<epoch ms>}'`, then
  `core watch --once`. Parity scenarios write `{NOW}` / `{NOW-91000}` in arguments; the transcript shows `<now>`.
- **Status.** `$VN run -- status --json`: `stdout_json` (epoch ms as `<now>`, `host` as `<HOST>`).

## Gotchas

- Without `--once` the watcher never exits; drive it with `--once` in the sandbox.
- The fake stream ends right after its last line, so `--once` exits 0 once the snapshot came.
