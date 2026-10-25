# Settings

`nudge settings [--pane]` is a full-screen terminal editor (alternate screen, a centered rounded box titled
` nudge settings `, `● unsaved` at the top right while there are unsaved edits) for
auto-notify, away mode, the `[auto]` settings, `[scope]` (who can notify) and the default webhook. It reads the config,
state.json `auto` and `away`, and the open workspaces/agents from `herdr workspace list` / `herdr agent list`; nothing changes until `s`,
which rewrites the config (mode 600) and state.json and prints `Saved nudge settings to <path>.` after leaving the
screen. Quitting without saving prints `No changes saved.`. herdr opens it as an overlay through the
`settings` action / entrypoint (`--pane` adds a `Settings saved` toast).

## Sub-features

- `settings-refusal` stdin or stdout not a terminal: exit 2, stderr `nudge: settings needs a terminal; edit <config path>
  instead (see config.example.toml)` plus the usage hint line; checked before the config, so no config gives the same.
  `settings extra` is a usage error (exit 2).
- `settings-no-config` in a terminal without a config: exit 1, `nudge: no config at … Run "nudge setup" to add a webhook.`,
  before drawing anything.
- `settings-rows` in order (cursor starts on the first): under `Mode`, `[x]/[ ] Auto-notify`, `[x]/[ ] I'm away`;
  under `Automatic notifications`, `[x]/[ ] Toast when one is sent`, then the value rows `Shortest turn sent`,
  `Cooldown per pane`, `Terminal lines sent`, each label padded to 22 columns, then `◂ 2 minutes ▸` under the cursor or
  `  2 minutes` elsewhere; under `Who can notify`, `( )/(•) Every agent`, `( ) Only these…` / `(•) Only these:`. Only
  while `Only these` is picked: `Workspaces`, one `[ ]/[x] <name>` per workspace (label, else id; detail `N agent(s)`
  open in it), `Agents`, one per agent (name, else pane id; detail `<workspace> · <cwd basename>`), open ones first,
  then names only the config has with the detail `(not open)`; names are padded so details line up. Under `Default
  webhook`, one `( )/(•) <webhook>  <host>[  signed]` per webhook, names padded the same way (radio = default webhook; never the URL
  path or secrets). Headers, sub-headers, blanks and `none open in herdr` (empty list) are not cursor stops, and
  neither are the hidden lists. Footer, below a rule: the status (✓ green, ✗ red, other yellow), or else a dim
  description of the row under the cursor, in 2 rows (up to 4 for a long status), then
  `↑↓ move  space pick  ←→ change  t test  s save  q quit`. Highlight = the cursor row in reverse video.
- `settings-keys` ↑/k ↓/j move (clamped at both ends); ←/h/- and →/l/+ step a value row to the next preset (shortest
  turn 0,30,60,120,180,300,600,900,1800 s; cooldown 0,60,120,300,600,900,1800,3600 s; lines 10,20,40,80,120,200;
  custom values snap to the next preset in that direction; the ends stay put); Enter/Space flips a checkbox, cycles a
  value row up (wrapping), picks a radio (`Only these` shows the lists, `Every agent` hides them; ticks are kept),
  ticks a workspace/agent or makes a webhook the default.
- `settings-save` `s` re-reads the config, writes `[auto]` with only non-default keys, `[scope]` with the ticked names
  (a workspace matched by id in the config is saved under its label; removed entirely for `Every agent`),
  `default_webhook`, and state.json `auto` and `away` (`"away": true` when ticked, key absent when not; other state is
  kept). `Only these` with nothing ticked refuses: status `Tick at least one workspace
  or agent, or choose Every agent.`, nothing written. `--pane` also runs `herdr notification show nudge --body
  "Settings saved"`.
- `settings-quit` `q`/Esc/Ctrl-C quit at once without edits; with unsaved edits the first shows `Unsaved changes: press
  q again to discard them, or s to save.` and only a second consecutive quit exits (any other key in between disarms).
- `settings-test` `t` on a webhook row: status `Testing "<webhook>"…`, then `✓ "<webhook>" received a test ping (HTTP 200)` or
  `✗ <error>[; copy the Authorization header|Webhook URL again with nudge setup]`; signed webhooks sign the ping.
  Elsewhere: `Move to a webhook to test it.`. Testing is not an edit. The ping runs in the background: keys keep
  working while `Testing …` is up, and the result replaces it when it arrives (`settings-slow-test-ping`, fake route
  `/slow`, 3 s).
- `settings-action` `nudge plugin action settings` runs `herdr plugin pane open --plugin weskor.nudge --entrypoint
  settings --focus` (no `--target-pane`); if herdr fails, exit 1 `could not open the settings pane`.

## How to get to it (user POV)

