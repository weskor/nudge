# Nudge

A 280 px panel on the right of the window that shows and changes [nudge](https://github.com/weskor/nudge)'s settings: whether nudge is on, where each agent's notifications go and why, and what nudge decided lately. It does it all by running the `nudge` command, on this computer or on a machine added with `hc machine add`. It never reads nudge's files and never sees a webhook's address or secret.

Requires a nudge with `nudge status --json`, installed on each machine the panel should show.

## What you see

Top to bottom, in the theme's colors (`chrome` background, `border` lines, `foreground` / `muted` / `dim` text):

- **Header** (40 px): a machine button, `This computer ▾` (or the machine's name), and on the right the big switch, `● On` (solid button colors) or `○ Off` (raised, muted).
- **Off line**, only when off: "Off: nothing is sent from this computer" (or "from box"), muted.
- **Config line**, only when nudge reports a config error, in the `blocked` color.
- **Auto-notify** and **I'm away** rows, each with a small `on` / `off` pill (accent when on).
- **Watching** row: `● 2 agents · service running` or `● 2 agents · started here` in the `working` color when nudge's watcher runs there (agents: terminals that report a status, not plain shells or the watcher itself); `○ Not watching` with a **Start** button when it doesn't (and nudge is on).
- **Agents**: one row per agent on the picked machine: a status glyph in its status color, its name (the terminal's name, else its tab's name unless Herdr numbered it, else its title, else its address), its status word, and where its notifications go with the reason, `→ client-a (pane)`, `→ client-a (workspace)` or `→ main (default)`, then `⋯`.
- **Workspaces**: one row per space on that machine, with its `→ webhook (reason)` and `⋯`.
- **Recent**: nudge's last decisions, newest first: time, agent (named as in Agents) and status, and what nudge did (`sent`, `you were watching`, or nudge's own reason).
- **Footer** (40 px): `Default webhook: main ⋯`, `· 3 webhooks`, a **Test** button and a `⋯` menu.

When nudge isn't installed on the picked machine, the panel shows the machine button and "nudge not found", and nothing else.

## How it behaves

- **Agent status** shows as a glyph in the theme's status color: working `●` (`working`), blocked `▲` (`blocked`), done `✓` (`done`), idle `○` (`idle`). The status word follows the name. The panel doesn't notify anyone itself: nudge's own watcher does, and the app already alerts you for blocked and done.
- **The switch** runs `nudge on` / `nudge off` on the picked machine. It flips at once; if nudge refuses, it flips back and a toast says why.
- **Auto-notify** and **I'm away** run `nudge auto on|off` and `nudge away on|off` there, flipping at once and back on failure.
- **The machine button** opens a popup: this computer and every connected machine, the picked one ticked, each with `● on · watching`, `○ off` or `○ nudge not found`. Below it, a note: servers you reach only over SSH (herdr TUI) aren't listed; change them there with `nudge on|off` or the `nudge.*` actions. Picking a machine makes every read and command run there.
- **`⋯` on an agent or workspace row** opens a popup listing the webhooks by name, the row's own route ticked, then "Use workspace / default" (agents) or "Use default" (workspaces), which removes the route, and on agent rows "Send this pane now". Routes are written with the agent's or space's name; when it has none, with its id, marked "(by id, ends with the session)".
- **"Send this pane now"** runs `nudge core send <address>` there (`--blocked` for a blocked agent) and toasts the result: sent, "Nudge is off: nothing was sent", or "nudge couldn't reach client-a".
- **`Default webhook: main ⋯`** opens the webhooks with the default ticked; picking one runs `nudge default <name>`.
- **Test** runs `nudge test`; "✓ sent" appears beside it, or a toast when it fails.
- **Footer `⋯`**: "Add a webhook…" and "Edit settings…" open a terminal tab on the picked machine running `nudge setup` / `nudge settings` (secrets are typed only there). "Install service…" opens a tab running `nudge core service install` (it may print a `loginctl enable-linger` hint on a server). "Remove service…" asks first, in Herdr's confirm dialog ("Remove the login service?", naming the machine); Confirm runs `nudge core service remove` there and toasts the result. "Open config folder" (this computer only) opens `~/.config/nudge`.
- **Start** runs `hc run --bg --name nudge-watch --label nudge=watch -- nudge core watch` (with `--on <machine>` for a connected one). If no watcher runs 2.5 s later, a toast says "The watcher didn't start" and which `hc read` shows why.
- **Keys** (from `keys.json`, `global` section): `ctrl-shift-y` turns this computer's nudge on or off (`nudge.toggle`), `ctrl-shift-s` sends the focused pane now (`nudge.send`), `ctrl-shift-r` opens the route popup for the focused pane (`nudge.route`). They reach the panel while it's shown. None of them is a Herdr key, and `ctrl-shift-u` is avoided: on Linux it starts Unicode input in fcitx5 and IBus.
- **Refresh**: this computer's status is read once on load and whenever `~/.local/state/nudge` changes; a connected machine's every 10 s, only while it's picked. The config isn't watched: a change made outside the panel shows at the next state change, at the latest with the 30 s focus heartbeat.
- **Focus**: so nudge skips the agent you're looking at, the panel tells the machine owning the focused terminal with `nudge core context '{"terminal":"t7","window_active":true,"spaces":[...]}'`, tells the machine that had focus before `"terminal": null`, repeats it to the focus owner every 30 s, and sends a machine its spaces when they change. Quitting the app sends `"terminal": null`, best effort.

## Built from

- `mod.json`: `{ "place": "right", "size": 280, "title": "Nudge", "overlay": { "width": 340, "height": 640 } }` (the overlay size is for `mod.open`).
- `main.js`: the panel's `View`. `popup-view.js`: the popups, opened as `openOverlay({ mod: "nudge", kind: "popup", props: { popup: "route" | "default" | "footer" | "machines", ... } })`; the popup tells the panel what changed through `setShared("nudge.machine")` and `setShared("nudge.bump")`.
- `rows.js` and `popup.js`: pure functions (rows, route reasons, the context plan, popup options with the exact `nudge` argv each runs), tested with `bun test`.
- `herdr` functions: `agents`, `terminals`, `spaces`, `machines`, `focused`, `window`, `exec`, `hc`, `notify`, `openOverlay` (its popups, and Herdr's own `confirm` dialog with `command: { name: "nudge.remove-service" }`), `closeOverlay`, `props`, `shared`, `setShared`, `run` (`tab.new`, `space.new`), `nextCommand`, `watchFolder`, `home`, `theme`.
- Programs: `nudge` (`status --json`, `on`, `off`, `auto`, `away`, `default`, `route set|rm`, `test`, `core send`, `core context`, `core service remove`) and `hc run`.
- `preview.png`: `herdr-app shot` of the panel in a sandboxed window (its own `HERDR_APP_HOME`, `HC_SESSION` and home, on a virtual display), cropped to the panel.

## Watch out

- **It runs programs**: `nudge` and `hc`, here and on connected machines (`exec(..., { on })`), and it **changes nudge's settings** there (on/off, auto, away, routes, default webhook, the login service). It reads no files and writes none itself; nudge writes its own.
- It only ever reads `nudge status --json`, which lists webhooks by name and host, never URLs or secrets. Toasts never repeat nudge's error text for a failed send.
- `exec` rejects when `nudge` isn't installed there; the panel shows "nudge not found" for that machine.
- An older nudge without `status --json` shows "This nudge has no `nudge status --json`; update nudge there" instead of the panel.
- **Start** runs `nudge` in a herdr-core terminal, so it uses the core's PATH, which can differ from the app's: an older nudge first on the core's PATH starts and stops at once (the toast says where to look).
- Herdr's own layouts take no new panels: Herdr docks a new mod only while a layout of your own is shown. `nudge panel install` handles it: on one of Herdr's layouts it makes your copy with Herdr's `layout.clone` (`main-nudge` for the default) and adds the panel to the copy, live.
- **Width in a narrow window.** Herdr saves a width for a block in `layouts/.state/<layout>.json` (a divider drag does that) and keeps it whatever the window's width; it doesn't rescale it on resize. Herdr's main layout already needs 268 px for the sidebar, plus 368 px (at least 280) for Files when it's open: at 826 px wide the terminal gets 60 columns without the panel, 16 with Files open, and with the panel at 280 px 27 columns, or none with Files open. 280 px is the narrowest every row of the panel fits at (at 240 the footer's `⋯` is cut off). The block has `"min": 280, "max": 360`: Herdr lays it out within those whatever width it saved, so at 826 px a saved width can't leave the terminal fewer than 17 columns. Running `nudge panel install` again gives an older block those limits and sets a saved width back to the layout's, and `nudge panel remove` takes the panel's entry out of the click state too. A `min` on Herdr's `work` column doesn't help: Herdr honors it by pushing the panel past the window's edge, not by shrinking it.
- `herdr-app check` renders only the first, loading state.

## Done when

- `herdr-app check` passes on the folder.
- The switch flips both ways, and a refused `nudge off` flips it back with a toast.
- The machine picker switches machines, and a machine without nudge says "nudge not found".
- A route popup sets a route and the row shows its reason; "Use workspace / default" removes it; the default webhook popup changes the default.
- With nudge not installed, the panel says so and offers nothing else.
- "Remove service…" asks before it runs; Cancel runs nothing.
- Start with no working nudge on the core's PATH toasts "The watcher didn't start".
- Quitting the app leaves `"terminal": null` in the focused machine's `context.json`.

## Install with nudge

With Herdr open, run `nudge panel install`. It writes the five files above into `~/.config/herdr-app/mods/nudge/` (only those that differ) and docks the panel on the right of the window without restarting anything. On one of Herdr's own layouts it first runs `herdr-app do layout.clone` (copy named `<layout>-nudge`, with your sizes and shown panels) and shows the copy; on a layout of your own it adds the panel's block to that layout's file and prints the block. Running it again changes nothing. With Herdr closed it installs the files and says to run it again once Herdr is open; without `herdr-app` on PATH it says that. If the layout can't take the block (its top isn't a row with children, or the file isn't plain JSON), it writes nothing, exits 1, and prints the block, the file to add it to, and `herdr-app do mod.open --name nudge` to open the panel over the window instead.

`nudge panel remove` undoes it, safe to run again and with Herdr closed: it switches Herdr from `<layout>-nudge` back to `<layout>`, takes the panel's block out of your layouts (leaving each file byte for byte as it was before install), deletes the copy install made while it's still as Herdr cloned it (otherwise it keeps it and says why), and deletes the panel's files.

## Install by hand

1. Copy `mods/nudge/` into `~/.config/herdr-app/mods/nudge/`, leaving `RECIPE.md`, `keys.json` and `preview.png` out.
2. Merge the `global` entries of `keys.json` into `~/.config/herdr-app/keys.json`, keeping any key you already use.
3. Run `herdr-app check ~/.config/herdr-app/mods/nudge`.
4. To dock it, show a layout of your own (on the default one: `herdr-app do layout.clone '{"name":"main"}'`) and add `{ "id": "nudge-panel", "type": "block", "content": { "mod": "nudge" }, "size": { "px": 280 }, "min": 280, "max": 360 }` as the last child of its root row. Or open it over the window with `herdr-app do mod.open --name nudge`.
