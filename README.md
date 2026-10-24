<p align="center">
  <img src="assets/hero.svg" width="920" alt="Three herdr agents. billing-fix is blocked, so nudge sends an event to your webhook; the AI routine behind it escalates it to you. api-refactor finished, so it replies straight into its pane with herdr agent prompt.">
</p>

<h1 align="center">nudge</h1>

<p align="center">
  <b>Your coding agents tell your webhook when they're done or stuck.<br>The AI routine behind it answers right in their pane.</b>
</p>

<p align="center">
  <a href="https://github.com/weskor/nudge/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/weskor/nudge/actions/workflows/ci.yml/badge.svg"></a>
  <a href="https://github.com/weskor/nudge/releases/latest"><img alt="Release" src="https://img.shields.io/github/v/release/weskor/nudge"></a>
  <img alt="herdr 0.9+" src="https://img.shields.io/badge/herdr-0.9%2B-8a2be2">
  <img alt="Linux and macOS" src="https://img.shields.io/badge/platforms-linux%20%C2%B7%20macOS-informational">
  <a href="LICENSE"><img alt="MIT license" src="https://img.shields.io/badge/license-MIT-blue"></a>
</p>

nudge is a [herdr](https://herdr.dev) plugin. When an agent finishes a long turn or gets blocked, it sends a small JSON event to your **webhook**: anything that accepts an HTTP POST, such as an AI assistant routine, a workflow automation or your own service. The automation behind it decides what to do. It can answer the agent by running `herdr agent prompt <agent> "..."`, which types the reply into the agent's pane, or escalate to you when a decision is yours.

## Why

- **Stop babysitting panes.** With several agents running, the slow part is noticing that one is done or waiting for approval. nudge notices for you.
- **Let an AI routine handle the routine questions.** The automation behind your webhook answers the questions it can (conventions, next steps, "which branch?") and escalates the rest, like credentials or production changes.
- **Answers land where the work is.** Replies arrive in the agent's pane as a new prompt. No copy-paste, no polling.
- **Careful by default.** Automatic notifications are off until you turn them on, skip the pane you're looking at, are rate-limited, and redact anything that looks like a secret.

## Requirements

- herdr 0.9 or later on Linux or macOS (x64 or arm64).
- A webhook that accepts an HTTP POST with an `Authorization` header. To reply to agents, the automation behind it needs to run `herdr agent prompt` on the machine where herdr runs.

## Install

```sh
herdr plugin install weskor/nudge
```

The install step downloads the prebuilt `nudge` binary for your platform (about 3 MB, static on Linux) and checks it against the release checksum, so there's nothing else to install. On other platforms it builds from source if [Rust](https://rustup.rs) is installed.

## Quick start

**1. Connect your webhook.** Open setup inside herdr:

```sh
herdr plugin action invoke weskor.nudge.setup
```

Setup is a wizard in four steps, with the step list on the left. **1 Connect webhook**: copy the webhook URL and the Authorization header from your webhook's settings; if the clipboard already holds a valid one it's filled in, otherwise press Enter on a field to paste, or type (pasting both at once works too). nudge never shows the header, only a masked preview like `Bearer ••••••••cdef  (34 characters)`. **2 Name**: `main` is fine if you only have one, plus an optional description; a name that's already a webhook is refused. **3 Options**: after saving, nudge turns on auto-notify (when the herdr plugin is installed), installs the `nudge` command into `~/.local/bin` and makes the webhook the default; Enter accepts these, `e` changes them. **4 Test and save**: nudge test-pings the webhook and saves only if it answers. If the ping fails, choose **Try again**, **Back to step 1** to fix the header or URL, or **Save anyway**. Shift+Tab or Esc goes back a step and keeps your answers.

<p align="center">
  <img src="assets/setup.svg" width="860" alt="Step 1 of 4 of the nudge setup wizard: the step list on the left shows Connect webhook as current, then Name, Options and Test and save; the webhook URL came from the clipboard and is checked, the Authorization header is shown masked as Bearer and its last four characters, pasted and checked.">
</p>

**2. Tell your webhook's automation what to do with events.** Paste [docs/webhook-instructions.md](docs/webhook-instructions.md) into its instructions. The template covers when to reply, when to escalate to you, and how to avoid reply loops.

**3. Check it.**

```sh
nudge doctor          # config, plugin, auto-notify, command, clipboard
nudge test            # send a test ping to your webhook
```

That's it. Agents in herdr are now covered; they don't need any instructions.

## How it works

### Automatic notifications

The plugin listens to herdr's agent status changes. It notifies your default webhook when all of these hold:

| Rule | Why |
| --- | --- |
| The agent is `blocked`, or `done` after a turn of at least 2 minutes | A quick reply isn't worth a webhook run |
| You aren't looking at the pane, or you said you're [away](#away-mode) | You already know |
| The agent didn't send its own `nudge notify` during the turn | Its own message says more than terminal output |
| No automatic notification for that pane in the last 5 minutes | Rate limit, and a brake on reply loops |
| The agent is in [scope](#choose-which-agents-can-notify) | You decide which agents may reach the webhook |
| Auto-notify is on | Off until you turn it on |

The event carries the last 40 lines of the pane, with credentials replaced by `[REDACTED]`. When it's sent, herdr shows a toast ("Told the webhook: api-refactor is blocked"). Every decision, including skipped events and why, is in `herdr plugin log list --plugin weskor.nudge`, and `nudge doctor` shows the most recent ones.

The 2 minutes, 5 minutes, 40 lines and the toast are [settings](#settings).

### Away mode

nudge counts the pane that's focused in herdr as one you're watching, even if you walked away and left the window open. Before you leave, run `weskor.nudge.away-on` (or `weskor.nudge.away` to toggle, or tick it in [settings](#settings)): until you run `weskor.nudge.away-off`, every agent's `blocked` and long `done` reach the webhook, including the pane on screen, and even with auto-notify off. The other rules (turn length, cooldown, scope) still apply. `nudge doctor` shows when away mode is on.

A keybinding makes it one keystroke:

```toml
[[keys.command]]
key = "prefix+a"
type = "plugin_action"
command = "weskor.nudge.away"
description = "nudge: away on/off"
```

### Choose which agents can notify

By default every agent can reach the webhook. To narrow that down, tick workspaces or agents in [settings](#settings), or focus a pane and run one of these herdr actions:

| Action | Who can notify afterwards |
| --- | --- |
| `weskor.nudge.only-workspace` | Agents in the focused pane's workspace |
| `weskor.nudge.only-agent` | Only the agent in the focused pane |
| `weskor.nudge.all-agents` | Every agent again |

Both write the `[scope]` table in the config. Scope applies to automatic notifications and to `nudge notify` and the MCP tool; an agent outside it gets an error and nothing is sent. Sending a pane yourself with `send-pane`, and `nudge test`, ignore it.

### Settings

`herdr plugin action invoke weskor.nudge.settings` (or `nudge settings` in a terminal) opens an editor for everything above, with the workspaces and agents that are open in herdr right now:

```text
╭ nudge settings ─────────────────────────────────── ● unsaved ╮
│                                                              │
│  Mode                                                        │
│    [x] Auto-notify                                           │
│    [ ] I'm away                                              │
│                                                              │
│  Automatic notifications                                     │
│    [x] Toast when one is sent                                │
│        Shortest turn sent    ◂ 2 minutes ▸                   │
│        Cooldown per pane       5 minutes                     │
│        Terminal lines sent     40                            │
│                                                              │
│  Who can notify                                              │
│    ( ) Every agent                                           │
│    (•) Only these:                                           │
│        Workspaces                                            │
│          [ ] Base  1 agent                                   │
│          [x] api   2 agents                                  │
│        Agents                                                │
│          [ ] builder   api · backend                         │
│          [x] reviewer  api · backend                         │
│                                                              │
│  Default webhook                                             │
│    (•) bridge  hooks.example                                 │
│    ( ) local   127.0.0.1  signed                             │
│  ──────────────────────────────────────────────────────────  │
│  Skip done turns shorter than this. Blocked agents always    │
│  count.                                                      │
│  ↑↓ move  space pick  ←→ change  t test  s save  q quit      │
╰──────────────────────────────────────────────────────────────╯
```

The line under the list explains the row under the cursor. The workspace and agent lists only appear when you pick "Only these", and each agent shows its workspace and directory. `t` sends a test ping to the webhook under the cursor. Nothing changes until you press `s`; `q` twice throws edits away. Webhooks show only their host, never the URL path or secrets.

### Notifications written by the agent

Agents (or you) can send their own message, which gives the webhook's automation more to go on:

```sh
nudge notify "Finished the auth refactor; PR #42 is ready for review."
nudge notify --status blocked "Need the staging DB password to run migrations."
nudge notify --status question "Use the v2 or v3 pricing API for the demo?"
```

To have agents do this on their own, add a few lines to their instructions:

<details>
<summary>AGENTS.md snippet</summary>

```markdown
## Webhook (nudge)

When you finish a task, or need a decision, tell the webhook:

    nudge notify --status done "<what you did; branch/PR; anything to check>"
    nudge notify --status blocked "<exactly what you need and why>"
    nudge notify --status question "<the decision you need, with the options>"

- Make the message self-contained: the webhook sees only it plus your repo, branch and cwd.
- Don't poll or wait for an answer. A reply arrives later as a new prompt starting with "Coordinator:".
- Don't notify just to acknowledge a reply.
- Never put secrets in a notification.
```

</details>

nudge is also an MCP server, for agents that prefer tools over shell commands: `claude mcp add nudge -- nudge mcp` adds `notify_webhook` and `list_webhooks`.

### The event

POSTed to your webhook with its `Authorization` header and `Content-Type: application/json`:

```json
{
  "id": "0b8f2c1e-6a4d-4f5e-9c3b-2d7a1e8f4b60",
  "sent_at": "2026-10-16T06:12:03Z",
  "from_agent": "api-refactor",
  "cwd": "/home/me/src/api",
  "repo": "acme/api",
  "branch": "auth-refactor",
  "status": "blocked",
  "message": "Need the staging DB password to run migrations.",
  "reply_hint": "reply with: herdr agent prompt api-refactor \"...\""
}
```

| Field | Value |
| --- | --- |
| `id` | A new UUID for every notification, so the webhook can drop duplicates |
| `sent_at` | When nudge sent it, in UTC |
| `from_agent` | The herdr agent to reply to: its name, or its pane id if unnamed. `null` when sent from outside an agent pane |
| `repo` | `owner/name` from the `origin` remote, else the directory name; `null` outside git |
| `branch` | The current branch; `null` when detached or outside git |
| `status` | `done`, `blocked`, `question`, or `test` |
| `message` | The agent's text; for automatic events, a note saying so plus the pane's last lines |

<details>
<summary>Signed requests, for webhooks you host yourself</summary>

Give a webhook a `signing_secret` (at least 16 characters; `nudge setup --signing-secret VALUE`, or edit the config) and every request also carries:

| Header | Value |
| --- | --- |
| `x-nudge-timestamp` | Unix time in seconds |
| `x-nudge-signature` | `sha256=` + hex HMAC-SHA256 of `<timestamp>.<raw body>`, keyed with the secret |

To verify, recompute the HMAC over the timestamp, a `.` and the raw request body, compare in constant time, and reject timestamps more than a few minutes old. A hosted automation that only checks the `Authorization` header doesn't need this.

</details>

## herdr actions

| Action | What it does |
| --- | --- |
| `weskor.nudge.setup` | Open setup in an overlay |
| `weskor.nudge.status` | Open `nudge doctor` in an overlay |
| `weskor.nudge.settings` | Open [settings](#settings) in an overlay |
| `weskor.nudge.send-pane` | Send the focused pane to the webhook now, skipping the rules above |
| `weskor.nudge.toggle` | Turn auto-notify on or off (also `.enable`, `.disable`) |
| `weskor.nudge.away` | Tell nudge you're [away](#away-mode) or back (also `.away-on`, `.away-off`) |
| `weskor.nudge.only-workspace`, `.only-agent`, `.all-agents` | Choose [which agents can notify](#choose-which-agents-can-notify) |

Bind them in `~/.config/herdr/config.toml`, then run `herdr server reload-config`:

```toml
[[keys.command]]
key = "prefix+n"
type = "plugin_action"
command = "weskor.nudge.send-pane"
description = "nudge: send this pane to the webhook"
```

## Privacy and safety

- **What leaves your machine:** only the event above, and only to the webhook you configured. nudge has no server of its own and collects no telemetry.
- **Terminal output** is sent only by automatic notifications and `send-pane`, after redaction. Redaction covers Authorization/Bearer values, `*TOKEN*`, `*SECRET*`, `*PASSWORD*` and `*API_KEY*` assignments, OpenAI, GitHub, Slack, AWS and Google key formats, JWTs, private-key blocks and long random strings. It is a best-effort filter, not a guarantee, so leave auto-notify off for panes that print sensitive data.
- **The Authorization header** is stored in `~/.config/nudge/config.toml` with mode 600, is never printed, and never appears in error messages.
- **Your webhook's automation can type into panes.** Give it the [instructions template](docs/webhook-instructions.md), which has it treat event content as data, reply only to the agent that sent the event, and escalate credentials, deploys and other irreversible actions to you.

## Configuration

`nudge setup` writes the config for you; [config.example.toml](config.example.toml) documents the format. To add another webhook, run `nudge setup --webhook research`. `--default` makes it the webhook the plugin uses.

[Settings](#settings) edits these for you. By hand, automatic notifications are tuned in an optional `[auto]` table; leave out anything you don't change:

```toml
[auto]
min_turn_seconds = 120   # a "done" turn shorter than this isn't sent
cooldown_seconds = 300   # at most one automatic notification per pane in this window
tail_lines = 40          # terminal lines sent along (1 to 200)
toast = true             # herdr toast when one is sent

[scope]                  # who may notify; leave out for everyone
workspaces = ["api"]     # herdr workspace names (or ids)
agents = ["reviewer"]    # herdr agent names (or pane ids)
```

| Path | Holds |
| --- | --- |
| `~/.config/nudge/config.toml` | Webhooks (URL, Authorization header, optional description and signing secret), `[auto]` and `[scope]` |
| `~/.local/state/nudge/state.json` | The auto-notify toggle and per-pane timestamps |
| `~/.local/bin/nudge` | Optional link to the plugin's `nudge` command; kept current across plugin updates |

| Environment variable | Effect |
| --- | --- |
| `NUDGE_CONFIG` | Use another config file |
| `NUDGE_STATE_DIR` | Use another state directory |
| `NUDGE_AGENT` | The agent name to send as `from_agent` |
| `NO_COLOR` | Plain output |

<details>
<summary>Command reference</summary>

```text
nudge setup  [--webhook NAME] [--url URL] [--authorization VALUE | --authorization-file PATH]
             [--description TEXT] [--signing-secret VALUE] [--default] [--no-test]
             [--yes] [--no-auto-notify] [--no-install-command]
nudge notify [--webhook NAME] [--status done|blocked|question] [--agent NAME] <message...|->
nudge test   [--webhook NAME] [--agent NAME]
nudge doctor [--ping]
nudge settings
nudge list   [--json]
nudge mcp
```

- `notify -` reads the message from stdin.
- Without a terminal, `setup` takes the URL and header from flags, `--authorization-file PATH` (`-` reads stdin), the `NUDGE_WEBHOOK_URL` and `NUDGE_WEBHOOK_AUTHORIZATION` environment variables, or stdin, one per line: `printf '%s\n%s\n' "$URL" "Bearer $TOKEN" | nudge setup`. Flags win over the file, the file over the environment, and all of them over stdin and the saved webhook. It still tests before saving; `--no-test` skips that. Plain scripted setup never touches auto-notify or the command link.
- `setup --yes` (`-y`) skips the screen even in a terminal and applies step 3's defaults: auto-notify on if the herdr plugin is installed, and `~/.local/bin/nudge` linked. `--no-auto-notify` and `--no-install-command` turn those off. It needs the URL and header from flags, the file or the environment.
- Exit codes: 0 ok, 1 error, 2 usage.

</details>

## Troubleshooting

Start with `nudge doctor`, or `herdr plugin action invoke weskor.nudge.status` to see the same report in herdr. It checks each piece and says how to fix what's broken.

| Symptom | Fix |
| --- | --- |
| Setup says the webhook rejected the Authorization header | Copy the header again. It's usually `Bearer <token>`; a pasted `Authorization:` prefix is fine |
| No notification after an agent finished | `herdr plugin log list --plugin weskor.nudge` shows why it was skipped: a short turn, you were watching, cooldown, outside the scope, or auto-notify off |
| No notification while I was away, with herdr still open | The agent's pane was focused, so nudge thought you were watching. Turn on [away mode](#away-mode) before you leave |
| `nudge notify` says the agent is outside nudge's scope | Run `weskor.nudge.all-agents`, or add the agent or its workspace to `[scope]` |
| `from_agent` is `null` | The event wasn't sent from an agent's pane, so pass `--agent NAME` |
| `nudge: command not found` | Run `herdr plugin action invoke weskor.nudge.setup` and accept the command install, or add `~/.local/bin` to your `PATH` |

## Limitations

- Linux and macOS only; there's no Windows build yet.
- Automatic events contain terminal output, not a summary; for richer messages, have agents use `nudge notify`.
- Replies need a webhook automation that can reach the machine where herdr runs.

## Contributing

Bug reports and pull requests are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) to get a development setup running.

## License

[MIT](LICENSE)
