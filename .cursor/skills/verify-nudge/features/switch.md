# Global switch

`nudge off` (or the herdr actions `nudge.off` / `nudge.switch`) turns nudge off on this machine: no webhook request
leaves it until `nudge on`. Every send goes through one check, so each entry point fails loudly instead of pretending
to send: `nudge notify` and `nudge test` exit 1 with `nudge is off: nothing was sent. Turn it on with nudge on`, MCP
`notify_webhook` returns that text as an `isError: true` result, the `send-pane` action fails with the toast
`Nudge is off: nothing was sent`, and setup's test ping fails (`--no-test` still saves). Auto-notify events are skipped
and logged, even in away mode, without starting a cooldown; turns keep being tracked. The switch is `on` in
`state.json`, absent while on (the default) and `"on": false` while off; it is separate from auto-notify and away mode.
The same release prefixes pane keys in `state.json` with their backend (`tui:<pane id>`); older unprefixed keys are
read as `tui:` keys and written back prefixed.

## Sub-features

- `switch-cli` `nudge on` prints `nudge is on: agents and the herdr plugin can notify webhooks.`; `nudge off` prints
  `nudge is off: nothing is sent from this machine. Turn it back on with nudge on`; both exit 0, are idempotent and
  take no arguments (exit 2).
- `switch-actions` `on`, `off` and `switch` set or flip `on` and toast `Nudge on` or `Nudge off: nothing is sent from
  this machine`; `auto` and `away` are untouched.
- `switch-off-notify-test` `nudge notify` / `nudge test` while off: exit 1, stderr `nudge: nudge is off: …`, no request,
  no `self_notified_at` record. Config and scope errors still come first.
- `switch-off-mcp` `notify_webhook` while off: `isError: true` with the same text; `list_webhooks` still works.
- `switch-off-send-pane` `plugin action send-pane` while off: exit 1, log `[nudge] failed: nudge is off: …`, toast
  `Nudge is off: nothing was sent`.
- `switch-off-event` a `blocked`/`done` that settles while off logs `[nudge] <pane> <status>: skipped (nudge is off)`,
  exit 0, no request, no toast, no `last_auto_at`. The check sits after turn tracking and before the auto-notify,
  focus and away rules.
- `switch-off-doctor` doctor's first line is `✓ Nudge on`, or `! Nudge off: nothing is sent` with fix `nudge on`
  (a warning, not a failure); `doctor --ping` while off shows `! Webhook not tested: nudge is off`.
- `switch-off-setup-yes` `setup --yes` while off fails its test ping (`test ping failed, nothing saved: nudge is off:
  …`); with `--no-test` it saves and leaves nudge off.
- `state-keys` pane entries are keyed `tui:<herdr pane id>` (`core:<address>` is reserved for herdr-core terminals);
  any key without a backend prefix is rewritten as `tui:<key>` on the next state write.

## How to get to it (user POV)

- `nudge off` / `nudge on` in any shell.
- herdr actions `nudge.off`, `nudge.on`, `nudge.switch` (key bindings or `herdr plugin action invoke nudge.switch`).
- `nudge doctor` shows whether nudge is on.

## Driving it with verify-nudge

Preconditions:

- `$VN seed-config` (standard webhooks); `$VN herdr agent-pane` for send-pane and events.
- `$VN seed-state '{"on": false}'` (add `"auto": true` and a `panes` entry with `working_since` for events).

- **CLI.** `$VN run -- off`, then `$VN files`: `state.json` has `"on": false`. `$VN run --expect 1 -- notify hi` and
  `$VN run --expect 1 -- test`: stderr has the off text, `webhook: []`. `$VN run -- on`: `on` is gone from `state.json`.
- **MCP.** `$VN mcp --call notify_webhook '{"message":"hi"}'`: the result has `isError: true` and the off text.
- **Actions.** `$VN run --cwd home -- plugin action switch`: toast `Nudge off: …`, `"on": false`; again: toast `Nudge on`.
- **send-pane.** `$VN run --cwd home --expect 1 --env 'HERDR_PLUGIN_CONTEXT_JSON={"focused_pane_id":"p_1"}' -- plugin
  action send-pane`: toast `Nudge is off: nothing was sent`, `webhook: []`.
- **Event.** `$VN run --cwd home --env "HERDR_PLUGIN_EVENT_JSON=$(E p_1 blocked)" -- plugin event`: stderr
  `skipped (nudge is off)`, `webhook: []`, `state.json` pane has no `last_auto_at`.
- **Proof.** `webhook` stays `[]` for every off step; `files.state.json` (`on`, `tui:` keys); `herdr_calls` (toasts).

## Gotchas

- Seeded states with old keys (`"p_1"`) show up as `"tui:p_1"` after any nudge run that writes state; `edit_state`
  writes `tui:` keys.
- Off is a warning in doctor, so doctor still exits 0 when nothing else failed.
