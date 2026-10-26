//! The end of a setup that saved: how the test ping went, what was saved, the step-3 choices done, and what happens
//! next.
use super::choices::{Choices, apply_choices};
use crate::config::{Auto, Config, default_config_path, seconds_in_words};
use crate::env::Env;
use crate::error::Result;
use crate::terminal::tty::{Mark, Style};

/// The shortest turn auto-notify sends, in words.
pub(super) fn min_turn(config: Option<&Config>) -> String {
    seconds_in_words(config.map_or_else(Auto::default, |config| config.auto).min_turn_seconds)
}

/// Say how the test ping went and what was saved, do what the choices ask for, and say what happens next. The
/// webhook `name` is already saved in `config`.
pub(super) fn finish(
    tested: Option<u16>,
    test: bool,
    config: &Config,
    name: &str,
    choices: &Choices,
    min_turn: &str,
    env: &Env,
) -> Result<u8> {
    match tested {
        Some(status) => eprintln!("{} Test ping: HTTP {status}, the webhook received it", Mark::Ok),
        None if test => eprintln!("{} Saved without a successful test ping", Mark::Warn),
        None => {}
    }
    let path = default_config_path(env);
    let as_default = if config.default_webhook.as_deref() == Some(name) {
        " as the default webhook"
    } else {
        ""
    };
    let location = Style::Dim.paint(format!("({}, mode 600)", path.display()));
    eprintln!("{} Saved \"{name}\"{as_default} {location}", Mark::Ok);
    apply_choices(choices, env)?;
    eprintln!();
    eprintln!("{}", Style::Bold.paint("What happens next"));
    if choices.auto_notify == Some(true) {
        eprintln!(
            "  • When an agent is blocked, or done after {min_turn} while you're not watching, your webhook gets an event."
        );
    } else {
        eprintln!("  • Turn on auto-notify in nudge settings to hear when an agent is blocked or done.");
    }
    eprintln!("  • Agents can send their own: nudge notify \"…\" (or the MCP tool notify_webhook).");
    eprintln!("  • Check everything with nudge doctor; change options with nudge settings.");
    Ok(0)
}
