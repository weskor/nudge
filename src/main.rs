//! `nudge`: tell your webhook (an AI routine or automation) when a herdr agent is done, blocked or has a question.
mod clipboard;
mod config;
mod context;
mod doctor;
mod error;
mod install;
mod mcp;
mod notify;
mod plugin;
mod settings;
mod setup;
mod state;
mod tty;
mod ui;
mod watch;

use std::io::Read as _;
use std::process::ExitCode;

use crate::config::{default_config_path, default_webhook_name, load_config};
use crate::context::{Env, System};
use crate::error::{Error, Result};
use crate::notify::{NotifyStatus, Origin, Sent, current_dir, notify_webhook, ping_webhook};
use crate::setup::{SetupOptions, setup};

pub(crate) const VERSION: &str = env!("CARGO_PKG_VERSION");

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
           full-screen editor (herdr action weskor.nudge.settings).
  list     Show configured webhooks (* = default).
  mcp      Run the stdio MCP server (tools notify_webhook, list_webhooks).

  --agent  herdr agent the webhook should reply to. Default: $NUDGE_AGENT, else the
           agent in this herdr pane, else the only agent working in this directory.

Exit codes: 0 ok, 1 error, 2 usage.
Config: $NUDGE_CONFIG or ~/.config/nudge/config.toml";

const EXIT_ERROR: u8 = 1;
const EXIT_USAGE: u8 = 2;

/// Parsed command line. Options may appear anywhere, as `--name value` or `--name=value`.
#[derive(Debug, Default)]
#[expect(clippy::struct_excessive_bools, reason = "independent command-line flags")]
struct Args {
    positionals: Vec<String>,
    webhook: Option<String>,
    status: Option<String>,
    agent: Option<String>,
    url: Option<String>,
    authorization: Option<String>,
    authorization_file: Option<String>,
    description: Option<String>,
    signing_secret: Option<String>,
    make_default: bool,
    no_test: bool,
    yes: bool,
    no_auto_notify: bool,
    no_install_command: bool,
    ping: bool,
    pane: bool,
    json: bool,
    help: bool,
    version: bool,
}

impl From<lexopt::Error> for Error {
    fn from(error: lexopt::Error) -> Self {
        Self::Usage(error.to_string())
    }
}

/// `argv` is UTF-8 already (checked in `main`), so option values are too.
fn utf8(value: std::ffi::OsString) -> Result<String> {
    value
        .into_string()
        .map_err(|_| Error::Usage("arguments must be UTF-8".into()))
}

fn parse_args(argv: Vec<String>) -> Result<Args> {
    use lexopt::Arg::{Long, Short, Value};

    let mut parsed = Args::default();
    let mut parser = lexopt::Parser::from_args(argv);
    while let Some(token) = parser.next()? {
        match token {
            Long("webhook") => parsed.webhook = Some(utf8(parser.value()?)?),
            Long("status") => parsed.status = Some(utf8(parser.value()?)?),
            Long("agent") => parsed.agent = Some(utf8(parser.value()?)?),
            Long("url") => parsed.url = Some(utf8(parser.value()?)?),
            Long("authorization") => parsed.authorization = Some(utf8(parser.value()?)?),
            Long("authorization-file") => parsed.authorization_file = Some(utf8(parser.value()?)?),
            Long("description") => parsed.description = Some(utf8(parser.value()?)?),
            Long("signing-secret") => parsed.signing_secret = Some(utf8(parser.value()?)?),
            Long("default") => parsed.make_default = true,
            Long("no-test") => parsed.no_test = true,
            Long("yes") | Short('y') => parsed.yes = true,
            Long("no-auto-notify") => parsed.no_auto_notify = true,
            Long("no-install-command") => parsed.no_install_command = true,
            Long("ping") => parsed.ping = true,
            Long("pane") => parsed.pane = true,
            Long("json") => parsed.json = true,
            Long("help") | Short('h') => parsed.help = true,
            Long("version") | Short('v') => parsed.version = true,
            Value(positional) => parsed.positionals.push(utf8(positional)?),
            unexpected @ (Long(_) | Short(_)) => return Err(unexpected.unexpected().into()),
        }
    }
    Ok(parsed)
}

fn no_args(rest: &[String], command: &str) -> Result<()> {
    if rest.is_empty() {
        Ok(())
    } else {
        Err(Error::Usage(format!("{command} takes no arguments")))
    }
}

fn report_sent(verb: &str, sent: &Sent) {
    let notification = &sent.notification;
    let status = serde_json::to_value(notification.status).ok();
    let status = status.as_ref().and_then(serde_json::Value::as_str).unwrap_or_default();
    let agent = notification.from_agent.as_deref().unwrap_or("<none>");
    println!(
        "{verb} webhook \"{}\" (status {status}, HTTP {}) as agent {agent}.",
        sent.webhook, sent.http_status
    );
    if notification.from_agent.is_none() {
        eprintln!(
            "nudge: note: not sent from a herdr agent pane, so the webhook can't reply directly. Pass --agent NAME or set NUDGE_AGENT."
        );
    }
}

