# Changelog

All notable changes to nudge are listed here. Versions follow [Semantic Versioning](https://semver.org).

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
