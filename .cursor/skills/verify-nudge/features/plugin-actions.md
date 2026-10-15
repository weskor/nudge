# Plugin actions and startup

herdr plugin actions run `nudge plugin action <id>`: `enable`/`disable`/`toggle` flip auto-notify and toast the new
state, `ask-desk` sends the focused pane's output to the default desk right away (skipping the auto rules), and
`setup`/`status` open nudge's overlay panes. At herdr server start `nudge plugin startup` repoints a stale
`~/.local/bin/nudge` link at the current install.

## Sub-features

- `action-auto` `enable` / `disable` / `toggle` write `auto` to state.json and run `herdr notification show nudge --body "Auto-notify on|off"`.
- `action-ask-desk` uses `HERDR_PLUGIN_CONTEXT_JSON.focused_pane_id` (else `HERDR_PANE_ID`); status `blocked`
  if `focused_pane_status` is `blocked`, else `question`; message lead `The user asked you to look at this agent`;
  toast `Sent this pane to the desk (HTTP 200)`; `No pane to send` without a pane.
- `action-ask-desk-failure` webhook/config errors exit 1 with `[nudge] failed: …` and toast `Desk notify failed: …`.
- `action-panes` `setup` / `status` run `herdr plugin pane open --plugin weskor.nudge --entrypoint <id> --focus`;
  if herdr fails: exit 1, `could not open the <id> pane`, no toast.
- `action-unknown` unknown ids and bad modes exit 1 with `[nudge] failed: …`.
- `startup-link` repoints a symlink that ends in `/bin/nudge` but targets another install; leaves the current
  link, foreign links, regular files and missing links alone (silent).

## How to get to it (user POV)

- `herdr plugin action invoke weskor.nudge.<id>` or a key binding; herdr sets `HERDR_PLUGIN_CONTEXT_JSON`.
- Startup runs automatically when the herdr server starts (`[[startup]]` in herdr-plugin.toml).

## Driving it with verify-nudge

Preconditions:

- `$VN herdr agent-pane` (toasts and `pane open` succeed; `p_1` has an agent and output). `$VN seed-config` for ask-desk.

- **Toggle.** Run `$VN run --cwd home -- plugin action toggle` twice, then `enable`, `disable`. `files.state.json.auto` goes true, false, true, false; each record's `herdr_calls` is one `notification show nudge --body Auto-notify on|off`.
- **Ask desk.** Run `$VN run --cwd home --env 'HERDR_PLUGIN_CONTEXT_JSON={"focused_pane_id":"p_1","focused_pane_status":"blocked"}' -- plugin action ask-desk`. Exit 0; `webhook[0].body.status` `blocked`, `from_agent` `claude-1`, redacted tail; toast `Sent this pane to the desk (HTTP 200)`. With status `working` the body status is `question`.
- **No pane.** Run `$VN run --cwd home --env 'HERDR_PLUGIN_CONTEXT_JSON={}' -- plugin action ask-desk`. No request; toast `No pane to send`.
- **Overlays.** Run `$VN run --cwd home -- plugin action setup` and `… status`. Exit 0; `herdr_calls` is the `plugin pane open … --entrypoint setup|status --focus` call. With `$VN herdr none`: exit 1, stderr `[nudge] failed: could not open the setup pane`.
- **Startup repoint.** Run `$VN link /opt/old-nudge/bin/nudge`, then `$VN run --cwd home --expect 0 -- plugin startup`. stderr `[nudge] repointed ~/.local/bin/nudge at this install`; `files.command_link.target` is `<NUDGE_ROOT>/bin/nudge`. A second run is silent and changes nothing; so do `$VN link /usr/bin/true`, `$VN link --file` and `$VN link --remove`.
- **Proof.** The records' `herdr_calls` are the toasts/overlays the user would see; `files` shows the state flips.

## Gotchas

- Toasts and overlays are herdr calls, not output: prove them via `herdr_calls`, never by looking at a real herdr.
- `ask-desk` ignores auto-notify, focus and cooldown, and does not update `last_auto_at`.
- `<NUDGE_ROOT>` is the install root nudge reports (`bin/nudge` sets `NUDGE_ROOT` to the checkout); `startup`
  compares the link against `<root>/bin/nudge`.
- `plugin action` with no id fails with `[nudge] failed: usage: nudge plugin event | action <id> | startup`, exit 1.
