//! Serializing a [`Config`] to the TOML `nudge setup` writes, leaving out absent and default settings.
use super::model::{Auto, Config, Scope, Webhook};

/// A TOML basic string. JSON string escapes are a subset of TOML's.
fn toml_string(value: &str) -> String {
    serde_json::Value::from(value).to_string()
}

fn toml_strings(values: &[String]) -> String {
    let items: Vec<String> = values.iter().map(|value| toml_string(value)).collect();
    format!("[{}]", items.join(", "))
}

pub(super) fn serialize_config(config: &Config) -> String {
    let mut lines = vec![
        "# nudge config, written by `nudge setup`.".to_owned(),
        "# Contains webhook secrets: keep it chmod 600 and out of git.".to_owned(),
        String::new(),
    ];
    if let Some(name) = &config.default_webhook {
        lines.extend([format!("default_webhook = {}", toml_string(name)), String::new()]);
    }
    for webhook in &config.webhook {
        lines.extend(webhook_table(webhook));
    }
    lines.extend(auto_table(config.auto));
    lines.extend(scope_table(&config.scope));
    lines.join("\n")
}

/// One `[[webhook]]` table, followed by a blank line.
fn webhook_table(webhook: &Webhook) -> Vec<String> {
    let mut lines = vec![
        "[[webhook]]".to_owned(),
        format!("name = {}", toml_string(&webhook.name)),
    ];
    if let Some(description) = &webhook.description {
        lines.push(format!("description = {}", toml_string(description)));
    }
    lines.extend([
        format!("url = {}", toml_string(&webhook.url)),
        format!("authorization = {}", toml_string(&webhook.authorization)),
    ]);
    if let Some(secret) = &webhook.signing_secret {
        lines.push(format!("signing_secret = {}", toml_string(secret)));
    }
    lines.push(String::new());
    lines
}

/// The `[auto]` table with only the settings that differ from the defaults, so a later default change reaches
/// this config too; nothing when every setting is the default.
fn auto_table(auto: Auto) -> Vec<String> {
    let defaults = Auto::default();
    let changed: Vec<String> = [
        (auto.min_turn_seconds != defaults.min_turn_seconds)
            .then(|| format!("min_turn_seconds = {}", auto.min_turn_seconds)),
        (auto.cooldown_seconds != defaults.cooldown_seconds)
            .then(|| format!("cooldown_seconds = {}", auto.cooldown_seconds)),
        (auto.tail_lines != defaults.tail_lines).then(|| format!("tail_lines = {}", auto.tail_lines)),
        (auto.toast != defaults.toast).then(|| format!("toast = {}", auto.toast)),
    ]
    .into_iter()
    .flatten()
    .collect();
    if changed.is_empty() {
        return changed;
    }
    let mut lines = vec!["[auto]".to_owned()];
    lines.extend(changed);
    lines.push(String::new());
    lines
}

/// The `[scope]` table; nothing when every agent may notify.
fn scope_table(scope: &Scope) -> Vec<String> {
    if scope.is_everyone() {
        return Vec::new();
    }
    let mut lines = vec!["[scope]".to_owned()];
    if !scope.workspaces.is_empty() {
        lines.push(format!("workspaces = {}", toml_strings(&scope.workspaces)));
    }
    if !scope.agents.is_empty() {
        lines.push(format!("agents = {}", toml_strings(&scope.agents)));
    }
    lines.push(String::new());
    lines
}

#[cfg(test)]
mod tests;
