//! Parsing config TOML into a [`Config`], checking the schema and reporting every problem at once.
mod auto;
mod scope;
mod webhooks;

use toml::Value;

use super::migrate::RENAMED_KEYS;
use super::model::Config;
use super::problems::Problems;
use crate::error::{Error, Result};

impl Problems {
    fn string<'a>(&mut self, path: &str, value: Option<&'a Value>) -> Option<&'a str> {
        if let Some(Value::String(text)) = value {
            Some(text)
        } else {
            self.wrong_type(path, "string", value);
            None
        }
    }

    fn config(&mut self, raw: &toml::Table) -> Option<Config> {
        self.old_keys(raw);
        let default_webhook = match raw.get("default_webhook") {
            None => Some(None),
            Some(value) => self
                .string("default_webhook", Some(value))
                .map(|name| Some(self.webhook_name("default_webhook", name))),
        };
        let webhooks = self.webhooks(raw.get("webhook"));
        let auto = self.auto(raw.get("auto"));
        let scope = self.scope(raw.get("scope"));
        let config = Config {
            default_webhook: default_webhook?,
            webhook: webhooks?,
            auto,
            scope,
        };
        if let Some(name) = &config.default_webhook
            && !config.webhook.iter().any(|webhook| &webhook.name == name)
        {
            self.add("default_webhook", &format!("no [[webhook]] named \"{name}\""));
        }
        Some(config)
    }

    /// Old-format keys are an error here; only a file using nothing but old keys is migrated (see `read`).
    fn old_keys(&mut self, raw: &toml::Table) {
        for (old, new) in RENAMED_KEYS {
            if raw.contains_key(old) {
                self.add(
                    old,
                    &format!("old name of {new}; rename it (a file mixing old and new names is not updated for you)"),
                );
            }
        }
    }
}

pub(super) fn parse_toml(text: &str, source: &str) -> Result<toml::Table> {
    toml::from_str(text).map_err(|error| Error::Config(format!("{source}: invalid TOML: {}", error.message())))
}

pub(super) fn validate_config(raw: &toml::Table, source: &str) -> Result<Config> {
    let mut problems = Problems::default();
    let config = problems.config(raw);
    let label = format!("{source}: invalid config");
    problems.into_result(config, &label)?.ok_or(Error::Config(label))
}

/// Parse and validate config TOML. `source` names the file in error messages.
pub(super) fn parse_config(text: &str, source: &str) -> Result<Config> {
    validate_config(&parse_toml(text, source)?, source)
}

#[cfg(test)]
mod tests;
