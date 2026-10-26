//! `nudge settings`: a terminal UI for auto-notify, who can notify, and the default webhook. herdr opens it as an
//! overlay (`nudge.settings`). It edits the same config and state as the actions and `nudge setup`.
mod choices;
mod edit;
mod keys;
mod lines;
mod ping;
mod presets;
mod rows;
mod run;
mod save;
mod state;

use std::io::IsTerminal as _;

use self::run::run_ui;
use self::state::Settings;
use crate::config::{default_config_path, load_config};
use crate::env::{Env, Run as _, System};
use crate::error::{Error, Result};
use crate::herdr::{herdr_agents, herdr_workspaces};
use crate::state::current_state;

pub(crate) fn settings(pane: bool, env: &Env) -> Result<u8> {
    let path = default_config_path(env);
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return Err(Error::Usage(format!(
            "settings needs a terminal; edit {} instead (see config.example.toml)",
            path.display()
        )));
    }
    let config = load_config(&path)?;
    let mut settings = Settings::new(
        &config,
        &current_state(env),
        &herdr_workspaces(env, &System),
        &herdr_agents(env, &System),
    );
    let mut terminal = ratatui::try_init()?;
    let saved = run_ui(&mut terminal, &mut settings, &config, env);
    ratatui::restore();
    if saved? {
        println!("Saved nudge settings to {}.", path.display());
        if pane {
            let _shown = System.run(
                &[env.herdr(), "notification", "show", "nudge", "--body", "Settings saved"],
                &env.home(),
            );
        }
    } else {
        println!("No changes saved.");
    }
    Ok(0)
}

#[cfg(test)]
mod tests;
