//! Usage text, exit codes, and the `--help` / `--version` answers.
use super::args::Args;
use crate::VERSION;

const USAGE: &str = "Usage:
  nudge setup  [--webhook NAME] [--url URL] [--authorization VALUE | --authorization-file PATH]
               [--description TEXT] [--signing-secret VALUE] [--default] [--no-test]
               [--yes] [--no-auto-notify] [--no-install-command]
  nudge notify [--webhook NAME] [--status done|blocked|question] [--agent NAME] <message...|->
  nudge test   [--webhook NAME] [--agent NAME]
  nudge on | off
  nudge auto on|off | away on|off
  nudge default <webhook>
  nudge route  ls [--json] | set (--pane NAME | --workspace NAME) --webhook NAME [--backend tui|core]
               | rm (--pane NAME | --workspace NAME) [--backend tui|core]
  nudge core   watch [--once] | send <terminal> [--blocked] | service install|remove|status
               | context '<json>'
  nudge doctor [--ping]
  nudge settings
  nudge list   [--json]
  nudge status --json
  nudge mcp

  setup    Connect a webhook, step by step: its Webhook URL and Authorization
           header (from the clipboard, pasted or typed; the header is never shown),
           a name, what to do after saving, then a test ping before saving.
           --yes (or no terminal): no screen; values from flags, the file,
           $NUDGE_WEBHOOK_URL / $NUDGE_WEBHOOK_AUTHORIZATION, or stdin (URL, then
           Authorization). --yes also turns on auto-notify and installs the nudge
           command, unless --no-auto-notify / --no-install-command.
           --authorization-file - reads the header from stdin.
           --signing-secret makes nudge sign every request (x-nudge-signature).
  notify   Send {id, sent_at, from_agent, cwd, repo, branch, status, message,
           reply_hint, source, webhook} to the webhook: --webhook, else the agent's
           route, else the default. Message \"-\" reads stdin. --status defaults to
           done. Agents outside the config's [scope] are refused. Inside a
           herdr-core terminal ($HC_TERMINAL_ID) the agent is that terminal.
  test     Send a status \"test\" ping (to the default webhook unless --webhook).
  on, off  Turn nudge on or off on this machine. While off, nothing is sent:
           notify and test fail (exit 1), and auto-notify skips.
  auto     Automatic notifications from terminal output (herdr action
           nudge.toggle); off until turned on.
  away     Away mode: the pane on screen notifies too (nudge.away).
  default  Make a configured webhook the default.
  route    Send one pane's or workspace's notifications to another webhook. Most
           specific wins: pane (agent name or pane id), then workspace (label or
           id), then the default. See nudge route --help.
  core     herdr-core. watch: follow `hc events` and notify the webhook when a
           terminal is blocked, errors, or is done after a long turn; one per
           machine, reconnecting forever (--once: until the stream ends).
           send: send a core terminal's output to its webhook now (as a
           question, or --blocked). service: run watch at every login (a
           systemd user unit on Linux, a launchd agent on macOS); status
           says whether it's installed and running. context: the desktop
           app's focus and space names, {terminal, window_active,
           spaces: [{id, name}]}, stamped with the time it arrives; the
           focused terminal isn't notified for 90 s after.
  doctor   Check config, webhook (--ping sends a test ping), herdr plugin,
           auto-notify, the nudge command and clipboard tools.
  settings Change auto-notify, who can notify and the default webhook in a
           full-screen editor (herdr action nudge.settings).
  list     Show configured webhooks (* = default).
  status   --json: this machine's nudge for the desktop panel (switches,
           webhooks by name and host, routes, the watcher, the login service,
           terminals and recent decisions). No URLs or secrets.
  mcp      Run the stdio MCP server (tools notify_webhook, list_webhooks).

  --agent  herdr agent the webhook should reply to. Default: $NUDGE_AGENT, else the
           agent in this herdr pane, else the only agent working in this directory.

Exit codes: 0 ok, 1 error (including \"nudge is off\"), 2 usage.
Config: $NUDGE_CONFIG or ~/.config/nudge/config.toml";

pub(crate) const EXIT_ERROR: u8 = 1;
pub(crate) const EXIT_USAGE: u8 = 2;

/// Print the version or the help when asked for (or the help when there's no command); returns the exit code, or
/// `None` when a command should run.
pub(super) fn answer_info(command: Option<&str>, args: &Args) -> Option<u8> {
    if args.version || command == Some("version") {
        println!("{VERSION}");
        return Some(0);
    }
    if args.help || command.is_none() || command == Some("help") {
        println!("nudge {VERSION}: tell your webhook when a herdr agent is done, blocked or has a question\n\n{USAGE}");
        return Some(if command.is_none() && !args.help { EXIT_USAGE } else { 0 });
    }
    None
}
