//! Saving: the edits written into the config and the saved toggles, keeping everything else as it is.
use super::choices::Choice;
use super::state::Settings;
use crate::config::{Config, Scope, default_config_path, load_config, write_config};
use crate::env::Env;
use crate::error::Result;
use crate::state::{State, update_state};

impl Settings {
    /// Write the edited settings into `config` (other settings and webhooks stay as they are).
    pub(crate) fn apply(&self, config: &mut Config) {
        config.auto = self.auto;
        let ticked = |choices: &[Choice]| -> Vec<String> {
            choices
                .iter()
                .filter(|choice| choice.checked)
                .map(|choice| choice.name.clone())
                .collect()
        };
        config.scope = if self.only {
            Scope {
                workspaces: ticked(&self.workspaces),
                agents: ticked(&self.agents),
            }
        } else {
            Scope::default()
        };
        if let Some(name) = &self.default_webhook
            && config.webhook.iter().any(|webhook| &webhook.name == name)
        {
            config.default_webhook = Some(name.clone());
        }
    }

    /// Write the edited toggles into `state` (pane timestamps stay as they are).
    pub(crate) const fn apply_state(&self, state: &mut State) {
        state.auto = self.auto_on;
        state.away = self.away;
    }
}

/// Save: re-read the config so webhooks edited meanwhile are kept, apply the edits, write it and the toggle.
pub(super) fn save(settings: &Settings, env: &Env) -> Result<()> {
    let path = default_config_path(env);
    let mut config = load_config(&path)?;
    settings.apply(&mut config);
    write_config(&path, &config)?;
    update_state(env, |state| settings.apply_state(state))?;
    Ok(())
}

#[cfg(test)]
mod tests;
