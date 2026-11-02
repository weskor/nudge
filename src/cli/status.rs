//! `nudge status --json`: what the desktop panel draws for this machine, in one JSON object. Webhooks by name and
//! host only: never a URL path, header or secret.
use serde::Serialize;

use crate::VERSION;
use crate::config::{Config, Route, default_config_path, default_webhook_name, read_config};
use crate::core::{CoreStatus, core_status};
use crate::env::{Env, Run};
use crate::error::{Error, Result};
use crate::source::host_name;
use crate::state::current_state;

#[derive(Debug, Serialize)]
struct Status<'a> {
    version: &'static str,
    host: String,
    /// The global switch (`nudge on` / `nudge off`).
    on: bool,
    auto: bool,
    away: bool,
    /// Why the config can't be read; `null` when it's fine or there is none yet.
    config_error: Option<String>,
    default_webhook: Option<&'a str>,
    webhooks: Vec<WebhookRow<'a>>,
    routes: &'a [Route],
    core: CoreStatus,
}

#[derive(Debug, Serialize)]
struct WebhookRow<'a> {
    name: &'a str,
    host: String,
    signed: bool,
}

/// Print this machine's status as JSON; `run` asks the service manager.
pub(super) fn status(json: bool, env: &Env, run: &dyn Run) -> Result<u8> {
    if !json {
        return Err(Error::Usage(
            "status prints JSON for the desktop panel: nudge status --json (nudge doctor is the readable report)"
                .into(),
        ));
    }
    let (config, config_error) = match read_config(&default_config_path(env)) {
        Ok(config) => (config, None),
        Err(error) => (None, Some(error.to_string())),
    };
    let config = config.as_ref();
    let state = current_state(env);
    let status = Status {
        version: VERSION,
        host: host_name(),
        on: state.on,
        auto: state.auto,
        away: state.away,
        config_error,
        default_webhook: config.and_then(default_webhook_name),
        webhooks: config.map_or_else(Vec::new, webhook_rows),
        routes: config.map_or(&[], |config| &config.route),
        core: core_status(env, run, config),
    };
    let text = serde_json::to_string_pretty(&status).map_err(|error| Error::Other(error.to_string()))?;
    println!("{text}");
    Ok(0)
}

fn webhook_rows(config: &Config) -> Vec<WebhookRow<'_>> {
    config
        .webhook
        .iter()
        .map(|webhook| WebhookRow {
            name: &webhook.name,
            host: webhook.host(),
            signed: webhook.signing_secret.is_some(),
        })
        .collect()
}
