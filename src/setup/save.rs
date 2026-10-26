//! Writing the webhook setup collected into the config file.
use std::path::Path;

use crate::config::{Config, Webhook, upsert_webhook, write_config};
use crate::error::Result;

/// Save `webhook` into the config at `path`; returns the new config.
pub(super) fn save(path: &Path, existing: Option<Config>, webhook: Webhook, make_default: bool) -> Result<Config> {
    let config = upsert_webhook(existing, webhook, make_default);
    write_config(path, &config)?;
    Ok(config)
}
