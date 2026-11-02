//! `nudge core watch [--once]`, `nudge core send <terminal> [--blocked]`,
//! `nudge core service install|remove|status` and `nudge core context '<json>'`: the herdr-core backend.
use super::args::Args;
use crate::core::{ServiceAction, save_app_context, send_terminal, service, watch};
use crate::env::Env;
use crate::error::{Error, Result};

/// `install`, `remove` or `status`.
fn service_action(word: &str) -> Result<ServiceAction> {
    match word {
        "install" => Ok(ServiceAction::Install),
        "remove" => Ok(ServiceAction::Remove),
        "status" => Ok(ServiceAction::Status),
        _ => Err(Error::Usage("core service takes install, remove or status".into())),
    }
}

/// `nudge core …` (the words after `core`).
pub(super) fn core(rest: &[String], args: &Args, env: &Env) -> Result<u8> {
    match rest {
        [command] if command == "watch" => {
            if args.blocked {
                return Err(Error::Usage("core watch doesn't take --blocked".into()));
            }
            watch(env, args.once)
        }
        [command, terminal] if command == "send" => {
            if args.once {
                return Err(Error::Usage("core send doesn't take --once".into()));
            }
            send_terminal(terminal, args.blocked, env)
        }
        [command, action] if command == "service" => {
            let action = service_action(action)?;
            if args.once || args.blocked {
                return Err(Error::Usage("core service takes no options".into()));
            }
            service(action, env)
        }
        [command, json] if command == "context" => {
            if args.once || args.blocked {
                return Err(Error::Usage("core context takes no options".into()));
            }
            save_app_context(env, json)
        }
        [command, ..] if command == "send" => Err(Error::Usage(
            "core send takes one terminal: t7, a name, or box/t7".into(),
        )),
        [command, ..] if command == "service" => {
            Err(Error::Usage("core service takes install, remove or status".into()))
        }
        [command, ..] if command == "context" => Err(Error::Usage(
            "core context takes one JSON argument: {terminal, window_active, spaces}".into(),
        )),
        _ => Err(Error::Usage(
            "core needs a command: watch, send <terminal>, service install|remove|status or context '<json>'".into(),
        )),
    }
}
