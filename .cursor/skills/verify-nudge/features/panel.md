# The Herdr desktop panel: install and remove

`nudge panel install` writes the panel's five files (`mod.json`, `main.js`, `rows.js`, `popup.js`,
`popup-view.js`, built into the binary) into `$HERDR_APP_HOME` (else `~/.config/herdr-app`) `/mods/nudge/`, leaving
files that already match alone (first line `Installed the Nudge panel in <dir>` or `Already installed: the Nudge
panel in <dir>`). Then it asks `herdr-app status` which layout the open window shows:

- `herdr-app` not on PATH: `` `herdr-app` isn't on PATH, so nudge can't reach Herdr's window to dock the panel. ``, exit 0;
- no answer (window closed): `Herdr's window isn't open, so nudge can't dock the panel or check that it is: …`, exit 0;
- the layout already has a block with `content.mod == "nudge"` (docked or floating): `It's docked in your layout "<name>".`;
  when it's install's block (`nudge-panel`) and the window's click state (`layouts/.state/<name>.json`) has a width
  for it other than the file's, `herdr-app do layout.resize --id nudge-panel --px <file's px>` and `Herdr had saved
  the panel <w> px wide; it's back to its layout's width.`; hidden there: `layout.show --id nudge-panel` and `Herdr
  had the panel hidden; it shows again.` (the same for a `<name>-nudge` copy shown again with `layout.switch`);
- a layout of your own (has `cloned_from`): the block `{"id":"nudge-panel","type":"block","content":{"mod":"nudge"},
  "size":{"px":320},"min":280,"max":400}` is spliced into the file's text after the root row's last child, with the
  same gap (newline and indent) before it, or right after `[` when the list is empty; no other byte changes. It
  prints the file, the block and `` `nudge panel remove` takes it out again. ``;
- a layout install can't dock in (a root that isn't a row with `children`, a file that isn't JSON, comments): exit 1,
  nothing written; the error is followed by `To dock the panel by hand, add this block as the last child of the top
  row in <file> (…): `, the block on its own line, and `Or open it over the window: herdr-app do mod.open --name nudge`;
- otherwise (Herdr's own layout): `herdr-app do layout.clone {"as":"<name>-nudge","name":"<name>"}`, recorded with the
  copy as Herdr wrote it in nudge's state dir (`panel.json`), or `herdr-app do layout.switch --name <name>-nudge` when
  that copy exists; then the copy gets the block. Herdr's own file is never written. A refused command is exit 1
  (`nudge: Herdr couldn't copy the layout "<name>"`).

`nudge panel remove` undoes it, safe to run again and with Herdr closed:

- window open on `<x>-nudge` (and `layouts/<x>.json` exists): `layout.switch --name <x>`, `Herdr showed "<x>-nudge"; it
  shows "<x>" again.`;
- the `nudge-panel` block is cut out of the text of every layout of your own that has it, with the gap before it, so
  install then remove gives the file back byte for byte (`Took the panel out of your layout "<name>" (<file>).`, not
  printed for a copy that's then deleted); Herdr's layouts and panel blocks with another id are left alone;
- each copy recorded in `panel.json` is deleted while it's still what Herdr cloned (`Deleted the layout …`); else
  kept: changed (`… it changed since …, so it's yours now.`, and forgotten), or Herdr opens with it and the window
  is closed (`… run nudge panel remove again while it's open …`, still recorded); a `<x>-nudge` it switched away from
  that nudge didn't make: `Kept the layout "<x>-nudge": nudge panel install didn't make it.`;
- the window's click state for the panel goes too: for the layout the open window shows (and that keeps the block),
  first `layout.resize` / `layout.show` as install does, since the window would save it again from memory; then the
  `nudge-panel` node is cut out of `.state/<name>.json` for every layout the block came out of and every copy deleted,
  leaving every other byte; Herdr's own layouts' state isn't touched;
- the panel's files (and `jsconfig.json` / `*.d.ts` Herdr writes) are deleted, then the folder if empty:
  `Removed the Nudge panel's files (<dir>).`, or it lists the other files it kept, or `… aren't in <dir>.`

Herdr's click state: a dragged divider saves `nodes.<id>.size.px` (unclamped, from the width as laid out); window
resizes, layout switches and `layout.clone` (which copies the source's state) add no width; a `layout.resize` to the
file's width drops the entry. The window rereads a layout's state file when it shows it, but rewrites the shown
layout's from memory on its next click change.

## Sub-features

- `panel-install-closed` window closed; a second run says `Already installed`; remove twice.
- `panel-install-remove-own-layout` a layout of your own in Herdr's formatting: remove first changes nothing; install
  adds the block (printed) in place; again: `It's docked`; remove takes it out, and `mine.json`'s hash is back to the
  first step's; remove again changes nothing.
- `panel-install-shows-the-copy` on Herdr's `main` with `main-nudge` present: `layout.switch`, no clone.
- `panel-remove-switches-back` shown `main-nudge` that nudge didn't make: switch to `main`, block out, copy kept.
- `panel-saved-width` a docked layout of your own whose `.state/mine.json` has the panel at 559 px: install asks the
  window for `layout.resize … --px 320` and says so; remove takes the block out and cuts only the panel's node from
  `.state/mine.json` (its hash changes, the `panel` node stays).
- `panel-install-refused` a layout of your own whose root is a column: exit 1 with the by-hand block and the
  `mod.open` fallback; `tall.json` unchanged.
- `panel-install-copy-refused` `layout.clone` fails: exit 1, `main.json` untouched, no copy.
- `panel-no-herdr-app` PATH without `herdr-app`: install and remove say so; remove still deletes the files.
- `panel-cli` `panel`, `panel frob`, `panel install extra`, `panel remove extra`: exit 2.
- Unit-tested only: the clone path and the record (a fake window that writes the copy like Herdr); install then
  remove round trips (Herdr's layout: copy deleted, `main` untouched; your own, from a real `layout.clone` file and
  a custom one: byte for byte); one-line, tab-indented and empty-children layouts byte for byte, including the
  whitespace inside `[ ]` / `[\n]`; blocks cut anywhere (leading runs, floating lists); refusals (column, row without
  children, comments, broken JSON) write nothing; a changed copy; Herdr closed with the copy it opens with;
  `HERDR_APP_HOME`; the click state: a saved width and hidden flag put back through the window, none when it matches
  the file or is another node's; the panel's node cut first, last, in the middle or alone, never another key's.

## Driving it with verify-nudge

- A fake `herdr-app` fixture at `<RUN>/herdr-app.json` (parity: `fake("herdr-app", ...)`); calls appear as
  `herdr-app_calls`. Layout files go in `home/.config/herdr-app/layouts/` (parity: the `write(...)` op). With no
  fixture the fake exits 1 (a closed window); `env={"PATH": "/usr/bin:/bin"}` hides it entirely.
- `files.herdr_app`: `mod_files` (the panel's folder) and `layouts` (each layout file, and each click state file as
  `.state/<name>` → `panel`: whether it shows the panel, `sha256`: the first 12 hex digits of its bytes).

## Gotchas

- The fake can't write a clone; the clone path is covered by unit tests and by runs against a real Herdr window in
  an isolated sandbox (its own `HERDR_APP_HOME`, `HC_SESSION`, home, `XDG_*` and display).
