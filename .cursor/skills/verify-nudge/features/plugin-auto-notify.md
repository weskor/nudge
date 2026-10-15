# Plugin auto-notify

herdr runs `nudge plugin event` on every `pane.agent_status_changed` with `HERDR_PLUGIN_EVENT_JSON`. nudge tracks
turns in state.json and notifies the default desk with the pane's last 40 meaningful lines (ANSI, rules and blank
runs removed, secrets redacted) when an agent is `blocked`, or `done` after a turn of 2+ minutes, unless auto-notify
is off, the pane is focused, the agent already ran `nudge notify` this turn, or the pane is in its 5-minute cooldown.
Every decision for done/blocked goes to stderr as `[nudge] <pane> <status>: …`.

## Sub-features

- `event-turn` `working` sets `working_since` (kept on repeats); `idle`/`unknown`/`done` clear it.
- `event-done` notifies after a turn ≥120 s; skips `turn shorter than 120s` and `turn start unknown`.
- `event-blocked` notifies immediately (status `blocked`).
- `event-skips` `auto-notify is off` (default), `you are looking at this pane`, `agent already notified the desk
  this turn`, `cooldown`.
- `event-payload` body: `from_agent` = herdr agent name (else pane id), `cwd` = agent cwd (else HOME), git
  repo/branch of that cwd, message lead + `Last terminal output (secrets redacted):` + tail, or `No terminal output captured.`
- `event-redaction` Bearer/Basic/Token values, `*KEY*/*TOKEN*/*SECRET*/*PASSWORD*` assignments, `sk-`, `ghp_`,
  `AKIA…`, JWTs, private-key blocks and long mixed tokens become `[REDACTED]`; hex SHAs stay.
- `event-failure` webhook/config errors exit 1, log `[nudge] failed: …` and toast `Desk notify failed: …`.
- `event-invalid` missing/garbled event JSON logs `ignoring event without pane_id/agent_status`, exit 0.

## How to get to it (user POV)

- Automatic: an agent in herdr changes status; herdr invokes the `[[events]]` hook from herdr-plugin.toml.
- Turned on by setup's `Turn on auto-notify?` or the `weskor.nudge.enable` action.

## Driving it with verify-nudge

Preconditions:

- `$VN seed-config`, `$VN herdr agent-pane`, `$VN seed-state '{"auto": true}'`.
- For repo fields: `$VN git-repo repo feature/x git@github.com:acme/api.git` (the preset agent's cwd is `<RUN>/repo`).
- `E() { printf '{"event":"pane.agent_status_changed","data":{"pane_id":"p_1","agent_status":"%s"}}' "$1"; }`

- **Turn starts.** Run `$VN run --cwd home --env "HERDR_PLUGIN_EVENT_JSON=$(E working)" -- plugin event`. Exit 0, empty stderr, no request; `files.state.json.panes.p_1.working_since` is `<now>`.
- **Long turn (no sleeping).** Move the turn start back: `$VN seed-state '{"auto": true, "panes": {"p_1": {"working_since": {"ago": 180000}}}}'`.
- **Done after a long turn.** Run `$VN run --cwd home --env "HERDR_PLUGIN_EVENT_JSON=$(E done)" -- plugin event`. stderr `[nudge] p_1 done: notified desk (HTTP 200)`; `webhook[0].body` has `from_agent: "claude-1"`, `status: "done"`, message starting `Agent finished a turn (auto-detected by herdr; …)`, the last 40 lines (from `step 22`), every planted secret as `[REDACTED]`, the commit SHA intact; state has `last_auto_at: <now>` and no `working_since`; herdr calls are `agent get p_1` (twice) and `pane read p_1 …`.
- **Cooldown.** Repeat with `$(E blocked)`. stderr `skipped (cooldown)`, no request.
- **Blocked.** Fresh state `{"auto": true}`, event `blocked`. Notified with the `Agent is waiting for approval or input` lead.
- **Focused / off / self-notified.** Use a fixture with `"focused": true` (skip `you are looking at this pane`), state `{}` (skip `auto-notify is off`), or run `$VN run --env HERDR_PANE_ID=p_1 -- notify done` during a turn (skip `agent already notified the desk this turn`).
- **Failure.** `$VN seed-config --default broken`; event `blocked` exits 1, stderr `[nudge] failed: desk "broken" webhook returned HTTP 500: desk exploded`, herdr call `notification show nudge --body Desk notify failed: …`.
- **Proof.** The event records' `webhook` body is what the desk saw; check redaction there, not in the fixture.

## Gotchas

- `working_since` must be in the past for `done` to qualify; seed it with `{"ago": ms}` instead of waiting 2 minutes.
- `self_notified_at` counts for a turn when it is at or after `working_since` (or within the last 2 minutes when
  no turn is known), so a blocked event right after `nudge notify` in that pane is also skipped.
- `idle` and `working` events never log; only done/blocked decisions do.
- Without a fixture rule for `agent get`, the target falls back to the pane id and the cwd to the sandbox HOME
  (no git info).
