# Plugin auto-notify

herdr runs `nudge plugin event` on every `pane.agent_status_changed` with `HERDR_PLUGIN_EVENT_JSON`. nudge tracks
turns in state.json and notifies the default desk with the pane's last `auto.tail_lines` (40) meaningful lines (ANSI,
rules and blank runs removed, secrets redacted) when an agent is `blocked`, or `done` after a turn of
`auto.min_turn_seconds` (120) or more, unless auto-notify is off, the pane is focused, the agent already ran
`nudge notify` this turn, the pane is in its `auto.cooldown_seconds` (300) cooldown, or the agent is outside
`[scope]`. Away mode (state.json `"away": true`) lifts the first two of those skips: you said you left your desk, so
the pane on screen and auto-notify being off no longer stop a notification. After a successful notification it toasts
`Told the desk: …` unless `auto.toast = false`. Every decision for done/blocked goes to stderr as `[nudge] <pane> <status>: …`.

## Sub-features

- `event-turn` `working` sets `working_since` (kept on repeats); `idle`/`unknown`/`done` clear it.
- `event-done` notifies after a turn ≥ `min_turn_seconds`; skips `turn shorter than <N>s` and `turn start unknown`.
- `event-blocked` notifies immediately (status `blocked`).
- `event-auto-config` `[auto]` keys `min_turn_seconds` (integer ≥ 0, default 120), `cooldown_seconds` (integer ≥ 0,
  default 300), `tail_lines` (1..=200, default 40, also used by `ask-desk`), `toast` (bool, default true); unknown
  keys and bad values are config errors (`auto.<key>: unknown setting`), so the event exits 1 with `[nudge] failed: …`.
- `event-toast` after a successful auto notification with `toast` on: `herdr notification show nudge --body "Told the
  desk: <agent> finished a turn"` (done) or `"Told the desk: <agent> is blocked"` (blocked); `<agent>` is the herdr
  agent name, else the pane id. No toast on skips or with `toast = false`.
- `event-scope` out of `[scope]`: `skipped (outside nudge's scope)`, no request, no toast (see scope.md). Checked only
  after the other rules decide to notify.
- `event-skips` `auto-notify is off` (default), `you are looking at this pane`, `agent already notified the desk
  this turn`, `cooldown`.
- `event-away` with state.json `away` true, `you are looking at this pane` and `auto-notify is off` no longer skip
  (blocked and long done notify as usual, toast included); min turn, cooldown, `agent already notified the desk this
  turn` and `[scope]` still skip. Without `away` (absent key) the rules are unchanged.
- `event-payload` body: `id` (UUID v4), `sent_at`, `from_agent` = herdr agent name (else pane id), `cwd` = agent cwd
  (else HOME), git repo/branch of that cwd, message lead + `Last terminal output (secrets redacted):` + tail, or
  `No terminal output captured.`; signed (`x-nudge-*`) when the default desk has a `signing_secret`.
- `event-redaction` Bearer/Basic/Token values, `*KEY*/*TOKEN*/*SECRET*/*PASSWORD*` assignments, `sk-`, `ghp_`,
  `AKIA…`, JWTs, private-key blocks and long mixed tokens become `[REDACTED]`; hex SHAs stay.
- `event-failure` webhook/config errors exit 1, log `[nudge] failed: …` and toast `Desk notify failed: …`.
- `event-invalid` missing/garbled event JSON logs `ignoring event without pane_id/agent_status`, exit 0.

## How to get to it (user POV)

- Automatic: an agent in herdr changes status; herdr invokes the `[[events]]` hook from herdr-plugin.toml.
- Turned on by setup's `Turn on auto-notify` checkbox, `nudge settings`, or the `weskor.nudge.enable` action.

## Driving it with verify-nudge

Preconditions:

