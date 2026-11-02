# The Herdr desktop panel install

`nudge panel install` writes the panel's five files (`mod.json`, `main.js`, `rows.js`, `popup.js`,
`popup-view.js`, built into the binary) into `$HERDR_APP_HOME` (else `~/.config/herdr-app`) `/mods/nudge/`, leaving
files that already match alone (first line `Installed the Nudge panel in <dir>` or `Already installed: the Nudge
panel in <dir>`). Then it asks `herdr-app status` which layout the open window shows:

- no answer (window closed): `Herdr's window isn't open, so nudge can't dock the panel or check that it is: …`, exit 0;
- the layout already has a block with `content.mod == "nudge"` (docked or floating): `It's docked in your layout "<name>".`;
- a layout of your own (has `cloned_from`): the panel is added as the root row's last child (`id: nudge-panel`,
  `size: {px: 320}`) and the file rewritten whole: `Docked it on the right of your layout "<name>".`;
- otherwise (Herdr's own layout): `herdr-app do layout.clone {"as":"<name>-nudge","name":"<name>"}`, or
  `herdr-app do layout.switch --name <name>-nudge` when that copy exists, then the copy gets the panel:
  `Docked it on the right of "<name>-nudge", your copy of Herdr's "<name>" layout, which Herdr shows now.` Herdr's own
  file is never written. A refused command is exit 1 (`nudge: Herdr couldn't copy the layout "<name>"`); a root that
  isn't a row is exit 1 too.

## Sub-features

- `panel-install-closed` window closed; a second run says `Already installed`.
- `panel-install-own-layout` a cloned layout gets the panel in place; again: `It's docked`.
- `panel-install-shows-the-copy` on Herdr's `main` with `main-nudge` present: `layout.switch`, no clone.
- `panel-install-copy-refused` `layout.clone` fails: exit 1, `main.json` untouched, no copy.
- `panel-cli` `panel`, `panel frob`, `panel install extra`: exit 2.
- Unit-tested only: the clone path writing the copy (a fake window that writes it like Herdr), the root-not-a-row
  error, `HERDR_APP_HOME`.

## Driving it with verify-nudge

- A fake `herdr-app` fixture at `<RUN>/herdr-app.json` (parity: `fake("herdr-app", ...)`); calls appear as
  `herdr-app_calls`. Layout files go in `home/.config/herdr-app/layouts/` (parity: the `write(...)` op).
- `files.herdr_app`: `mod_files` (the panel's folder) and `layouts` (each file → whether it shows the panel).

## Gotchas

- The fake can't write a clone; the clone path is covered by unit tests and by a run against a real Herdr window
  in an isolated sandbox (its own `HERDR_APP_HOME`, `HC_SESSION`, home and display).
