//! Run one `nudge` invocation: hand plugin entrypoints to the plugin, answer help and version, else dispatch the
//! command.
use super::args::{Args, no_args, parse_args};
use super::list::list;
use super::send::{notify, ping};
use super::usage::answer_info;
use crate::env::Env;
use crate::error::{Error, Result};
use crate::setup::{SetupOptions, setup};
use crate::{doctor, mcp, plugin, settings};

/// Run the command line `words` (without the program name); returns the exit code.
pub(crate) fn run(words: Vec<String>, env: &Env) -> Result<u8> {
    // herdr plugin entrypoints parse their own arguments.
    if let Some((first, rest)) = words.split_first()
        && first == "plugin"
    {
        return Ok(plugin::run_plugin(rest, env));
    }
    let mut args = parse_args(words)?;
    let positionals = std::mem::take(&mut args.positionals);
    let (command, rest) = positionals
        .split_first()
        .map_or((None, &[][..]), |(c, r)| (Some(c.as_str()), r));
    if let Some(code) = answer_info(command, &args) {
        return Ok(code);
    }
    dispatch(command.unwrap_or_default(), rest, args, env)
}

fn dispatch(command: &str, rest: &[String], args: Args, env: &Env) -> Result<u8> {
    match command {
        "setup" => {
            no_args(rest, "setup")?;
            setup(&setup_options(args), env)
        }
        "notify" => notify(rest, &args, env),
        "test" => ping(rest, args, env),
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

/// The `nudge setup` options from the command line.
fn setup_options(args: Args) -> SetupOptions {
    SetupOptions {
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
    }
}
