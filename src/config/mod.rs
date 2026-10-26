//! The webhook config (`~/.config/nudge/config.toml`): parsing with every problem reported, and safe writes.
mod check;
mod migrate;
mod model;
mod parse;
mod problems;
mod read;
mod select;
mod serialize;
mod write;

pub(crate) use check::{check_authorization, check_webhook_name, check_webhook_url, validate_webhook};
pub(crate) use model::{Auto, Config, Scope, Webhook, seconds_in_words};
pub(crate) use read::{default_config_path, load_config, read_config};
pub(crate) use select::{default_webhook_name, find_webhook, upsert_webhook};
pub(crate) use write::write_config;

#[cfg(test)]
mod tests;
