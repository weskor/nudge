---
name: verify-nudge
description: Drive the real `nudge` CLI / herdr-plugin binary (any build, via $NUDGE_BIN) the way a user or herdr does, in a sandbox with a fake webhook (signature-checking), fake herdr and fake clipboard, and capture proof. Use to verify notify/list/test, setup (scripted and interactive PTY), the full-screen `nudge settings` editor (PTY, rendered-screen matching), doctor, request signing, plugin events/actions/startup, away mode, [auto] settings, [scope] and the MCP server, or to diff two builds (e.g. before/after a change, in CI) with `verify-nudge parity`.
---

# verify-nudge

`nudge` is a short-lived CLI (plus herdr plugin entrypoints and a stdio MCP server) that POSTs JSON to a
"main" webhook. There is no server to keep alive: every drive is one isolated process. The helper
`bin/verify-nudge` (python3 stdlib only) owns the sandbox, the fakes and the evidence.

Paths below are relative to the repo root. `VN=.cursor/skills/verify-nudge/bin/verify-nudge`
(`.claude/skills/verify-nudge` is a symlink to the same directory).

Safety (enforced by the helper; do not bypass it by running `nudge` directly):

- Never run nudge against the real `~/.config/nudge`, `~/.local/state/nudge`, a real webhook, the real herdr
  server (it opens overlays/toasts on the user's screen) or the real clipboard. The helper builds the child env
  from scratch: `HOME`, `XDG_*`, `NUDGE_CONFIG`, `NUDGE_STATE_DIR` point into the sandbox; `PATH` is
  `<sandbox>/fakebin:/usr/local/bin:/usr/bin:/bin` with fake `herdr`, `wl-paste`, `xclip` first (sh wrappers
  running the fakes with the helper's own python, so the machine needs only `sh`, `git` and `python3`);
  `HERDR_BIN_PATH` is the fake herdr; `DISPLAY`, `WAYLAND_DISPLAY`, `HERDR_PANE_ID`, `HERDR_WORKSPACE_ID`,
  `HERDR_PLUGIN_*`, `NUDGE_AGENT` are absent unless a step sets them. `LANG=LC_ALL=C.UTF-8`.
- Real `herdr`/`wl-paste` may exist in `/usr/bin` on a dev machine; the fakes only win because they come first
  on PATH. Never put the real `PATH` into a drive.

## Launch

Binary under test: `NUDGE_BIN` (default `bin/nudge`, the repo launcher: runs `dist/nudge` if present, else
`target/release/nudge`; or point `NUDGE_BIN` at `target/release/nudge` directly). Nothing else is assumed about
the build.

1. Build once: `cargo build --release`. If a stale `dist/nudge` exists, `bin/nudge` prefers it; use
   `NUDGE_BIN=$PWD/target/release/nudge` to be sure you test the fresh build.
2. Start a session:

   ```sh
   $VN launch
   ```

   Ready when it prints `verify-nudge: ready session=<id>` followed by the webhook URL, sandbox and evidence paths.
   Launch creates `/tmp/verify-nudge-<id>-*/` (sandbox + fake webhook state), starts `bin/fake-webhook` on a free
   127.0.0.1 port (its own process group; PID recorded), and writes `.verify/<id>/session.json` and
   `.verify/current`. The sandbox starts empty: no config, no state, herdr "not installed", empty clipboard.
   To run two sessions side by side, `export VERIFY_NUDGE_SESSION=<id>` in each shell; sessions share nothing.
3. Seed what the journey needs (preconditions, not proof):

   ```sh
   $VN seed-config [--default NAME]   # webhooks: main (default) research badauth(401) nohook(404) broken(500) offline(refused) signed(signing_secret)
   $VN seed-config --append '[scope]\nworkspaces = ["api-work"]'   # standard webhooks + an [auto]/[scope] table (\n expands)
   $VN herdr running                  # or: none | agent-pane | workspaces | path/to/fixture.json
   $VN seed-state '{"auto": true, "panes": {"p_1": {"working_since": {"ago": 180000}}}}'   # add "away": true for away mode
   $VN clip 'http://127.0.0.1:<port>/hook'     # or: $VN clip --clear
   $VN git-repo repo feature/x git@github.com:acme/api.git   # <sandbox>/repo, for `run --cwd repo`
   $VN link /opt/old-nudge/bin/nudge  # ~/.local/bin/nudge in the sandbox; or {NUDGE_ROOT}/bin/nudge | --file | --remove
   ```

Teardown is `$VN cleanup` (see Cleanup).

## Doctor

```sh
$VN doctor
```

Read-only. Each line is `ok`/`FAIL`: session not cleaned, sandbox exists, the webhook PID is our
`fake-webhook` and alive, the port answers `GET /__verify` with this session id (so the port is ours),
`NUDGE_BIN` is executable, `herdr`/`wl-paste`/`xclip` resolve to the fakes inside the sandbox PATH, HOME/config/state
are inside the sandbox, no display/pane/agent variables leak, and `nudge version` exits 0. It ends with an `info`
line naming the checkout revision (and `(dirty)`) and the nudge root. Exit 1 if anything failed: do not drive
until it is clean; `$VN cleanup && $VN launch` is the reset.

## Drive

All drives run the binary with the sandbox env, default cwd `<sandbox>/work` (not a git repo). Each prints a JSON
record and writes it to `.verify/<id>/NNN-<label>.json`: argv, stdin, env overrides, exit, stdout, stderr, the
webhook requests it caused (method, path, authorization/content-type/user-agent/x-nudge-timestamp/x-nudge-signature
headers, parsed body, the fake webhook's `signature` verdict), the fake herdr calls (argv), the clipboard calls, and the
resulting files (config text + mode + dir mode, state.json, `~/.local/bin/nudge` link). Paths are shown as `<RUN>`,
the install root as `<NUDGE_ROOT>`, the port as `<PORT>`; ids/timestamps are normalized as in Parity. The record also
lists `secret_leaks` and `contract_problems` (see Parity), and the command prints a `WARNING:` line for each.

- One command (stdin is a pipe, never a terminal):

  ```sh
  $VN run [--stdin TEXT | --stdin-file F] [--env K=V]... [--cwd DIR] [--expect EXIT] -- notify --status blocked "Need the DB password"
  ```

  `--stdin` understands `\n`, `\r`, `\t`, `\xNN`. `--cwd` is relative to the sandbox (`work`, `home`, or a repo
  you created) or absolute. `--expect` makes the helper exit 1 on a different exit code. `{RUN}` in `--env`
  values expands to the sandbox path.
- Interactive terminal (PTY, 120x40, `TERM=xterm-256color`, controlling tty; output keeps ANSI colors):

  ```sh
  $VN pty --expect 'Press any key to close' --send x -- doctor --pane
  ```

  Steps run in order. `--expect RE` waits (15 s) for a regex in output after the previous match; `--send TEXT`
  waits 0.2 s (so the program has switched the terminal to raw mode) then types it (`\r` Enter, `\x03` Ctrl-C, `\x13`
  Ctrl-S, `\x15` Ctrl-U, `\x7f` backspace, `\e` Esc, `\e[A`/`\e[B`/`\e[C`/`\e[D` arrows, `\e[200~…\e[201~` a bracketed
  paste); `--clip TEXT` changes the fake clipboard mid-run. `Press any key to close` ends `--pane` runs.
- Full-screen UIs (`nudge settings`, the interactive `nudge setup` form; ratatui): their bytes are cursor-addressed
  redraws of changed cells, so match the rendered screen instead: `--screen RE` waits for RE (multiline mode) on a
  120x40 terminal emulation in which each row starts with `> ` when it is highlighted (the cursor row) and `  `
  otherwise, then for 0.25 s of quiet, and keeps that screen in the record's `screens`. With any `--screen` step,
  `output` is the text left on the normal screen after exit (`Saved nudge settings to …`, setup's `✓ Saved …` / `Done.`
  lines, `Cancelled; nothing saved.`), not the raw bytes. Recipes: [features/settings.md](features/settings.md),
  [features/setup.md](features/setup.md).

  ```sh
  $VN pty --screen 'Auto-notify' --send ' ' --screen '^> .*\[x\] Auto-notify' --send s --expect-exit 0 -- settings
  ```
- MCP over stdio (sends `initialize` protocol `2025-06-18`, `notifications/initialized`, `tools/list`, then each call):

  ```sh
  $VN mcp --call list_webhooks '{}' --call notify_webhook '{"message":"Done via MCP","status":"question"}'
  ```

- herdr plugin entrypoints are plain commands with herdr's env, e.g.
  `$VN run --cwd home --env 'HERDR_PLUGIN_EVENT_JSON={"data":{"pane_id":"p_1","agent_status":"blocked"}}' -- plugin event`,
  `$VN run --cwd home --env 'HERDR_PLUGIN_CONTEXT_JSON={"focused_pane_id":"p_1","focused_pane_status":"blocked"}' -- plugin action send-pane`,
  `$VN run --cwd home -- plugin action away-on` (away mode: state.json `away`, see features/plugin-actions.md).
- Inspect without running nudge: `$VN files`, `$VN requests` (raw webhook log), `$VN env` (exports, for a manual shell —
  prefer `run`).

The fake webhook (`bin/fake-webhook`): `POST /hook` answers 200 `ok` only for `Authorization: Bearer
good-token-0123456789abcdef`, else 401 `{"message":"Invalid API key"}`; `POST /signed` additionally needs a valid
`x-nudge-signature: sha256=<hex HMAC-SHA256(secret, "<x-nudge-timestamp>.<raw body>")>` for one of the secrets the
helper passes it (`sign-secret-0123456789abcdef`, the seeded `signed` webhook's; `setup-secret-fedcba9876543210`, the one
parity hands to `setup --signing-secret`), else 401 `{"message":"Invalid signature"}`; `/fail` answers 500
`webhook exploded`; `POST /moved` answers 302 to `/landing` (whose GET answers 200); `POST /echo` answers 400 with the
request's Authorization header in ANSI escapes; `POST /slow` answers 200 after 3 s; any other path 404 `no such hook`.
Each logged request carries `"signature": "valid:<signed-webhook|setup>"
| "invalid" | "missing"` when it has a signature header or targets `/signed`, and its receive time (`received_unix`).
The offline webhook URL is a closed port.

The fake herdr (`bin/fake-herdr`) answers from `<sandbox>/herdr.json` (`{"rules":[{"argv":[...]|"prefix":[...],
"stdout": str|json, "exit": n}], "default_exit": 1}`; `{RUN}` in stdout expands to the sandbox) and logs every
call. Presets: `none` (no fixture: every call exits 1, like herdr missing), `running` (`--version`, plugin
`nudge` enabled at `<RUN>/plugin`, 7 plugin log entries, no agents, `notification show` and
`plugin pane open` succeed), `agent-pane` (running + agent `claude-1` in pane `p_1`, cwd `<RUN>/repo`, unfocused,
`pane read p_1` returns 45 numbered build lines, then ANSI, box rules, blank runs and planted secrets),
`workspaces` (running + herdr's real `agent get` / `pane get` / `workspace get` JSON shapes: workspace `wD` labelled
`api-work` with agent `claude-1` in `wD:p1` and a plain shell in `wD:p3`, workspace `wE` labelled `web-work` with agent
`reviewer` in `wE:p2`; `agent list` puts `claude-1` on `<RUN>/work`; `pane read` works for `wD:p1` and `wE:p2`).

Feature recipes: [features/README.md](features/README.md).

### Parity (old vs new build)

```sh
NUDGE_BIN=/path/to/old/nudge $VN parity .verify/parity/old.json
NUDGE_BIN=/path/to/new/nudge $VN parity .verify/parity/new.json
diff -u .verify/parity/old.json .verify/parity/new.json
```

Runs the fixed scenario list (218 scenarios; `--only SUBSTRING` repeatable filters by name) each in a fresh sandbox
with its own fakes and one shared fake webhook, then writes one JSON transcript with sorted keys. Per step it
records the same fields as `run`, with JSON payloads (webhook bodies, `list --json` stdout as `stdout_json`, MCP
responses keyed by JSON-RPC id with JSON tool text as `text_json`, state.json) compared as data. Multi-line text is
split into line arrays so `diff -u` points at the changed line; config text stays verbatim. Normalized: sandbox
path `<RUN>`, install root `<NUDGE_ROOT>` (= dirname(dirname(realpath NUDGE_BIN))) and an older install next to it
`<NUDGE_SIBLING>` (`{NUDGE_SIBLING}` in scenario links), ports `<PORT>`/`<OFFLINE_PORT>`,
state epoch-ms values as `<now>` / `<now-Nms>` (values the harness seeded) / `<epoch-ms>`, the relative age at the
end of doctor's `Recent:` lines (`just now`, `5m ago`, `371d ago`) as `<ago>`, any lowercase UUID v4 as `<uuid>` and any
`YYYY-MM-DDTHH:MM:SSZ` as `<sent_at>` (a malformed `id`/`sent_at` stays raw so the diff shows it), a webhook
`x-nudge-timestamp` within 60 s of receipt as `<ts>`, a well-formed `x-nudge-signature` as `sha256=<hmac>` (whether it
verified is the request's `signature` field), `received_unix` dropped, and fake herdr calls sorted (nudge issues some
concurrently). The version in `user-agent` is kept. Each scenario lists `check_failures`: unexpected exit code,
expectation not seen in a PTY, missing MCP response, a webhook secret (Authorization token or signing secret) in stdout,
stderr, PTY output, an MCP response, a webhook body or a herdr call, harness error, and these contract problems:
a `/signed` request whose signature is `invalid`/`missing`, `x-nudge-*` headers sent to a webhook without
`signing_secret`, an `x-nudge-timestamp` that is not unix seconds within 60 s, a webhook body without a lowercase UUID v4
`id` or a `sent_at` in that format within 60 s, and an MCP `notify_webhook` result whose `id`/`sent_at` is missing or is
not the request the webhook received. Steps may also assert `sends=N` (requests the webhook got), `toasts=[…]` (exact
`notification show nudge --body` bodies), `has=TEXT|[…]` / `lacks=[…]` (in / not in the output, PTY screens included),
`scope_is=(workspaces, agents)` (the config's `[scope]` after the step), `config_keys={key: value}` (top-level config
keys after the step, `None` = absent), `state_auto=BOOL`, `state_keys={key: value}` (top-level state.json keys after the
step, `None` = absent, e.g. `{"away": None}`), `config_mode="600"`, `config_unchanged=True` (config text
identical to before the step) and `herdr_has=[argv…]` (exact herdr calls made); TOML checks need python ≥ 3.11 for
`tomllib`. PTY steps that use `screen` expectations record the matched `screens` and the normal-screen `output` instead
of raw bytes (the raw bytes are still scanned for secrets). The command prints the failures and exits 1 if there are
any (0 otherwise), so CI can run it directly:
`NUDGE_BIN=$PWD/target/release/nudge .cursor/skills/verify-nudge/bin/verify-nudge parity $RUNNER_TEMP/parity.json`.
Two runs of the same binary must produce identical files — if not, the harness has a nondeterminism bug; fix it
before trusting a cross-build diff. Transcripts from an older harness (`"harness"` other than `verify-nudge parity v4`)
differ in scenarios and normalization; regenerate both sides with the same harness before diffing. A build without
away mode (before 0.8.0) fails the `*away*` scenarios and the `settings-*` cursor steps (the `I'm away` row
moves every cursor stop below `Auto-notify` down by one): that is the expected diff, not a harness problem.

## Evidence

- Session drives: `.verify/<id>/NNN-<label>.json` (one per `run`/`pty`/`mcp`), `.verify/<id>/session.json`, and
  after cleanup `.verify/<id>/webhook-requests.jsonl` (every request the fake webhook received). Parity: the
  transcript path you pass (convention `.verify/parity/<build>.json`). `.verify/` is git-ignored.
- Proof standards:
  - Drive the user path: `nudge …` commands, a PTY for interactive setup, herdr's env vars for plugin
    entrypoints. Do not call internals or edit config/state to fake an outcome; seeding preconditions is fine.
  - Capture the action and the resulting state: the record's `webhook` (what the webhook got), `files` (config
    mode 600 / dir 700, state.json, command link), `herdr_calls` (toasts, overlays opened) — not only stdout.
  - Side effects are part of the proof: "nothing saved" means `files.config` is `null`; "no request sent" means
    `webhook` is `[]`; a hidden header means the token text is absent from `output` (`secret_leaks` is `[]`); a
    signed request means the request's `signature` is `valid:…`.
  - Mocks stop only at real process boundaries nudge already has: HTTP to the webhook, the `herdr` CLI, the
    clipboard CLIs. Git is real (scenarios create temp repos).
- Read-only second view for mutations: after `setup`, `$VN run -- list` and `$VN files`.

## Cleanup

```sh
$VN cleanup
```

Stops the fake webhook only if its recorded PID's command line is our `fake-webhook` for this session's scratch
dir, copies the webhook log into the evidence dir, deletes `/tmp/verify-nudge-<id>-*`, marks the session cleaned and
clears `.verify/current`. It never touches `.verify/<id>/` evidence or parity transcripts and never kills by
name. Drives are synchronous; a PTY or MCP child that outlives its timeout is killed by the helper that started it.
`parity` cleans up its own webhook and sandbox even on failure. Run cleanup after every failed attempt too.

## Helpers

All in `.cursor/skills/verify-nudge/bin/`, executable, python3 stdlib:

- `verify-nudge` — every command above; `$VN --help` prints the usage.
- `fake-webhook --dir DIR --id ID [--secret LABEL=SECRET]...` — the webhook; started by `launch`/`parity`, not by hand.
- `fake-herdr` — run by the sh wrapper `<sandbox>/fakebin/herdr`; fixture `<sandbox>/herdr.json`, log `<sandbox>/herdr-calls.jsonl`.
- `fake-clipboard` — run by the wrappers `<sandbox>/fakebin/wl-paste` and `xclip`; prints `<sandbox>/clipboard.txt` (exit 1 if empty),
  logs to `<sandbox>/clipboard-calls.jsonl` with `DISPLAY`/`WAYLAND_DISPLAY`/`XDG_RUNTIME_DIR`.
