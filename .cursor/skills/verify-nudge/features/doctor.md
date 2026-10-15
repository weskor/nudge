# Doctor

`nudge doctor` prints a checklist (`✓` ok, `!` warning, `✗` failure, with `→` fixes) for the config, the default
desk's webhook (`--ping` sends a test ping), herdr and the nudge plugin, auto-notify, recent plugin log lines, the
`nudge` command and clipboard tools, and exits 1 when any check failed.

## Sub-features

- `doctor-config` reports no config (fail), invalid TOML/schema (fail, with each problem), desk count and default
  (warn + fix when several desks have no default).
- `doctor-ping` with `--ping` reports `Webhook accepted a test ping (HTTP 200)` or a failure with a fix that blames
  the header (401/403), the URL (404/405/410), or the desk (other/unreachable).
- `doctor-herdr` reports herdr not found (warn), server unreachable (warn), plugin not installed (fail), plugin
  disabled (fail), prebuilt binary vs from-source, auto-notify on/off.
- `doctor-logs` shows the last 5 non-empty plugin log runs sorted by time, last stderr line, local time.
- `doctor-command` reports `nudge` on PATH, the `~/.local/bin/nudge` link (ok inside a plugin pane when
  `HERDR_PLUGIN_ID` is set), or a warning.
- `doctor-clipboard` lists available `wl-paste`/`xclip`.
- `doctor-pane` (`--pane`) waits for a key before exiting (herdr status overlay).

## How to get to it (user POV)

- `nudge doctor [--ping]` in a terminal.
- herdr action `weskor.nudge.status` opens `nudge doctor --pane` in an overlay.

## Driving it with verify-nudge

Preconditions:

- Session launched; `$VN seed-config` for the config checks; `$VN herdr running` for the plugin checks.

- **No config.** On a fresh session run `$VN run --expect 1 -- doctor`. stdout has `✗ No desk configured  <RUN>/home/.config/nudge/config.toml` and `→ run nudge setup`, `! herdr not found`, `✓ Clipboard: wl-paste, xclip`, `✗ 1 problem found.`.
- **Valid config, herdr running.** `$VN seed-config; $VN herdr running`, run `$VN run --expect 0 -- doctor`. stdout has `✓ Config: 6 desks, default "desk"`, `✓ herdr plugin weskor.nudge 0.4.0 enabled  <RUN>/plugin` (the fixture's plugin version), `✓ Plugin runs a local build  no dist/nudge (linked checkout)`, `! Auto-notify off` with `→ herdr plugin action invoke weskor.nudge.enable`, five `Recent:` lines (the newest five of the seven fixture entries, oldest first) each ending in a relative age such as `371d ago`, `✓ All good.`; `herdr_calls` include `--version`, `plugin list …`, `plugin log list …`.
- **Ping.** Run `$VN run --expect 0 -- doctor --ping`; `webhook[0].body.status` is `test`. Then `$VN seed-config --default badauth` and run `$VN run --expect 1 -- doctor --ping`: `✗ Webhook test failed  desk "badauth" webhook returned HTTP 401: …` with `→ copy the Authorization header again: nudge setup --desk badauth`. `--default nohook` blames the URL; `broken`/`offline` say `check that the desk is running, then nudge test`.
- **Auto-notify on.** `$VN seed-state '{"auto": true}'`, rerun doctor: `✓ Auto-notify on`.
- **Proof.** The `.verify/<id>/NNN-run-doctor*.json` records hold stdout, exit and the herdr calls.

## Gotchas

- `Recent:` lines end with the age of the log entry relative to now (`just now`, `5m ago`, `3h ago`, `371d ago`); the
  fixture timestamps are fixed (October 2025), so the age grows daily. Parity normalizes it to `<ago>`.
- The sandbox PATH contains the fake `wl-paste`/`xclip`, so the clipboard check is always ok here; the no-clipboard
  warning can't be reached without a PATH that also exposes the real `/usr/bin/wl-paste`, which the skill forbids.
- `Plugin runs the prebuilt binary` depends on `<plugin_root>/dist/nudge` existing; the `running` preset's root is
  `<RUN>/plugin`, empty unless you create `<RUN>/plugin/dist/nudge` (any file).
- `--pane` waits for a key only on a terminal; drive it with `$VN pty --expect 'Press any key to close' --send q -- doctor --pane`.
