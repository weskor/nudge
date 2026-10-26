//! Setup without a screen. Values come from flags, the file and the environment, then stdin (URL, then
//! Authorization, one per line), then the saved webhook. With `--yes` it also does the step-3 choices (plain
//! scripted setup leaves them alone) and says what happens next.
use std::io::{IsTerminal as _, Read as _};

use super::SetupOptions;
use super::choices::choices;
use super::outro::{finish, min_turn};
use super::ping::{explain, ping};
use super::save::save;
use super::sources::{given, saved_webhook};
use crate::config::{Webhook, default_config_path, read_config, validate_webhook};
use crate::env::Env;
use crate::error::{Error, Result};

/// What a run without a screen says when a value is missing (`yes`: name the ways to give it).
fn missing(yes: bool, url_missing: bool) -> String {
    match (yes, url_missing) {
        (true, true) => "setup --yes needs the webhook URL: --url or NUDGE_WEBHOOK_URL".into(),
        (true, false) => "setup --yes needs the Authorization header: --authorization, --authorization-file or \
             NUDGE_WEBHOOK_AUTHORIZATION"
            .into(),
        (false, _) => "setup needs the webhook URL and Authorization value: run it in a terminal, pass \
                       --url/--authorization, or pipe them on stdin (one per line)"
            .into(),
    }
}

/// The URL and the Authorization header: as given, else from stdin (one per line, unless stdin held the
/// `--authorization-file`), else the saved webhook's.
fn url_and_authorization(options: &SetupOptions, env: &Env, current: Option<&Webhook>) -> Result<(String, String)> {
    let (url, authorization) = given(options, env)?;
    let mut url = url.map(|(url, _)| url);
    let mut authorization = authorization.map(|(value, _)| value);
    let stdin_read = options.authorization_file.as_deref() == Some("-");
    if (url.is_none() || authorization.is_none()) && !stdin_read && !std::io::stdin().is_terminal() {
        let mut input = String::new();
        std::io::stdin().read_to_string(&mut input)?;
        let mut lines = input
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_owned);
        url = url.or_else(|| lines.next());
        authorization = authorization.or_else(|| lines.next());
    }
    let url = url.or_else(|| current.map(|webhook| webhook.url.clone()));
    let authorization = authorization.or_else(|| current.map(|webhook| webhook.authorization.clone()));
    let url_missing = url.is_none();
    let (Some(url), Some(authorization)) = (url, authorization) else {
        return Err(Error::Setup(missing(options.yes, url_missing)));
    };
    Ok((url, authorization))
}

/// The webhook `name` to save, checked; the description and signing secret not given are kept from `current`.
fn webhook(options: &SetupOptions, env: &Env, name: &str, current: Option<&Webhook>) -> Result<Webhook> {
    let (url, authorization) = url_and_authorization(options, env, current)?;
    let signing_secret = options
        .signing_secret
        .clone()
        .or_else(|| current.and_then(|webhook| webhook.signing_secret.clone()));
    let description = options
        .description
        .clone()
        .or_else(|| current.and_then(|webhook| webhook.description.clone()));
    validate_webhook(
        name,
        &url,
        &authorization,
        description.as_deref(),
        signing_secret.as_deref(),
    )
}

/// Test-ping `webhook` unless `--no-test`: the HTTP status of a passed ping; a failed one stops setup unsaved.
fn test(options: &SetupOptions, webhook: &Webhook, env: &Env) -> Result<Option<u16>> {
    if !options.test {
        return Ok(None);
    }
    match ping(webhook, env)? {
        Ok(status) => Ok(Some(status)),
        Err(error) => Err(Error::Setup(format!(
            "test ping failed, nothing saved: {}. Use --no-test to save without testing.",
            explain(&error)
        ))),
    }
}

/// Setup without a screen; returns the exit code.
pub(super) fn scripted(options: &SetupOptions, env: &Env) -> Result<u8> {
    let path = default_config_path(env);
    let existing = read_config(&path)?;
    let name = options.webhook.as_deref().map_or("main", str::trim);
    let current = saved_webhook(existing.as_ref(), name);
    let webhook = webhook(options, env, name, current.as_ref())?;
    let tested = test(options, &webhook, env)?;
    if options.yes {
        let choices = choices(options, existing.is_none(), env);
        let min_turn = min_turn(existing.as_ref());
        let config = save(&path, existing, webhook, choices.make_default)?;
        return finish(tested, options.test, &config, name, &choices, &min_turn, env);
    }
    if let Some(status) = tested {
        println!("Test ping to \"{name}\": HTTP {status}.");
    }
    let config = save(&path, existing, webhook, options.make_default)?;
    let as_default = if config.default_webhook.as_deref() == Some(name) {
        " (default)"
    } else {
        ""
    };
    println!("Saved webhook \"{name}\"{as_default} to {} (mode 600).", path.display());
    Ok(0)
}

#[cfg(test)]
mod tests;
