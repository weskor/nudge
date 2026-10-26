# Changelog

All notable changes to nudge are listed here. Versions follow [Semantic Versioning](https://semver.org).

## Unreleased

- **Internal:** the code is split into area folders (`cli/`, `setup/`, `settings/`, `terminal/`, `doctor/`, `plugin/`, `herdr/`, `auto/`, `webhook/`, `config/`) of single-responsibility files, with unit tests beside each module. CI now fails on a source file over 300 lines or a function over 50. No change in behavior: the verification harness transcript is byte-for-byte the same.

## 0.11.0

- **BREAKING: the plugin id is now `nudge` (was `weskor.nudge`).** Actions are `nudge.send-pane`, `nudge.enable`, `nudge.settings` and so on, and `herdr plugin uninstall nudge` / `update nudge` / `log list --plugin nudge` use the short id. Install is unchanged: `herdr plugin install weskor/nudge` (herdr installs from GitHub paths). To upgrade an existing install, run `herdr plugin uninstall weskor.nudge`, then `herdr plugin install weskor/nudge`, and rename any `weskor.nudge.*` keybindings in `~/.config/herdr/config.toml` to `nudge.*`. Your nudge config and state are kept: they live in `~/.config/nudge` and `~/.local/state/nudge`, not under the plugin id.
- **Webhook instructions template:** it now covers panes you send from herdr (the automation reports back to you instead of replying to the agent), and it always escalates auto-detected `blocked` events, because herdr won't prompt an agent that's waiting on its own terminal prompt. Update the instructions in your automation if you copied the old template.
- **README:** the suggested send-pane key is `prefix+shift+s`; `prefix+n` is herdr's "next tab".

## 0.10.4

- **0.10.3, released.** 0.10.3 was tagged but never released: its clipboard tests assumed the Linux clipboard tools and failed on macOS. 0.10.4 has every 0.10.3 change below, with tests that check the Linux and macOS clipboard paths on any platform, and CI now tests on macOS on every push.

## 0.10.3

- **Private keys no longer slip past redaction.** Pane output is now redacted as a whole before it's cut to the last lines, so a key whose `-----BEGIN` line scrolled above the cut is still recognized. The base64 lines before a lone `-----END … PRIVATE KEY-----` are redacted too.
- **A redirecting webhook is reported, not followed.** Following a redirect turned the POST into a body-less GET, so nudge could report "sent" while the webhook never got the event. It now fails with `returned HTTP 302 (a redirect: … set the webhook URL to where it points: …)`, and setup sends you back to fix the URL.
- **Webhook replies can't leak your header or garble the terminal.** Text from a failing webhook's reply has control characters removed, and your Authorization value and signing secret are replaced with `[REDACTED]`, in case the endpoint echoes the request back.
- **A failed send no longer uses up the cooldown.** If the webhook is down, the config is broken or the pane is outside the scope, the next `blocked` or `done` from that pane still notifies.
- **herdr startup only repoints its own link.** `~/.local/bin/nudge` is repointed only when it points into an older install of the plugin next to the current one; links to `~/.cargo/bin/nudge`, `/usr/local/bin/nudge` or a checkout are left alone.
- **Setup can't overwrite another webhook by a trailing space.** `research ` is now refused as taken when `research` exists (and `--webhook " research"` means `research`).
- **A pasted `curl -H "Authorization: …"` saves only the header.** Pastes and clipboard reads store the header found in the text, not the text around it.
- **A slow clipboard read never overwrites what you typed** (Enter on an empty field, then typing before the read finished).
- **The settings test ping no longer freezes the screen.** It runs in the background; keys and Ctrl-C keep working.
- **Safer config writes.** The config is flushed to disk before it replaces the old one, a symlinked config stays a symlink, and a failed write leaves no temp file behind.
- **Clipboard on macOS.** Setup reads the clipboard with `pbpaste`, and `nudge doctor` no longer tells Mac users to install wl-clipboard.

## 0.10.2

- **The setup wizard never freezes on the clipboard.** Clipboard reads (when the wizard opens, and Enter on an empty field) run in the background, so a slow clipboard tool no longer holds up the first screen or the keyboard. A read that finishes after you've typed fills only fields that are still empty.
- **A failed save keeps your answers.** Setup now saves while the wizard is still on screen. If the config can't be written, step 4 says why (`✗ Not saved: …`) and keeps every answer; Enter tries again once the problem is fixed, and Shift+Tab goes back to change something. It used to exit with an error and lose everything you'd entered.
- **Internal:** the wizard's per-step state (focus, the options cursor, the test and save state) is now separate from your answers, so moving between steps can't drop an answer.

## 0.10.1

- **The Authorization header can no longer end up on screen while pasting.** In the setup wizard, a header (or anything that isn't the start of a URL) pasted into the Webhook URL field now goes into the Authorization field, where it's never shown. A header pasted into the name or description is refused, and a URL and header pasted together on step 2 fill step 1 instead of the name.
- **`--authorization-file -` refuses a terminal.** It used to read the terminal before the wizard opened, echoing the typed header. It now exits with a usage error that says to pipe the header in or pass a file path.
- **Secrets stay out of debug output.** The header and signing secret are redacted (only their length is shown) wherever nudge's types are debug-printed, including failing test assertions.

## 0.10.0

- **Setup is a step-by-step wizard.** `nudge setup` in a terminal (and the `weskor.nudge.setup` overlay) now walks through four steps with a step list on the left: connect webhook (URL and Authorization header), name (and an optional description), options, and test and save. Shift+Tab or Esc goes back a step and keeps your answers. The clipboard prefills the URL or header only when it holds a value that checks out, and is never shown otherwise. Pasting the URL and the header together fills both fields. A name that's already a webhook is refused, with a pointer to `nudge setup --webhook NAME` to change it. Step 3 lists what saving will do (auto-notify, the `nudge` command in `~/.local/bin`, the default webhook); Enter accepts, `e` changes them. Step 4 shows a spinner while it test-pings; if the ping fails you can try again, go back to step 1 to fix the header or URL, or save anyway. After saving, setup says what happens next.
- **Setup without a screen.** `--authorization-file PATH` (`-` for stdin) and the `NUDGE_WEBHOOK_URL` and `NUDGE_WEBHOOK_AUTHORIZATION` environment variables supply the values. `--yes` (`-y`) saves without the wizard and applies step 3's defaults (auto-notify on if the herdr plugin is installed, `~/.local/bin/nudge` linked); `--no-auto-notify` and `--no-install-command` turn those off. Plain scripted setup (no terminal, no `--yes`) is unchanged.

## 0.9.0

- **BREAKING: "desk" is now "webhook" everywhere.** `--desk` is now `--webhook` on `setup`, `notify` and `test`. In the config, `[[desk]]` and `default_desk` are now `[[webhook]]` and `default_webhook`. An old config is migrated automatically the first time nudge reads it: it is rewritten in the new format (mode 600, every setting and secret kept, comments dropped) and nudge prints a one-line note. The MCP tools are now `notify_webhook` and `list_webhooks`, with the argument `webhook`. The herdr action `ask-desk` is now `send-pane`, so update your key bindings. Setup's default name is now `main`.
- **A clearer settings editor.** `nudge settings` is now a centered, colored box. Auto-notify and away mode have their own section at the top. A line under the list explains the row under the cursor. The workspace and agent lists appear only when you pick "Only these", and each agent shows its workspace and directory, so panes named like `w1:p5` are easier to tell apart. The default webhook section is now called "Default webhook", and the title shows "● unsaved" while you have changes that aren't saved.
- **Setup is a form.** `nudge setup` in a terminal (and the `weskor.nudge.setup` overlay) is now a full-screen form in the same style as settings, instead of a series of prompts. It explains what a webhook is and walks through three numbered steps: where to send (URL and Authorization header), this webhook (its name, which you can now change there, and a description), and what to do after saving. Press Enter on an empty field to paste the clipboard, or type or paste into it (bracketed paste works); the terminal's cursor shows where typing goes. The Authorization header shows only dots while you type and a masked preview afterwards. Auto-notify, the command install and making the webhook the default are checkboxes. Renaming says whether saving adds a webhook, updates it or replaces another one. A failed test ping sends the cursor back to the field it blames, or offers "Save anyway" when it blames neither. Scripted setup (flags or stdin) is unchanged.

## 0.8.0

- **Away mode.** The herdr actions `away` (toggle), `away-on` and `away-off`, and a row in `nudge settings`, tell nudge you left your desk. While away, the pane focused in herdr no longer counts as watched and auto-notify sends even if it's off; turn length, cooldown and scope still apply. `nudge doctor` shows when it's on.

## 0.7.0

- **Settings editor.** `nudge settings`, or the herdr action `weskor.nudge.settings` as an overlay, edits auto-notify, its thresholds and toast, who can notify (with the workspaces and agents open in herdr to tick), and the default desk, and can test-ping a desk. Nothing is written until you save.

## 0.6.0

- **Choose which agents can notify.** A `[scope]` table limits notifications to listed herdr workspaces or agents. The herdr actions `only-workspace`, `only-agent` and `all-agents` set it from the focused pane. Agents outside it are refused by `nudge notify` and the MCP tool and skipped by auto-notify; `ask-desk` and `nudge test` ignore it.
- **Tunable auto-notify.** An `[auto]` table sets the minimum turn length, the per-pane cooldown, how many terminal lines are sent, and the toast.
- **A toast when auto-notify sends** ("Told the desk: api-refactor is blocked"), so you know it happened.
- **`id` and `sent_at` in every event,** so the desk can drop duplicates.
- **Optional signed requests.** A desk with `signing_secret` gets `x-nudge-timestamp` and an HMAC-SHA256 `x-nudge-signature`. Set it with `nudge setup --signing-secret`; re-running setup keeps it.

## 0.5.0

- **Rewritten in Rust.** The binary is about 3 MB instead of 30–40 MB, and static on Linux. Commands, config, state and the webhook JSON are unchanged.
- `nudge doctor` shows recent plugin runs by age ("5m ago") instead of clock time.
- Building from source needs Rust instead of Bun.

## 0.4.0

First public release.

- **herdr plugin.** Notifies your desk when an agent is `blocked`, or `done` after a turn of 2+ minutes while you're not watching it. Rate-limited per pane, skips turns where the agent already ran `nudge notify`, and redacts credentials from the terminal output it sends. Auto-notify is off until you turn it on.
- **Setup.** Clipboard-first, with a masked Authorization header. It tests the webhook before saving, and asks again for whichever value was rejected (401/403 → header, 404 → URL).
- **`nudge doctor`** checks the config, webhook, plugin, auto-notify, command and clipboard tools, and says how to fix problems.
- **herdr actions:** setup and status overlays, send the focused pane to the desk, and toggle auto-notify.
- **`nudge notify`, `nudge test`, `nudge list` and `nudge mcp`**, for agent-written notifications and MCP clients.
- **Prebuilt binaries** for Linux and macOS (x64 and arm64), checksum-verified at install; built from source with Bun on other platforms.
