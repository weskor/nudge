//! `nudge setup`: connect a webhook, test it before saving, then turn on the herdr plugin's auto-notify and put the
//! `nudge` command on PATH. In a terminal it is a step-by-step wizard (`interactive`; `wizard` holds its state,
//! `view` draws and runs it); otherwise it reads flags and stdin (`scripted`).
mod choices;
mod interactive;
mod options;
mod outro;
mod paste;
mod ping;
mod save;
mod scripted;
mod sources;
mod view;
mod wizard;

use std::io::IsTerminal as _;

use self::interactive::interactive;
pub(crate) use self::options::SetupOptions;
use self::scripted::scripted;
use crate::env::Env;
use crate::error::{Error, Result};
use crate::terminal::tty::{Style, wait_for_key};

pub(crate) fn setup(options: &SetupOptions, env: &Env) -> Result<u8> {
    if options.yes || !std::io::stdin().is_terminal() {
        return scripted(options, env);
    }
    let result = match interactive(options, env) {
        Err(Error::Cancelled) => {
            eprintln!("{}", Style::Dim.paint("Cancelled; nothing saved."));
            Ok(1)
        }
        other => other,
    };
    if options.pane {
        wait_for_key();
    }
    result
}