- `$VN seed-config`, `$VN herdr agent-pane`, `$VN seed-state '{"auto": true}'`.
- For repo fields: `$VN git-repo repo feature/x git@github.com:acme/api.git` (the preset agent's cwd is `<RUN>/repo`).
- `E() { printf '{"event":"pane.agent_status_changed","data":{"pane_id":"p_1","agent_status":"%s"}}' "$1"; }`

- **Turn starts.** Run `$VN run --cwd home --env "HERDR_PLUGIN_EVENT_JSON=$(E working)" -- plugin event`. Exit 0, empty stderr, no request; `files.state.json.panes.p_1.working_since` is `<now>`.
- **Long turn (no sleeping).** Move the turn start back: `$VN seed-state '{"auto": true, "panes": {"p_1": {"working_since": {"ago": 180000}}}}'`.
- **Done after a long turn.** Run `$VN run --cwd home --env "HERDR_PLUGIN_EVENT_JSON=$(E done)" -- plugin event`. stderr `[nudge] p_1 done: notified desk (HTTP 200)`; `webhook[0].body` has `id: "<uuid>"`, `sent_at: "<sent_at>"`, `from_agent: "claude-1"`, `status: "done"`, message starting `Agent finished a turn (auto-detected by herdr; …)`, the last 40 lines (from `step 22`), every planted secret as `[REDACTED]`, the commit SHA intact; state has `last_auto_at: <now>` and no `working_since`; herdr calls are `agent get p_1` (twice), `pane read p_1 …` and the toast `notification show nudge --body Told the desk: claude-1 finished a turn`.
- **Cooldown.** Repeat with `$(E blocked)`. stderr `skipped (cooldown)`, no request.
- **Blocked.** Fresh state `{"auto": true}`, event `blocked`. Notified with the `Agent is waiting for approval or input` lead.
- **Focused / off / self-notified.** Use a fixture with `"focused": true` (skip `you are looking at this pane`), state `{}` (skip `auto-notify is off`), or run `$VN run --env HERDR_PANE_ID=p_1 -- notify done` during a turn (skip `agent already notified the desk this turn`).
- **Failure.** `$VN seed-config --default broken`; event `blocked` exits 1, stderr `[nudge] failed: desk "broken" webhook returned HTTP 500: desk exploded`, herdr call `notification show nudge --body Desk notify failed: …` (no `Told the desk` toast).
- **Thresholds.** `$VN seed-config --append '[auto]\nmin_turn_seconds = 30'` and a 60 s turn (`{"ago": 60000}`): `done` notifies (default would skip). `min_turn_seconds = 600` with a 300 s turn skips. `cooldown_seconds = 30` with `last_auto_at` 60 s ago notifies; `cooldown_seconds = 900` with 600 s ago skips `cooldown`.
- **Tail.** `--append '[auto]\ntail_lines = 5'`, event `blocked`: the message tail has 5 lines. `tail_lines = 0` or `201`: `$VN run --expect 1 -- list` lists `auto.tail_lines`.
- **Toast off.** `--append '[auto]\ntoast = false'`: the notification is sent but `herdr_calls` has no `notification show`.
- **Away.** With the focused fixture and `{"auto": true}`, event `blocked` skips `you are looking at this pane`; run `$VN run --cwd home -- plugin action away-on`, repeat the event: `notified desk (HTTP 200)`, one request, toast `Told the desk: claude-1 is blocked`. Same for `done` after re-seeding a 180 s turn. `$VN seed-state '{"away": true}'` (auto off) with `agent-pane`: `blocked` notifies; after `away-off` it skips `auto-notify is off`. Away with a 30 s turn, `last_auto_at` 60 s ago, `self_notified_at` in the turn, or an out-of-`[scope]` pane still skips (`turn shorter than 120s`, `cooldown`, `agent already notified the desk this turn`, `outside nudge's scope`). Parity: `away-event-*`.
- **Unknown key.** `--append '[auto]\nbogus = 1'`: `list`, `notify` and the event exit 1 with `auto.bogus: unknown setting`; nothing sent.
- **Proof.** The event records' `webhook` body is what the desk saw; check redaction there, not in the fixture.

## Gotchas

- `working_since` must be in the past for `done` to qualify; seed it with `{"ago": ms}` instead of waiting 2 minutes.
- `self_notified_at` counts for a turn when it is at or after `working_since` (or within the last 2 minutes when
  no turn is known), so a blocked event right after `nudge notify` in that pane is also skipped.
- `idle` and `working` events never log; only done/blocked decisions do.
- Without a fixture rule for `agent get`, the target falls back to the pane id and the cwd to the sandbox HOME
  (no git info), and the toast names the pane id (`Told the desk: p_7 is blocked`).
- `toast` affects only the success toast; failure toasts (`Desk notify failed: …`) are not part of the contract.
