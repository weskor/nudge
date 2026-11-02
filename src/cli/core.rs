//! `nudge core watch [--once]` and `nudge core send <terminal> [--blocked]`: the herdr-core backend.
use super::args::Args;
use crate::core::{send_terminal, watch};
use crate::env::Env;
use crate::error::{Error, Result};

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
        [command, ..] if command == "send" => Err(Error::Usage(
            "core send takes one terminal: t7, a name, or box/t7".into(),
        )),
        _ => Err(Error::Usage("core needs a command: watch or send <terminal>".into())),
    }
}
