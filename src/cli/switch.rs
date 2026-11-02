//! `nudge on` and `nudge off`: nudge's global switch for this machine. While off, nothing is sent.
use super::args::no_args;
use crate::env::Env;
use crate::error::Result;
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
