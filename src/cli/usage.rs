//! Usage text, exit codes, and the `--help` / `--version` answers.
use super::args::Args;
use crate::VERSION;

const USAGE: &str = "Usage:
  nudge setup  [--webhook NAME] [--url URL] [--authorization VALUE | --authorization-file PATH]
               [--description TEXT] [--signing-secret VALUE] [--default] [--no-test]
               [--yes] [--no-auto-notify] [--no-install-command]
  nudge notify [--webhook NAME] [--status done|blocked|question] [--agent NAME] <message...|->
  nudge test   [--webhook NAME] [--agent NAME]
  nudge doctor [--ping]
  nudge settings
  nudge list   [--json]
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
           reply_hint} to the webhook. Message \"-\" reads stdin. --status
           defaults to done. Agents outside the config's [scope] are refused.
  test     Send a status \"test\" ping.
  doctor   Check config, webhook (--ping sends a test ping), herdr plugin,
           auto-notify, the nudge command and clipboard tools.
  settings Change auto-notify, who can notify and the default webhook in a
           full-screen editor (herdr action nudge.settings).
  list     Show configured webhooks (* = default).
  mcp      Run the stdio MCP server (tools notify_webhook, list_webhooks).

  --agent  herdr agent the webhook should reply to. Default: $NUDGE_AGENT, else the
           agent in this herdr pane, else the only agent working in this directory.

Exit codes: 0 ok, 1 error, 2 usage.
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
