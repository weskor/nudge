# Settings

`nudge settings [--pane]` is a full-screen terminal editor (alternate screen, a box titled ` nudge settings `) for
auto-notify, the `[auto]` settings, `[scope]` (who can notify) and the default desk. It reads the config, state.json
`auto`, and the open workspaces/agents from `herdr workspace list` / `herdr agent list`; nothing changes until `s`,
which rewrites the config (mode 600) and state.json and prints `Saved nudge settings to <path>.` after leaving the
screen. Quitting without saving prints `No changes saved.`. herdr opens it as an overlay through the
`settings` action / entrypoint (`--pane` adds a `Settings saved` toast).

## Sub-features

- `settings-refusal` stdin or stdout not a terminal: exit 2, stderr `nudge: settings needs a terminal; edit <config path>
  instead (see config.example.toml)` plus the usage hint line; checked before the config, so no config gives the same.
  `settings extra` is a usage error (exit 2).
- `settings-no-config` in a terminal without a config: exit 1, `nudge: no config at … Run "nudge setup" to add a desk.`,
  before drawing anything.
- `settings-rows` in order (cursor starts on the first): `[x]/[ ] Auto-notify`, `[x]/[ ] Toast when one is sent`,
  `Shortest turn sent    ◂ 2 minutes ▸`, `Cooldown per pane     ◂ 5 minutes ▸`, `Terminal lines sent   ◂ 40 ▸`,
  `( )/(•) Every agent`, `( )/(•) Only these:`, one `[ ]/[x] <name>` per workspace (label, else id) then per agent
  (name, else pane id) — open ones first, then names only the config has with `  (not open)` — then one
  `( )/(•) <desk>  <host>[  signed]` per desk (radio = default desk; never the URL path or secrets). Headers
  (`Automatic notifications`, `Who can notify`, `Workspaces`, `Agents`, `Desk the plugin notifies`), blanks and
  `none open in herdr` (empty list) are not cursor stops. Footer: status line, then
  `↑↓ move  space pick  ←→ change  t test  s save  q quit`.
- `settings-keys` ↑/k ↓/j move (clamped at both ends); ←/h/- and →/l/+ step a value row to the next preset (shortest
  turn 0,30,60,120,180,300,600,900,1800 s; cooldown 0,60,120,300,600,900,1800,3600 s; lines 10,20,40,80,120,200;
  custom values snap to the next preset in that direction; the ends stay put); Enter/Space flips a checkbox, cycles a
  value row up (wrapping), picks a radio, ticks a workspace/agent (which also selects `Only these`) or makes a desk
  the default.
- `settings-save` `s` re-reads the config, writes `[auto]` with only non-default keys, `[scope]` with the ticked names
  (a workspace matched by id in the config is saved under its label; removed entirely for `Every agent`),
  `default_desk`, and state.json `auto`. `Only these` with nothing ticked refuses: status `Tick at least one workspace
  or agent, or choose Every agent.`, nothing written. `--pane` also runs `herdr notification show nudge --body
  "Settings saved"`.
- `settings-quit` `q`/Esc/Ctrl-C quit at once without edits; with unsaved edits the first shows `Unsaved changes: press
  q again to discard them, or s to save.` and only a second consecutive quit exits (any other key in between disarms).
- `settings-test` `t` on a desk row: status `Testing "<desk>"…`, then `✓ "<desk>" received a test ping (HTTP 200)` or
  `✗ <error>[; copy the Authorization header|Webhook URL again with nudge setup]`; signed desks sign the ping.
  Elsewhere: `Move to a desk to test it.`. Testing is not an edit.
- `settings-action` `nudge plugin action settings` runs `herdr plugin pane open --plugin weskor.nudge --entrypoint
  settings --focus` (no `--target-pane`); if herdr fails, exit 1 `could not open the settings pane`.

## How to get to it (user POV)

