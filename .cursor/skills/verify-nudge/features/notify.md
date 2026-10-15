# Notify, test and list

A user or agent runs `nudge notify` to POST `{from_agent, cwd, repo, branch, status, message, reply_hint}` to a
desk webhook with the desk's Authorization header, `nudge test` to send a harmless `status: "test"` ping, and
`nudge list` to see configured desks with the default marked `*`.

## Sub-features

- `notify-args` sends the joined positional words as the message, status `done` by default.
- `notify-stdin` reads the message from stdin with `-` (trimmed).
- `notify-desk-status` picks a desk with `--desk` and a status with `--status done|blocked|question`.
- `notify-agent` sets `from_agent` from `--agent`, else `$NUDGE_AGENT`, else the herdr agent in `$HERDR_PANE_ID`
  (name, else pane id), else the single herdr agent whose cwd/foreground_cwd is the current dir; otherwise
  `null` plus a stderr note. `reply_hint` shell-quotes odd agent names.
- `notify-pane-state` records `self_notified_at` in state.json when run inside a herdr pane.
- `notify-git` fills `repo` (`owner/name` from origin, else the checkout dir name) and `branch`.
- `notify-errors` exit 1 for empty message, unknown desk, no/invalid config, several desks without default,
  webhook 401/404/500 (body text included) and unreachable webhook; exit 2 for usage errors and bad `--status`.
- `test-ping` sends the fixed test message to the default or `--desk` desk.
- `list` prints `* name  description` rows (`--json` prints `[{name, default, description}]`).

## How to get to it (user POV)

- `nudge notify [--desk NAME] [--status S] [--agent NAME] <message...|->` in any shell or agent pane.
- `nudge test [--desk NAME] [--agent NAME]`.
- `nudge list [--json]`.

## Driving it with verify-nudge

Preconditions:

- Session launched, `$VN doctor` clean, `$VN seed-config` done.
- For pane/agent detection: `$VN herdr agent-pane` (agent `claude-1` in `p_1`), or a custom fixture file passed to `$VN herdr FILE.json`.
- For git fields: `$VN git-repo repo feature/x git@github.com:acme/api.git`.

- **Plain notify.** Run `$VN run --expect 0 -- notify Finished the auth refactor`. stdout `Notified desk "desk" (status done, HTTP 200) as agent <none>.`; stderr carries the `--agent NAME or set NUDGE_AGENT` note; `webhook[0]` is `POST /hook` with `authorization: Bearer good-token-…`, `content-type: application/json`, `user-agent: nudge/<version>` and `from_agent: null`, `repo: null`.
- **Stdin.** Run `$VN run --stdin 'Line one\nLine two\n' --expect 0 -- notify -`. Body `message` is `Line one\nLine two`.
- **Desk and status.** Run `$VN run --expect 0 -- notify --desk research --status blocked Need the DB password`. stdout names desk `research`, status `blocked`.
- **Agent from flag/env.** Run `$VN run --env NUDGE_AGENT=env-agent -- notify hi` then `$VN run -- notify --agent "my agent's pane" hi`. Bodies carry the agent; the second `reply_hint` is `reply with: herdr agent prompt 'my agent'\''s pane' "..."`.
- **Agent from pane.** With `$VN herdr agent-pane`, run `$VN run --env HERDR_PANE_ID=p_1 -- notify from the pane`. `from_agent: "claude-1"`, `herdr_calls` is `[["agent","get","p_1"]]`, `files.state.json.panes.p_1.self_notified_at` is `<now>`.
- **Plain shell pane.** Run `printf '{"rules":[{"argv":["agent","get","p_shell"],"exit":1}]}' > /tmp/vn-shell.json && $VN herdr /tmp/vn-shell.json`, then `$VN run --env HERDR_PANE_ID=p_shell --expect 0 -- notify hi`. `from_agent: null` and the stderr note; `herdr_calls` is `[["agent","get","p_shell"]]`.
- **Agent by cwd.** Run `printf '{"rules":[{"argv":["agent","list"],"stdout":{"result":{"agents":[{"name":"worker","pane_id":"p_3","cwd":"{RUN}/work"}]}}}]}' > /tmp/vn-cwd.json && $VN herdr /tmp/vn-cwd.json`, then `$VN run -- notify hi`. `from_agent: "worker"`; with two agents on `{RUN}/work` it is `null`.
- **Git.** After `$VN git-repo repo feature/x git@github.com:acme/api.git`, run `$VN run --cwd repo -- notify --agent a in git`. Body `repo: "acme/api"`, `branch: "feature/x"`, `cwd: <RUN>/repo`.
- **Errors.** Run each with `--expect 1`: `notify "   "` (`message is empty`, no request), `notify --desk nope hi`, `notify --desk badauth hi` (`HTTP 401: {"message":"Invalid API key"}`), `--desk nohook` (404), `--desk broken` (500 `desk exploded`), `--desk offline` (`could not reach desk "offline" webhook: …`). Usage errors with `--expect 2`: `notify`, `notify --status urgent hi`.
- **Test ping.** Run `$VN run --expect 0 -- test` and `$VN run -- test --desk research --agent pinger`. Body `status: "test"`, `message: "nudge test ping. No action needed."`.
- **List.** Run `$VN run -- list` and `$VN run -- list --json`. Text has `* desk      Main desk`; JSON (`stdout_json`) marks only `desk` as default.
- **Proof.** Cite the `.verify/<id>/NNN-run-notify-*.json` records; each holds the request the desk received.

## Gotchas

- The default cwd `<RUN>/work` is deliberately not a git repo; the sandbox lives under `/tmp` so nudge never sees
  this repo's git metadata. Create a repo inside the sandbox for git fields.
- With no fixture, herdr calls exit 1 (like herdr missing) and agent detection quietly yields `null`; that is a
  valid journey, not a harness failure.
- The unreachable-webhook message embeds the HTTP client's own error text (currently `io: Connection refused (os
  error 111)`); it changes if the HTTP stack changes, and the parity diff shows it.
- `--status` is validated before the config is read, so a bad status exits 2 even without a config.