- `nudge settings` in a terminal.
- In herdr: the `weskor.nudge.settings` action (key binding or `herdr plugin action invoke weskor.nudge.settings`) opens
  the `settings` pane entrypoint, which runs `nudge settings --pane`.

## Driving it with verify-nudge

Preconditions:

- `$VN seed-config`; `$VN seed-state '{"auto": true}'` to start with auto-notify on (`"away": true` for away mode on).
- A herdr fixture with `workspace list` / `agent list` rules (parity's `settings_herdr()`: workspaces `api-work`, `web-work`,
  unlabelled `wF`; agents `claude-1`, `reviewer`, unnamed `wF:p4`), e.g. a JSON file for `$VN herdr FILE.json`:
  `{"rules":[{"argv":["workspace","list"],"stdout":{"result":{"workspaces":[{"workspace_id":"wD","label":"api-work"}]}}},
  {"argv":["agent","list"],"stdout":{"result":{"agents":[{"name":"claude-1","pane_id":"wD:p1"}]}}},
  {"prefix":["notification","show"]}],"default_exit":1}`. Without a fixture both lists show `none open in herdr`.

- **Refusal.** `$VN run --expect 2 -- settings` (stdin is a pipe): the `needs a terminal` stderr; `files.config` unchanged.
- **No config.** Before seeding: `$VN pty --expect 'no config at' --expect-exit 1 -- settings`.
- **Render.** `$VN pty --screen 'Auto-notify' --send q --expect-exit 0 -- settings`; the record's `screens[0].lines` is
  the screen (rows marked `> ` are highlighted = cursor), `output` is `No changes saved.`.
- **Edit and save.** e.g. `$VN pty --screen '^> .*Auto-notify' --send ' ' --send 'jjj\e[C' --screen '^> .*3 minutes' --send s
  --expect-exit 0 -- settings`: `files.config.text` gains `[auto]` / `min_turn_seconds = 180`, `files.state.json.auto`
  flips, `output` is `Saved nudge settings to <RUN>/home/.config/nudge/config.toml.`. Re-open to see the saved values.
- **Away.** `$VN pty --screen '^> .*Auto-notify' --send j --screen '^> .*\[ \] I'm away' --send ' '
  --screen '^> .*\[x\] I'm away' --send s --expect-exit 0 -- settings`: `files.state.json` gains `"away": true`
  (`auto` as before). Re-open: the row shows `[x]`; untick and save: the `away` key is gone. A state set by
  `plugin action away-on` opens ticked too. Parity: `settings-away`, `settings-away-from-action`.
- **Test ping.** Move to the `signed` webhook and send `t`: the record's `webhook` has the `/signed` request with
  `signature: "valid:signed-webhook"`, and the screen the `✓ "signed" received a test ping (HTTP 200)` status.
- **Pane.** `-- settings --pane` then save: `herdr_calls` has `notification show nudge --body Settings saved`.
- **Action.** `$VN herdr running; $VN run --cwd home -- plugin action settings`: `herdr_calls` is the `plugin pane open`
  call above.
- **Proof.** `files.config` (text, mode 600), `files.state`, `webhook`, `herdr_calls`, the `screens` snapshots; parity
  scenarios `settings-*` and `action-settings-*` assert them (`config_keys`, `scope_is`, `state_auto`, `state_keys`,
  `config_unchanged`, `toasts`, `herdr_has`, `lacks`).

## Gotchas

- ratatui redraws only changed cells, so raw PTY bytes are cursor-addressed fragments. Use `--screen RE` (matched against
  the rendered 120x40 screen, multiline mode, highlighted rows prefixed `> `, others `  `), not `--expect`, for
  anything on the settings screen. A `--screen` match waits until the program has been quiet for 0.25 s, so it shows
  the settled screen; write each one to assert something the previous key changed, or it can match the stale screen.
- With any `--screen` step the record keeps `screens` and the text left on the normal screen as `output`; the raw bytes
  are only scanned for secrets. Transient states (the `Testing "<webhook>"…` status) are only in the raw bytes: parity
  matches them with an escape-tolerant `expect`.
- Opening settings only reads state.json; quitting without saving leaves it untouched (and doesn't create it).
- The box is 70–96 columns wide (its widest line, within the screen), so the status wraps (up to 4 rows) at about
  64 columns: match a long status in pieces, e.g. `✗ webhook "badauth" returned HTTP 401` and, on the next row,
  `copy the Authorization header again with nudge setup`. Value rows show `◂ ▸` only under the cursor, so assert
  unfocused values as `<label padded to 22>  <value>` (e.g. `Shortest turn sent      2 minutes`).
- Send keys only after a `--screen` step: the first key before the UI is in raw mode would be lost or echoed. Esc must be
  its own `--send` (an Esc followed by more bytes in one write is parsed as an escape sequence).
