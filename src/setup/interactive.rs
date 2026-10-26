//! The setup wizard in a terminal: where it starts from, the screen it runs on, and saving what it collected.
use ratatui::crossterm::event::{DisableBracketedPaste, EnableBracketedPaste};
use ratatui::crossterm::execute;

use super::SetupOptions;
use super::choices::choices;
use super::outro::{finish, min_turn};
use super::save::save;
use super::sources::{given, saved_webhook};
use super::view;
use super::wizard::{Field, Source, Sourced, Start, Wizard};
use crate::config::{Config, Webhook, check_authorization, check_webhook_url, default_config_path, read_config};
use crate::env::Env;
use crate::error::{Error, Result};

/// The step-1 values from flags, the file and the environment, checked now (the wizard shows them as accepted),
/// else the saved webhook's.
fn step_one(options: &SetupOptions, env: &Env, current: Option<&Webhook>) -> Result<(Sourced, Sourced)> {
    let (url, authorization) = given(options, env)?;
    let url = url
        .map(|(url, source)| check_webhook_url(&url).map(|url| (url, source)))
        .transpose()?
        .or_else(|| current.map(|webhook| (webhook.url.clone(), Source::Saved)));
    let authorization = authorization
        .map(|(value, source)| check_authorization(&value).map(|value| (value, source)))
        .transpose()?
        .or_else(|| current.map(|webhook| (webhook.authorization.clone(), Source::Saved)));
    Ok((url, authorization))
}

/// Where the wizard starts: the flags, the environment, the saved config and what this machine offers.
fn start(options: &SetupOptions, existing: Option<&Config>, min_turn: String, env: &Env) -> Result<Start> {
    let name = options.webhook.as_deref().map_or("main", str::trim).to_owned();
    let current = saved_webhook(existing, &name);
    let (url, authorization) = step_one(options, env, current.as_ref())?;
    Ok(Start {
        name,
        loaded: current.as_ref().map(|webhook| webhook.name.clone()),
        url,
        authorization,
        description: options
            .description
            .clone()
            .or_else(|| current.as_ref().and_then(|webhook| webhook.description.clone())),
        saved: existing
            .map(|config| config.webhook.iter().map(|webhook| webhook.name.clone()).collect())
            .unwrap_or_default(),
        default_webhook: existing.and_then(|config| config.default_webhook.clone()),
        choices: choices(options, existing.is_none(), env),
        min_turn,
        signing_secret: options
            .signing_secret
            .clone()
            .or_else(|| current.and_then(|webhook| webhook.signing_secret)),
        test: options.test,
    })
}

/// Run the wizard with the screen up and bracketed paste on, and restore the terminal however it ends.
fn on_screen(
    wizard: &mut Wizard,
    env: &Env,
    save: impl FnMut(&Wizard) -> Result<Config>,
) -> Result<(Option<u16>, Config)> {
    let mut terminal = ratatui::try_init()?;
    let ran = execute!(std::io::stdout(), EnableBracketedPaste)
        .map_err(Error::from)
        .and_then(|()| view::run(&mut terminal, wizard, env, save));
    let unpasted = execute!(std::io::stdout(), DisableBracketedPaste);
    ratatui::restore();
    unpasted?;
    ran
}

/// The setup wizard in a terminal; returns the exit code.
pub(super) fn interactive(options: &SetupOptions, env: &Env) -> Result<u8> {
    let existing = read_config(&default_config_path(env))?;
    let min_turn = min_turn(existing.as_ref());
    let mut wizard = Wizard::new(start(options, existing.as_ref(), min_turn.clone(), env)?);
    let path = default_config_path(env);
    let flag_default = options.make_default;
    // The file is read again when saving: something else may have changed it while the wizard was open.
    let save_now = |wizard: &Wizard| {
        let make_default = wizard.final_choices(flag_default).make_default;
        save(&path, read_config(&path)?, wizard.webhook()?, make_default)
    };
    let (tested, config) = on_screen(&mut wizard, env, save_now)?;
    finish(
        tested,
        options.test,
        &config,
        &wizard.entry(Field::Name).value,
        &wizard.final_choices(flag_default),
        &min_turn,
        env,
    )
}