- `nudge settings` in a terminal.
- In herdr: the `weskor.nudge.settings` action (key binding or `herdr plugin action invoke weskor.nudge.settings`) opens
  the `settings` pane entrypoint, which runs `nudge settings --pane`.

## Driving it with verify-nudge

Preconditions:

- `$VN seed-config`; `$VN seed-state '{"auto": true}'` to start with auto-notify on.
- A herdr fixture with `workspace list` / `agent list` rules (parity's `settings_herdr()`: workspaces `api-work`, `web-work`,
  unlabelled `wF`; agents `claude-1`, `reviewer`, unnamed `wF:p4`), e.g. a JSON file for `$VN herdr FILE.json`:
  `{"rules":[{"argv":["workspace","list"],"stdout":{"result":{"workspaces":[{"workspace_id":"wD","label":"api-work"}]}}},
  {"argv":["agent","list"],"stdout":{"result":{"agents":[{"name":"claude-1","pane_id":"wD:p1"}]}}},
  {"prefix":["notification","show"]}],"default_exit":1}`. Without a fixture both lists show `none open in herdr`.

- **Refusal.** `$VN run --expect 2 -- settings` (stdin is a pipe): the `needs a terminal` stderr; `files.config` unchanged.
- **No config.** Before seeding: `$VN pty --expect 'no config at' --expect-exit 1 -- settings`.
- **Render.** `$VN pty --screen 'Auto-notify' --send q --expect-exit 0 -- settings`; the record's `screens[0].lines` is
  the screen (rows marked `> ` are highlighted = cursor), `output` is `No changes saved.`.
- **Edit and save.** e.g. `$VN pty --screen '^> .*Auto-notify' --send ' ' --send 'jj\e[C' --screen '^> .*3 minutes' --send s
  --expect-exit 0 -- settings`: `files.config.text` gains `[auto]` / `min_turn_seconds = 180`, `files.state.json.auto`
  flips, `output` is `Saved nudge settings to <RUN>/home/.config/nudge/config.toml.`. Re-open to see the saved values.
- **Test ping.** Move to the `signed` desk and send `t`: the record's `webhook` has the `/signed` request with
  `signature: "valid:signed-desk"`, and the screen the `✓ "signed" received a test ping (HTTP 200)` status.
- **Pane.** `-- settings --pane` then save: `herdr_calls` has `notification show nudge --body Settings saved`.
- **Action.** `$VN herdr running; $VN run --cwd home -- plugin action settings`: `herdr_calls` is the `plugin pane open`
  call above.
- **Proof.** `files.config` (text, mode 600), `files.state`, `webhook`, `herdr_calls`, the `screens` snapshots; parity
  scenarios `settings-*` and `action-settings-*` assert them (`config_keys`, `scope_is`, `state_auto`,
  `config_unchanged`, `toasts`, `herdr_has`, `lacks`).

## Gotchas

- ratatui redraws only changed cells, so raw PTY bytes are cursor-addressed fragments. Use `--screen RE` (matched against
  the rendered 120x40 screen, multiline mode, highlighted rows prefixed `> `, others `  `), not `--expect`, for
  anything on the settings screen. A `--screen` match waits until the program has been quiet for 0.25 s, so it shows
  the settled screen; write each one to assert something the previous key changed, or it can match the stale screen.
- With any `--screen` step the record keeps `screens` and the text left on the normal screen as `output`; the raw bytes
  are only scanned for secrets. Transient states (the `Testing "<desk>"…` status) are only in the raw bytes: parity
  matches them with an escape-tolerant `expect`.
- Opening settings only reads state.json; quitting without saving leaves it untouched (and doesn't create it).
- The status line wraps (up to 4 rows) above the help line, so long test-ping errors keep their fix hint, e.g.
  `…; copy the Authorization header again with` / `nudge setup` on the next row at 120 columns.
- Send keys only after a `--screen` step: the first key before the UI is in raw mode would be lost or echoed. Esc must be
  its own `--send` (an Esc followed by more bytes in one write is parsed as an escape sequence).
