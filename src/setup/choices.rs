//! The step-3 choices after saving (auto-notify, the `nudge` command on PATH, the default webhook): what this machine
//! offers, and doing what was chosen.
use std::path::Path;

use super::SetupOptions;
use crate::env::{Env, System};
use crate::error::Result;
use crate::herdr::{Plugin, command_link, link_command, nudge_root, plugin};
use crate::state::{current_state, update_state};
use crate::terminal::tty::{Mark, Style};

/// What to do after saving (the wizard's step 3): what's offered and what's chosen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Choices {
    pub(super) plugin: Plugin,
    /// Whether to turn on auto-notify; `None` when the herdr plugin isn't installed, so there's nothing to turn on.
    pub(super) auto_notify: Option<bool>,
    /// Where `nudge` already is, as shown; `None` offers to link it.
    pub(super) command: Option<String>,
    pub(super) link_command: bool,
    pub(super) make_default: bool,
}

/// `path` with the home directory shown as `~`.
fn tilde(path: &Path, home: &Path) -> String {
    path.strip_prefix(home)
        .map_or_else(|_| path.display().to_string(), |rest| format!("~/{}", rest.display()))
}

/// What setup starts from on this machine (herdr's plugin, the command on PATH, the auto-notify state), with the
/// step-3 flags applied.
pub(super) fn choices(options: &SetupOptions, first_setup: bool, env: &Env) -> Choices {
    let plugin = plugin(env, &System);
    let link = command_link(env);
    Choices {
        // A first setup suggests auto-notify; a rerun keeps what's on.
        auto_notify: matches!(plugin, Plugin::Installed(_))
            .then(|| !options.no_auto_notify && (first_setup || current_state(env).auto)),
        plugin,
        // Plugin panes get herdr's server PATH, which usually lacks ~/.local/bin, so also accept the link itself.
        command: env
            .which("nudge")
            .or_else(|| link.exists.then_some(link.path))
            .map(|path| tilde(&path, &env.home())),
        link_command: !options.no_install_command,
        make_default: options.make_default,
    }
}

/// Do what the choices ask for, and say so.
pub(super) fn apply_choices(choices: &Choices, env: &Env) -> Result<()> {
    match (&choices.plugin, choices.auto_notify) {
        (Plugin::Installed(_), Some(auto)) => {
            update_state(env, |state| state.auto = auto)?;
            let mark = if auto { Mark::Ok } else { Mark::Warn };
            eprintln!("{mark} auto-notify {}", if auto { "on" } else { "off" });
        }
        (Plugin::Installed(_), None) | (Plugin::HerdrUnavailable, _) => eprintln!(
            "{} herdr isn't running, so auto-notify wasn't turned on. Run {} again inside herdr.",
            Mark::Warn,
            Style::Bold.paint("nudge setup")
        ),
        (Plugin::NotInstalled, _) => eprintln!(
            "{} The herdr plugin isn't installed: {}",
            Mark::Warn,
            Style::Bold.paint("herdr plugin install weskor/nudge")
        ),
    }
    match &choices.command {
        Some(shown) => eprintln!("{} nudge command {shown}", Mark::Ok),
        None if choices.link_command => match link_command(&nudge_root(env), env) {
            Ok(path) => eprintln!("{} nudge command {}", Mark::Ok, tilde(&path, &env.home())),
            Err(error) => eprintln!("{} {error}", Mark::Fail),
        },
        None => {}
    }
    Ok(())
}
