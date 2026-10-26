//! Config checks: the config file, its default webhook, the scope, and (with `--ping`) a test ping.
use std::path::Path;

use super::check::{Check, Level};
use crate::config::{Config, default_config_path, default_webhook_name, read_config};
use crate::env::{Env, System};
use crate::error::{Blame, Error, Result};
use crate::herdr::PLUGIN_ID;
use crate::webhook::{Origin, current_dir, ping_webhook};

pub(super) fn check_config(ping: bool, env: &Env) -> Result<Vec<Check>> {
    let path = default_config_path(env);
    let config = match load(&path)? {
        Ok(config) => config,
        Err(check) => return Ok(vec![check]),
    };
    let shown = path.display().to_string();
    let count = config.webhook.len();
    let plural = if count == 1 { "" } else { "s" };
    let Some(default_name) = default_webhook_name(&config) else {
        return Ok(vec![
            Check::new(Level::Warn, format!("Config: {count} webhook{plural}, default not set"))
                .detail(shown)
                .fix("run nudge setup --webhook NAME --default (the plugin sends to the default webhook)"),
        ]);
    };
    let mut checks = vec![
        Check::new(
            Level::Ok,
            format!("Config: {count} webhook{plural}, default \"{default_name}\""),
        )
        .detail(shown),
        check_scope(&config),
    ];
    if ping {
        checks.push(check_ping(&config, default_name, env)?);
    } else {
        checks.push(Check::new(Level::Ok, "Webhook not tested").detail("nudge doctor --ping sends a test ping"));
    }
    Ok(checks)
}

/// The config at `path`, or the failed check when there is none or it's invalid.
fn load(path: &Path) -> Result<Result<Config, Check>> {
    let shown = || path.display().to_string();
    match read_config(path) {
        Ok(Some(config)) => Ok(Ok(config)),
        Ok(None) => Ok(Err(Check::new(Level::Fail, "No webhook configured")
            .detail(shown())
            .fix("run nudge setup"))),
        Err(Error::Config(message)) => {
            let fix = message.replace('\n', "\n      ");
            Ok(Err(Check::new(Level::Fail, "Config is invalid")
                .detail(shown())
                .fix(fix)))
        }
        Err(other) => Err(other),
    }
}

/// Which agents may notify, per the config's `[scope]`.
fn check_scope(config: &Config) -> Check {
    if config.scope.is_everyone() {
        return Check::new(Level::Ok, "Scope: all agents");
    }
    let list = |label: &str, names: &[String]| (!names.is_empty()).then(|| format!("{label} {}", names.join(", ")));
    let parts: Vec<String> = [
        list("workspaces", &config.scope.workspaces),
        list("agents", &config.scope.agents),
    ]
    .into_iter()
    .flatten()
    .collect();
    Check::new(Level::Ok, format!("Scope: {}", parts.join("; "))).fix(format!(
        "herdr plugin action invoke {PLUGIN_ID}.all-agents lets every agent notify"
    ))
}

/// Send a test ping to the default webhook; a failure says whether to copy the header or the URL again.
fn check_ping(config: &Config, default_name: &str, env: &Env) -> Result<Check> {
    let cwd = current_dir()?;
    let origin = Origin {
        env,
        cwd: &cwd,
        run: &System,
    };
    match ping_webhook(config, Some(default_name), None, origin) {
        Ok(sent) => Ok(Check::new(
            Level::Ok,
            format!("Webhook accepted a test ping (HTTP {})", sent.http_status),
        )),
        Err(Error::Webhook(error)) => {
            let fix = match error.blames() {
                Some(Blame::Authorization) => {
                    format!("copy the Authorization header again: nudge setup --webhook {default_name}")
                }
                Some(Blame::Url) => format!("copy the Webhook URL again: nudge setup --webhook {default_name}"),
                None => "check that the webhook is running, then nudge test".to_owned(),
            };
            Ok(Check::new(Level::Fail, "Webhook test failed")
                .detail(error.message)
                .fix(fix))
        }
        Err(other) => Err(other),
    }
}
