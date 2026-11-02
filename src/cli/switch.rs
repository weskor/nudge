//! `nudge on` and `nudge off`, nudge's global switch for this machine (while off, nothing is sent), and
//! `nudge auto on|off` / `nudge away on|off`, the herdr actions' toggles for scripts and the desktop panel.
use super::args::no_args;
use crate::env::Env;
use crate::error::{Error, Result};
use crate::state::update_state;

/// Turn nudge on or off and say what that means.
pub(super) fn switch(on: bool, rest: &[String], env: &Env) -> Result<u8> {
    no_args(rest, if on { "on" } else { "off" })?;
    update_state(env, |state| state.on = on)?;
    println!(
        "{}",
        if on {
            "nudge is on: agents and the herdr plugin can notify webhooks."
        } else {
            "nudge is off: nothing is sent from this machine. Turn it back on with nudge on"
        }
    );
    Ok(0)
}

/// A toggle in state.json that `nudge auto` / `nudge away` sets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Toggle {
    /// Automatic notifications from terminal output.
    Auto,
    /// Away mode: the pane on screen notifies too.
    Away,
}

/// `nudge auto on|off` or `nudge away on|off`: set the toggle and say what it means (the herdr actions' toasts).
pub(super) fn toggle(which: Toggle, rest: &[String], env: &Env) -> Result<u8> {
    let name = match which {
        Toggle::Auto => "auto",
        Toggle::Away => "away",
    };
    let on = match rest {
        [word] if word == "on" => true,
        [word] if word == "off" => false,
        _ => return Err(Error::Usage(format!("{name} takes on or off"))),
    };
    update_state(env, |state| match which {
        Toggle::Auto => state.auto = on,
        Toggle::Away => state.away = on,
    })?;
    println!(
        "{}",
        match (which, on) {
            (Toggle::Auto, true) => "Auto-notify on",
            (Toggle::Auto, false) => "Auto-notify off",
            (Toggle::Away, true) => "Away: the webhook hears from every agent, even the pane on screen",
            (Toggle::Away, false) => "Back: nudge stays quiet for the pane you're looking at",
        }
    );
    Ok(0)
}
