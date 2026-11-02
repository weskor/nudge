//! The global switch's one check: every webhook request nudge makes goes through [`deliver`].
use super::payload::Notification;
use super::send::post_to_webhook;
use crate::config::Webhook;
use crate::env::Env;
use crate::error::{Error, Result};
use crate::state::current_state;

/// POST `payload` to `webhook`, unless nudge is off (`nudge off`): then nothing is sent and the error is
/// [`Error::Off`]. Returns the webhook's 2xx status.
pub(crate) fn deliver(env: &Env, webhook: &Webhook, payload: &Notification) -> Result<u16> {
    if !current_state(env).on {
        return Err(Error::Off);
    }
    Ok(post_to_webhook(webhook, payload)?)
}
