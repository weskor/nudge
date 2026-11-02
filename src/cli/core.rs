//! `nudge core watch [--once]`, `nudge core send <terminal> [--blocked]` and
//! `nudge core service install|remove|status`: the herdr-core backend.
use super::args::Args;
use crate::core::{ServiceAction, send_terminal, service, watch};
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
        [command, ..] if command == "send" => Err(Error::Usage(
            "core send takes one terminal: t7, a name, or box/t7".into(),
        )),
        [command, ..] if command == "service" => {
            Err(Error::Usage("core service takes install, remove or status".into()))
        }
        _ => Err(Error::Usage(
            "core needs a command: watch, send <terminal> or service install|remove|status".into(),
        )),
    }
}
