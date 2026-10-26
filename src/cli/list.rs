//! `nudge list`: the configured webhooks, as a table or JSON.
use crate::config::{default_config_path, default_webhook_name, load_config};
use crate::env::Env;
use crate::error::Result;

pub(super) fn list(json: bool, env: &Env) -> Result<()> {
    let config = load_config(&default_config_path(env))?;
    let default_name = default_webhook_name(&config);
    let webhooks: Vec<(bool, &str, &str)> = config
        .webhook
        .iter()
        .map(|webhook| {
            (
                Some(webhook.name.as_str()) == default_name,
                webhook.name.as_str(),
                webhook.description.as_deref().unwrap_or_default(),
            )
        })
        .collect();
    if json {
        let rows: Vec<serde_json::Value> = webhooks
            .iter()
            .map(|(default, name, description)| {
                serde_json::json!({ "name": name, "default": default, "description": description })
            })
            .collect();
        println!("{}", serde_json::Value::from(rows));
    } else {
        let width = webhooks
            .iter()
            .map(|(_, name, _)| name.chars().count())
            .max()
            .unwrap_or_default();
        for (default, name, description) in webhooks {
            let marker = if default { "*" } else { " " };
            println!("{}", format!("{marker} {name:<width$}  {description}").trim_end());
        }
    }
    Ok(())
}