fn list(json: bool, env: &Env) -> Result<()> {
    let config = load_config(&default_config_path(env))?;
    let default_name = default_webhook_name(&config);
    let webhooks: Vec<(bool, &str, &str)> = config
        .webhook
        .iter()
        .map(|webhook| {
            (
                Some(webhook.name.as_str()) == default_name,
                webhook.name.as_str(),
                webhook.description.as_deref().unwrap_or_default(),
            )
        })
        .collect();
    if json {
        let rows: Vec<serde_json::Value> = webhooks
            .iter()
            .map(|(default, name, description)| {
                serde_json::json!({ "name": name, "default": default, "description": description })
            })
            .collect();
        println!("{}", serde_json::Value::from(rows));
    } else {
        let width = webhooks
            .iter()
            .map(|(_, name, _)| name.chars().count())
            .max()
            .unwrap_or_default();
        for (default, name, description) in webhooks {
            let marker = if default { "*" } else { " " };
            println!("{}", format!("{marker} {name:<width$}  {description}").trim_end());
        }
    }
    Ok(())
}

/// `nudge notify <message...|->`.
fn notify(rest: &[String], args: &Args, env: &Env) -> Result<u8> {
    if rest.is_empty() {
        return Err(Error::Usage(
            "usage: nudge notify [--webhook NAME] [--status STATUS] <message...|->".into(),
        ));
    }
    let status_name = args.status.as_deref().unwrap_or("done");
    let Some(status) = NotifyStatus::parse(status_name) else {
        let all: Vec<&str> = NotifyStatus::ALL.into_iter().map(NotifyStatus::as_str).collect();
        return Err(Error::Usage(format!("--status must be one of: {}", all.join(", "))));
    };
    let message = if rest == ["-"] {
        let mut input = String::new();
        std::io::stdin().read_to_string(&mut input)?;
        input
    } else {
        rest.join(" ")
    };
    let config = load_config(&default_config_path(env))?;
    let cwd = current_dir()?;
    let origin = Origin {
        env,
        cwd: &cwd,
        run: &System,
    };
    report_sent(
        "Notified",
        &notify_webhook(
            &config,
            args.webhook.as_deref(),
            status,
            &message,
            args.agent.clone(),
            origin,
        )?,
    );
    Ok(0)
}

fn run(words: Vec<String>, env: &Env) -> Result<u8> {
    // herdr plugin entrypoints parse their own arguments.
    if let Some((first, rest)) = words.split_first()
        && first == "plugin"
    {
        return Ok(plugin::run_plugin(rest, env));
    }
    let args = parse_args(words)?;
    let (command, rest) = args
        .positionals
        .split_first()
        .map_or((None, &[][..]), |(c, r)| (Some(c.as_str()), r));
    if args.version || command == Some("version") {
        println!("{VERSION}");
        return Ok(0);
    }
    if args.help || command.is_none() || command == Some("help") {
        println!("nudge {VERSION}: tell your webhook when a herdr agent is done, blocked or has a question\n\n{USAGE}");
        return Ok(if command.is_none() && !args.help { EXIT_USAGE } else { 0 });
    }
    match command.unwrap_or_default() {
        "setup" => {
            no_args(rest, "setup")?;
            let options = SetupOptions {
                webhook: args.webhook,
                url: args.url,
                authorization: args.authorization,
                authorization_file: args.authorization_file,
                description: args.description,
                signing_secret: args.signing_secret,
                make_default: args.make_default,
                test: !args.no_test,
                yes: args.yes,
                no_auto_notify: args.no_auto_notify,
                no_install_command: args.no_install_command,
                pane: args.pane,
            };
            setup(&options, env)
        }
        "notify" => notify(rest, &args, env),
        "test" => {
            no_args(rest, "test")?;
            let config = load_config(&default_config_path(env))?;
            let cwd = current_dir()?;
            let origin = Origin {
                env,
                cwd: &cwd,
                run: &System,
            };
            report_sent(
                "Pinged",
                &ping_webhook(&config, args.webhook.as_deref(), args.agent, origin)?,
            );
            Ok(0)
        }
        "doctor" => {
            no_args(rest, "doctor")?;
            doctor::doctor(args.ping, args.pane, env)
        }
        "settings" => {
            no_args(rest, "settings")?;
            settings::settings(args.pane, env)
        }
        "list" => {
            no_args(rest, "list")?;
            list(args.json, env)?;
            Ok(0)
        }
        "mcp" => {
            no_args(rest, "mcp")?;
            mcp::run_mcp(env)?;
            Ok(0)
        }
        other => Err(Error::Usage(format!("unknown command \"{other}\""))),
    }
}

fn main() -> ExitCode {
    let env = Env::from_process();
    let argv: Result<Vec<String>> = std::env::args_os().skip(1).map(utf8).collect();
    let code = match argv.and_then(|argv| run(argv, &env)) {
        Ok(code) => code,
        Err(Error::Usage(message)) => {
            eprintln!("nudge: {message}\nRun \"nudge --help\" for usage.");
            EXIT_USAGE
        }
        Err(error) => {
            eprintln!("nudge: {error}");
            EXIT_ERROR
        }
    };
    ExitCode::from(code)
}
