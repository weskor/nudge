//! Choosing a configured webhook (by name or the default) and adding or replacing one.
use super::model::{Config, Webhook};
use crate::error::{Error, Result};

/// `default_webhook`, else the only webhook's name; `None` when several webhooks and no default.
pub(crate) fn default_webhook_name(config: &Config) -> Option<&str> {
    let only = match config.webhook.as_slice() {
        [only] => Some(only.name.as_str()),
        [] | [_, _, ..] => None,
    };
    config.default_webhook.as_deref().or(only)
}

/// Pick the webhook to notify: `name` if given, else the default webhook.
pub(crate) fn find_webhook<'a>(config: &'a Config, name: Option<&str>) -> Result<&'a Webhook> {
    let known = || {
        config
            .webhook
            .iter()
            .map(|webhook| webhook.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let Some(wanted) = name.or_else(|| default_webhook_name(config)) else {
        return Err(Error::Config(format!(
            "several webhooks configured ({}) and no default_webhook; pass --webhook",
            known()
        )));
    };
    config
        .webhook
        .iter()
        .find(|webhook| webhook.name == wanted)
        .ok_or_else(|| {
            Error::Config(format!(
                "unknown webhook \"{wanted}\". Configured webhooks: {}",
                known()
            ))
        })
}

/// Add or replace `webhook` by name, keeping the other settings. The first webhook, or any webhook with
/// `make_default`, becomes `default_webhook`.
pub(crate) fn upsert_webhook(config: Option<Config>, webhook: Webhook, make_default: bool) -> Config {
    let mut config = config.unwrap_or_default();
    if make_default || config.default_webhook.is_none() {
        config.default_webhook = Some(webhook.name.clone());
    }
    match config.webhook.iter_mut().find(|existing| existing.name == webhook.name) {
        Some(existing) => *existing = webhook,
        None => config.webhook.push(webhook),
    }
    config
}

#[cfg(test)]
mod tests;
