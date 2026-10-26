//! The test ping setup sends before saving, and a failed one in plain language.
use crate::config::{Config, Webhook};
use crate::env::{Env, System};
use crate::error::{Blame, Error, Result, WebhookError};
use crate::webhook::{Origin, current_dir, ping_webhook};

/// Plain-language reason for a failed test ping.
pub(super) fn explain(error: &WebhookError) -> String {
    let detail = error
        .message
        .strip_prefix("webhook \"")
        .and_then(|rest| rest.split_once("\" "))
        .map_or(error.message.as_str(), |(_, detail)| detail);
    match error.blames() {
        Some(Blame::Authorization) => format!("the webhook rejected the Authorization header ({detail})"),
        Some(Blame::Url) => format!("there's no webhook at that URL ({detail})"),
        None => detail.to_owned(),
    }
}

/// Test-ping `webhook` alone. A webhook failure is returned, not raised.
pub(super) fn ping(webhook: &Webhook, env: &Env) -> Result<Result<u16, WebhookError>> {
    let config = Config {
        webhook: vec![webhook.clone()],
        ..Config::default()
    };
    let cwd = current_dir()?;
    match ping_webhook(
        &config,
        Some(&webhook.name),
        None,
        Origin {
            env,
            cwd: &cwd,
            run: &System,
        },
    ) {
        Ok(sent) => Ok(Ok(sent.http_status)),
        Err(Error::Webhook(error)) => Ok(Err(error)),
        Err(other) => Err(other),
    }
}
