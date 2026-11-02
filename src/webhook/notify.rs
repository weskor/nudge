//! The two ways to send: `notify_webhook` (an agent reporting in, within `[scope]`, along its route) and
//! `ping_webhook` (a test).
use super::deliver::deliver;
use super::origin::Origin;
use super::payload::{Notification, build_notification, message_text};
use super::status::{NotifyStatus, Status};
use super::who::who;
use crate::config::{Config, find_webhook};
use crate::env::Env;
use crate::error::Result;
use crate::state::{now_ms, update_state};

/// A notification the webhook accepted (`notification.webhook` names it).
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Sent {
    /// HTTP status the webhook answered with.
    pub(crate) http_status: u16,
    pub(crate) notification: Notification,
}

/// Tell a webhook that this agent is done, blocked or has a question: `webhook` if given, else the one the sender's
/// `[[route]]` names, else the default webhook. Refuses agents outside the config's `[scope]`. Inside a herdr pane
/// or core terminal, records that the agent notified, so automatic notifications don't send a duplicate.
pub(crate) fn notify_webhook(
    config: &Config,
    webhook: Option<&str>,
    status: NotifyStatus,
    message: &str,
    agent: Option<String>,
    origin: Origin<'_>,
) -> Result<Sent> {
    // Named, or the default with no routes to consider: checked first, so a config problem shows before any lookup.
    let fixed = (webhook.is_some() || config.route.is_empty())
        .then(|| find_webhook(config, webhook))
        .transpose()?;
    let message = message_text(message)?;
    let who = who(agent, origin);
    let checked = who.check(config, fixed.is_none(), origin)?;
    let webhook = match fixed {
        Some(webhook) => webhook,
        None => find_webhook(config, checked.route.as_deref())?,
    };
    let notification = build_notification(status.into(), message, who.author(), &webhook.name, origin)?;
    let http_status = deliver(origin.env, webhook, &notification)?;
    if let Some(key) = checked.state_key {
        record_self_notified(origin.env, key);
    }
    Ok(Sent {
        http_status,
        notification,
    })
}

/// Record that the agent under `key` notified the webhook itself, so automatic notifications don't duplicate it.
fn record_self_notified(env: &Env, key: String) {
    let now = now_ms();
    // Best effort: a missing record only risks one duplicate automatic notification.
    update_state(env, |state| {
        state.panes.entry(key).or_default().self_notified_at = Some(now);
    })
    .ok();
}

/// Send a harmless `status: "test"` notification to check the webhook URL and Authorization header. Ignores
/// `[scope]`: it tests the webhook, not an agent.
pub(crate) fn ping_webhook(
    config: &Config,
    webhook: Option<&str>,
    agent: Option<String>,
    origin: Origin<'_>,
) -> Result<Sent> {
    let webhook = find_webhook(config, webhook)?;
    let notification = build_notification(
        Status::Test,
        "nudge test ping. No action needed.",
        who(agent, origin).author(),
        &webhook.name,
        origin,
    )?;
    let http_status = deliver(origin.env, webhook, &notification)?;
    Ok(Sent {
        http_status,
        notification,
    })
}

#[cfg(test)]
mod tests;
