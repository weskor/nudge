---
name: verify-nudge
description: Drive the real `nudge` CLI / herdr-plugin binary (any build, via $NUDGE_BIN) the way a user or herdr does, in a sandbox with a fake desk webhook, fake herdr and fake clipboard, and capture proof. Use to verify notify/list/test, setup (scripted and interactive PTY), doctor, plugin events/actions/startup and the MCP server, or to diff two builds (e.g. before/after a change, in CI) with `verify-nudge parity`.
---

# verify-nudge

`nudge` is a short-lived CLI (plus herdr plugin entrypoints and a stdio MCP server) that POSTs JSON to a
"desk" webhook. There is no server to keep alive: every drive is one isolated process. The helper
`bin/verify-nudge` (python3 stdlib only) owns the sandbox, the fakes and the evidence.

Paths below are relative to the repo root. `VN=.cursor/skills/verify-nudge/bin/verify-nudge`
(`.claude/skills/verify-nudge` is a symlink to the same directory).

Safety (enforced by the helper; do not bypass it by running `nudge` directly):

- Never run nudge against the real `~/.config/nudge`, `~/.local/state/nudge`, a real webhook, the real herdr
  server (it opens overlays/toasts on the user's screen) or the real clipboard. The helper builds the child env
  from scratch: `HOME`, `XDG_*`, `NUDGE_CONFIG`, `NUDGE_STATE_DIR` point into the sandbox; `PATH` is
  `<sandbox>/fakebin:/usr/local/bin:/usr/bin:/bin` with fake `herdr`, `wl-paste`, `xclip` first (sh wrappers
  running the fakes with the helper's own python, so the machine needs only `sh`, `git` and `python3`);
  `HERDR_BIN_PATH` is the fake herdr; `DISPLAY`, `WAYLAND_DISPLAY`, `HERDR_PANE_ID`, `HERDR_PLUGIN_*`,
  `NUDGE_AGENT` are absent unless a step sets them. `LANG=LC_ALL=C.UTF-8`.
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
   $VN seed-config [--default NAME]   # desks: desk (default) research badauth(401) nohook(404) broken(500) offline(refused)
   $VN herdr running                  # or: none | agent-pane | path/to/fixture.json
   $VN seed-state '{"auto": true, "panes": {"p_1": {"working_since": {"ago": 180000}}}}'
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
webhook requests it caused (method, path, authorization/content-type/user-agent headers, parsed body), the fake
herdr calls (argv), the clipboard calls, and the resulting files (config text + mode + dir mode, state.json,
`~/.local/bin/nudge` link). Paths are shown as `<RUN>`, the install root as `<NUDGE_ROOT>`, the port as `<PORT>`.

- One command (stdin is a pipe, never a terminal):

  ```sh
  $VN run [--stdin TEXT | --stdin-file F] [--env K=V]... [--cwd DIR] [--expect EXIT] -- notify --status blocked "Need the DB password"
  ```

  `--stdin` understands `\n`, `\r`, `\t`, `\xNN`. `--cwd` is relative to the sandbox (`work`, `home`, or a repo
  you created) or absolute. `--expect` makes the helper exit 1 on a different exit code. `{RUN}` in `--env`
  values expands to the sandbox path.
- Interactive terminal (PTY, 120x40, `TERM=xterm-256color`, controlling tty; output keeps ANSI colors):

  ```sh
  $VN pty --expect '1/2 Copy the Webhook URL[^\n]*›' --send '\r' \
          --expect '2/2 Copy the Authorization header[^\n]*›' --clip 'Bearer good-token-0123456789abcdef' --send '\r' \
          --expect 'Install the[^\n]*›' --send 'n\r' --expect 'Done\.' --expect-exit 0 -- setup
  ```

  Steps run in order. `--expect RE` waits (15 s) for a regex in output after the previous match; `--send TEXT`
  waits 0.2 s (so the prompt has switched to raw mode) then types it (`\r` Enter, `\x03` Ctrl-C, `\x7f`
  backspace); `--clip TEXT` changes the fake clipboard mid-run. Prompts end with `›`; stable prompt strings:
  `1/2 Copy the Webhook URL`, `2/2 Copy the Authorization header`, `↻ Copy the …` (re-ask), `No clipboard text
  found. Paste or type the …`, `Turn on auto-notify?`, `Install the nudge command`, `Save anyway?`,
  `Press any key to close` (`--pane`).
- MCP over stdio (sends `initialize` protocol `2025-06-18`, `notifications/initialized`, `tools/list`, then each call):

  ```sh
  $VN mcp --call list_desks '{}' --call notify_desk '{"message":"Done via MCP","status":"question"}'
  ```

- herdr plugin entrypoints are plain commands with herdr's env, e.g.
  `$VN run --cwd home --env 'HERDR_PLUGIN_EVENT_JSON={"data":{"pane_id":"p_1","agent_status":"blocked"}}' -- plugin event`,
  `$VN run --cwd home --env 'HERDR_PLUGIN_CONTEXT_JSON={"focused_pane_id":"p_1","focused_pane_status":"blocked"}' -- plugin action ask-desk`.
- Inspect without running nudge: `$VN files`, `$VN requests` (raw webhook log), `$VN env` (exports, for a manual shell —
  prefer `run`).

The fake desk (`bin/fake-webhook`): `POST /hook` answers 200 `ok` only for `Authorization: Bearer
good-token-0123456789abcdef`, else 401 `{"message":"Invalid API key"}`; `/fail` answers 500 `desk exploded`;
any other path 404 `no such hook`. The offline desk URL is a closed port.

The fake herdr (`bin/fake-herdr`) answers from `<sandbox>/herdr.json` (`{"rules":[{"argv":[...]|"prefix":[...],
"stdout": str|json, "exit": n}], "default_exit": 1}`; `{RUN}` in stdout expands to the sandbox) and logs every
call. Presets: `none` (no fixture: every call exits 1, like herdr missing), `running` (`--version`, plugin
`weskor.nudge` enabled at `<RUN>/plugin`, 7 plugin log entries, no agents, `notification show` and
`plugin pane open` succeed), `agent-pane` (running + agent `claude-1` in pane `p_1`, cwd `<RUN>/repo`, unfocused,
`pane read p_1` returns 45 numbered build lines, then ANSI, box rules, blank runs and planted secrets).

Feature recipes: [features/README.md](features/README.md).

### Parity (old vs new build)

```sh
NUDGE_BIN=/path/to/old/nudge $VN parity .verify/parity/old.json
NUDGE_BIN=/path/to/new/nudge $VN parity .verify/parity/new.json
diff -u .verify/parity/old.json .verify/parity/new.json
```

Runs the fixed scenario list (114 scenarios; `--only SUBSTRING` repeatable filters by name) each in a fresh sandbox
with its own fakes and one shared fake webhook, then writes one JSON transcript with sorted keys. Per step it
records the same fields as `run`, with JSON payloads (webhook bodies, `list --json` stdout as `stdout_json`, MCP
responses keyed by JSON-RPC id with JSON tool text as `text_json`, state.json) compared as data. Multi-line text is
split into line arrays so `diff -u` points at the changed line; config text stays verbatim. Normalized: sandbox
path `<RUN>`, install root `<NUDGE_ROOT>` (= dirname(dirname(realpath NUDGE_BIN))), ports `<PORT>`/`<OFFLINE_PORT>`,
state epoch-ms values as `<now>` / `<now-Nms>` (values the harness seeded) / `<epoch-ms>`, the relative age at the
end of doctor's `Recent:` lines (`just now`, `5m ago`, `371d ago`) as `<ago>`, and fake herdr calls sorted (nudge
issues some concurrently). The version in `user-agent` is kept. Each scenario lists `check_failures` (unexpected
exit code, expectation not seen in a PTY, missing MCP response, a desk secret printed, harness error); the command
prints them and exits 1 if there are any (0 otherwise), so CI can run it directly:
`NUDGE_BIN=$PWD/target/release/nudge .cursor/skills/verify-nudge/bin/verify-nudge parity $RUNNER_TEMP/parity.json`.
Two runs of the same binary must produce identical files — if not, the harness has a nondeterminism bug; fix it
before trusting a cross-build diff.

## Evidence

- Session drives: `.verify/<id>/NNN-<label>.json` (one per `run`/`pty`/`mcp`), `.verify/<id>/session.json`, and
  after cleanup `.verify/<id>/webhook-requests.jsonl` (every request the fake desk received). Parity: the
  transcript path you pass (convention `.verify/parity/<build>.json`). `.verify/` is git-ignored.
- Proof standards:
  - Drive the user path: `nudge …` commands, a PTY for interactive setup, herdr's env vars for plugin
    entrypoints. Do not call internals or edit config/state to fake an outcome; seeding preconditions is fine.
  - Capture the action and the resulting state: the record's `webhook` (what the desk got), `files` (config
    mode 600 / dir 700, state.json, command link), `herdr_calls` (toasts, overlays opened) — not only stdout.
  - Side effects are part of the proof: "nothing saved" means `files.config` is `null`; "no request sent" means
    `webhook` is `[]`; a hidden header means the token text is absent from `output` (`secret_leaks` is `[]`).
  - Mocks stop only at real process boundaries nudge already has: HTTP to the desk, the `herdr` CLI, the
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
- `fake-webhook --dir DIR --id ID` — the desk; started by `launch`/`parity`, not by hand.
- `fake-herdr` — run by the sh wrapper `<sandbox>/fakebin/herdr`; fixture `<sandbox>/herdr.json`, log `<sandbox>/herdr-calls.jsonl`.
- `fake-clipboard` — run by the wrappers `<sandbox>/fakebin/wl-paste` and `xclip`; prints `<sandbox>/clipboard.txt` (exit 1 if empty),
  logs to `<sandbox>/clipboard-calls.jsonl` with `DISPLAY`/`WAYLAND_DISPLAY`/`XDG_RUNTIME_DIR`.
