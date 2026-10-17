# Plugin actions and startup

herdr plugin actions run `nudge plugin action <id>`: `enable`/`disable`/`toggle` flip auto-notify and toast the new
state, `away`/`away-on`/`away-off` flip away mode (notify even for the pane on screen, see
[plugin-auto-notify.md](./plugin-auto-notify.md)), `ask-desk` sends the focused pane's output to the default desk right away (skipping the auto rules and
`[scope]`), `only-workspace`/`only-agent`/`all-agents` rewrite the config's `[scope]` from the focused pane, and
`setup`/`status`/`settings` open nudge's overlay panes (`settings` is the full-screen editor, see
[settings.md](./settings.md)). At herdr server start `nudge plugin startup` repoints a stale
`~/.local/bin/nudge` link at the current install.

## Sub-features

- `action-auto` `enable` / `disable` / `toggle` write `auto` to state.json and run `herdr notification show nudge --body "Auto-notify on|off"`.
- `action-away` `away` toggles, `away-on` sets, `away-off` clears state.json `away` (`"away": true` while on; the key
  is absent while off, and files without it read as off); other state (`auto`, panes) is kept. No config needed. Each
  toasts `herdr notification show nudge --body …`: on → `Away: the desk hears from every agent, even the pane on
  screen`, off → `Back: nudge stays quiet for the pane you're looking at` (repeating `away-on`/`away-off` toasts again).
- `action-ask-desk` uses `HERDR_PLUGIN_CONTEXT_JSON.focused_pane_id` (else `HERDR_PANE_ID`); status `blocked`
  if `focused_pane_status` is `blocked`, else `question`; message lead `The user asked you to look at this agent`;
  toast `Sent this pane to the desk (HTTP 200)`; `No pane to send` without a pane.
- `action-ask-desk-failure` webhook/config errors exit 1 with `[nudge] failed: …` and toast `Desk notify failed: …`.
- `action-scope` `only-workspace` → `[scope] workspaces = ["<label>"]`, toast `Only workspace "<label>" can notify the
  desk` (`No workspace for this pane` without one); `only-agent` → `agents = ["<name>"]`, toast `Only agent "<name>"
  can notify the desk` (`No agent in this pane`, config unchanged); `all-agents` clears `[scope]`, toast `All agents
  can notify the desk`. Focused pane from `focused_pane_id`, else `HERDR_PANE_ID`. No config: exit 1. Details in scope.md.
- `action-panes` `setup` / `status` / `settings` run `herdr plugin pane open --plugin weskor.nudge --entrypoint <id> --focus`
  (no `--target-pane`); if herdr fails: exit 1, `could not open the <id> pane`, no toast. The `settings` entrypoint
  runs `nudge settings --pane`.
- `action-unknown` unknown ids and bad modes exit 1 with `[nudge] failed: …`.
- `startup-link` repoints a symlink that ends in `/bin/nudge` but targets another install; leaves the current
  link, foreign links, regular files and missing links alone (silent).

## How to get to it (user POV)

- `herdr plugin action invoke weskor.nudge.<id>` or a key binding; herdr sets `HERDR_PLUGIN_CONTEXT_JSON`.
- Startup runs automatically when the herdr server starts (`[[startup]]` in herdr-plugin.toml).

## Driving it with verify-nudge

Preconditions:

- `$VN herdr agent-pane` (toasts and `pane open` succeed; `p_1` has an agent and output). `$VN seed-config` for ask-desk. `$VN herdr workspaces` for the scope actions.

- **Toggle.** Run `$VN run --cwd home -- plugin action toggle` twice, then `enable`, `disable`. `files.state.json.auto` goes true, false, true, false; each record's `herdr_calls` is one `notification show nudge --body Auto-notify on|off`.
- **Away.** Run `$VN run --cwd home -- plugin action away` twice, then `away-on` twice and `away-off` twice. `files.state.json` gains `"away": true`, loses the key, gains it (twice), loses it (twice); each record's `herdr_calls` is the matching `Away: …` / `Back: …` toast; `webhook` `[]`. Seeded `auto`/`panes` stay as they were. Parity: `action-away-toggle`, `action-away-keeps-state`.
- **Ask desk.** Run `$VN run --cwd home --env 'HERDR_PLUGIN_CONTEXT_JSON={"focused_pane_id":"p_1","focused_pane_status":"blocked"}' -- plugin action ask-desk`. Exit 0; `webhook[0].body.status` `blocked`, `from_agent` `claude-1`, redacted tail; toast `Sent this pane to the desk (HTTP 200)`. With status `working` the body status is `question`.
- **No pane.** Run `$VN run --cwd home --env 'HERDR_PLUGIN_CONTEXT_JSON={}' -- plugin action ask-desk`. No request; toast `No pane to send`.
- **Overlays.** Run `$VN run --cwd home -- plugin action setup`, `… status` and `… settings`. Exit 0; `herdr_calls` is the `plugin pane open … --entrypoint setup|status|settings --focus` call. With `$VN herdr none`: exit 1, stderr `[nudge] failed: could not open the setup pane`. Parity: `action-open-panes`, `action-settings-pane`, `action-settings-pane-fails`.
- **Startup repoint.** Run `$VN link /opt/old-nudge/bin/nudge`, then `$VN run --cwd home --expect 0 -- plugin startup`. stderr `[nudge] repointed ~/.local/bin/nudge at this install`; `files.command_link.target` is `<NUDGE_ROOT>/bin/nudge`. A second run is silent and changes nothing; so do `$VN link /usr/bin/true`, `$VN link --file` and `$VN link --remove`.
- **Scope actions.** With `$VN herdr workspaces; $VN seed-config`, run `$VN run --cwd home --env 'HERDR_PLUGIN_CONTEXT_JSON={"focused_pane_id":"wD:p1"}' -- plugin action only-workspace`: `files.config.text` gains `[scope]` / `workspaces = ["api-work"]` (mode still 600), toast `Only workspace "api-work" can notify the desk`, `webhook` `[]`. Same with `only-agent` (`agents = ["claude-1"]`), `only-agent` on `wD:p3` (toast `No agent in this pane`), then `all-agents` (scope cleared, toast `All agents can notify the desk`). A following `$VN run -- list` exits 0.
- **Proof.** The records' `herdr_calls` are the toasts/overlays the user would see; `files` shows the state flips.

## Gotchas

- Toasts and overlays are herdr calls, not output: prove them via `herdr_calls`, never by looking at a real herdr.
- `ask-desk` ignores auto-notify, focus, cooldown and `[scope]`, and does not update `last_auto_at`; its tail length is
  `auto.tail_lines`.
- Away mode and auto-notify are separate keys: `disable` leaves `away` alone and `away-off` leaves `auto` alone.
- The scope actions rewrite the whole config file (comments and key order are nudge's, not the seeded text).
- `<NUDGE_ROOT>` is the install root nudge reports (`bin/nudge` sets `NUDGE_ROOT` to the checkout); `startup`
  compares the link against `<root>/bin/nudge`.
- `plugin action` with no id fails with `[nudge] failed: usage: nudge plugin event | action <id> | startup`, exit 1.
