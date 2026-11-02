# The Herdr desktop panel: install and remove

`nudge panel install` writes the panel's five files (`mod.json`, `main.js`, `rows.js`, `popup.js`,
`popup-view.js`, built into the binary) into `$HERDR_APP_HOME` (else `~/.config/herdr-app`) `/mods/nudge/`, leaving
files that already match alone (first line `Installed the Nudge panel in <dir>` or `Already installed: the Nudge
panel in <dir>`). Then it asks `herdr-app status` which layout the open window shows:

- `herdr-app` not on PATH: `` `herdr-app` isn't on PATH, so nudge can't reach Herdr's window to dock the panel. ``, exit 0;
- no answer (window closed): `Herdr's window isn't open, so nudge can't dock the panel or check that it is: …`, exit 0;
- the layout already has a block with `content.mod == "nudge"` (docked or floating): `It's docked in your layout "<name>".`;
- a layout of your own (has `cloned_from`): the block `{"content":{"mod":"nudge"},"id":"nudge-panel","size":{"px":320},
  "type":"block"}` is added as the root row's last child and the file rewritten whole. It prints the file, the block
  and `` `nudge panel remove` takes it out again. ``;
- otherwise (Herdr's own layout): `herdr-app do layout.clone {"as":"<name>-nudge","name":"<name>"}`, recorded with the
  copy as Herdr wrote it in nudge's state dir (`panel.json`), or `herdr-app do layout.switch --name <name>-nudge` when
  that copy exists; then the copy gets the block. Herdr's own file is never written. A refused command is exit 1
  (`nudge: Herdr couldn't copy the layout "<name>"`); a root that isn't a row is exit 1 too.

`nudge panel remove` undoes it, safe to run again and with Herdr closed:

- window open on `<x>-nudge` (and `layouts/<x>.json` exists): `layout.switch --name <x>`, `Herdr showed "<x>-nudge"; it
  shows "<x>" again.`;
- the `nudge-panel` block is taken out of every layout of your own that has it (`Took the panel out of your layout
  "<name>" (<file>).`, not printed for a copy that's then deleted); Herdr's layouts and panel blocks with another id
  are left alone;
- each copy recorded in `panel.json` is deleted while it's still what Herdr cloned (`Deleted the layout …`); else
  kept: changed (`… it changed since …, so it's yours now.`, and forgotten), or Herdr opens with it and the window
  is closed (`… run nudge panel remove again while it's open …`, still recorded); a `<x>-nudge` it switched away from
  that nudge didn't make: `Kept the layout "<x>-nudge": nudge panel install didn't make it.`;
- the panel's files (and `jsconfig.json` / `*.d.ts` Herdr writes) are deleted, then the folder if empty:
  `Removed the Nudge panel's files (<dir>).`, or it lists the other files it kept, or `… aren't in <dir>.`

## Sub-features

- `panel-install-closed` window closed; a second run says `Already installed`; remove twice.
- `panel-install-remove-own-layout` a cloned layout gets the block (printed) in place; again: `It's docked`; remove
  takes it out; remove again changes nothing.
- `panel-install-shows-the-copy` on Herdr's `main` with `main-nudge` present: `layout.switch`, no clone.
- `panel-remove-switches-back` shown `main-nudge` that nudge didn't make: switch to `main`, block out, copy kept.
- `panel-install-copy-refused` `layout.clone` fails: exit 1, `main.json` untouched, no copy.
- `panel-no-herdr-app` PATH without `herdr-app`: install and remove say so; remove still deletes the files.
- `panel-cli` `panel`, `panel frob`, `panel install extra`, `panel remove extra`: exit 2.
- Unit-tested only: the clone path and the record (a fake window that writes the copy like Herdr); install then
  remove round trips (Herdr's layout: copy deleted, `main` untouched; your own: the same layout field for field, byte
  for byte when nudge wrote it); a changed copy; Herdr closed with the copy it opens with; a custom own layout; a
  root that isn't a row; `HERDR_APP_HOME`.

## Driving it with verify-nudge

- A fake `herdr-app` fixture at `<RUN>/herdr-app.json` (parity: `fake("herdr-app", ...)`); calls appear as
  `herdr-app_calls`. Layout files go in `home/.config/herdr-app/layouts/` (parity: the `write(...)` op). With no
  fixture the fake exits 1 (a closed window); `env={"PATH": "/usr/bin:/bin"}` hides it entirely.
- `files.herdr_app`: `mod_files` (the panel's folder) and `layouts` (each file → whether it shows the panel).

## Gotchas

- The fake can't write a clone; the clone path is covered by unit tests and by runs against a real Herdr window in
  an isolated sandbox (its own `HERDR_APP_HOME`, `HC_SESSION`, home, `XDG_*` and display).
