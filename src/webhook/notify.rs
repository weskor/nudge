//! The two ways to send: `notify_webhook` (an agent reporting in, within `[scope]`) and `ping_webhook` (a test).
use super::origin::{Origin, sender};
use super::payload::{Notification, build_notification, message_text};
use super::send::post_to_webhook;
use super::status::{NotifyStatus, Status};
use crate::config::{Config, find_webhook};
use crate::env::Env;
use crate::error::{Error, Result};
use crate::herdr::{Sender, in_scope};
use crate::state::{now_ms, update_state};

/// A notification the webhook accepted.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Sent {
    pub(crate) webhook: String,
    /// HTTP status the webhook answered with.
    pub(crate) http_status: u16,
    pub(crate) notification: Notification,
}

/// Tell a webhook (the default webhook unless `webhook` is given) that this agent is done, blocked or has a
/// question. Refuses agents outside the config's `[scope]`. Inside a herdr pane, records that the agent notified,
/// so the herdr plugin doesn't send a duplicate.
pub(crate) fn notify_webhook(
    config: &Config,
    webhook: Option<&str>,
    status: NotifyStatus,
    message: &str,
    agent: Option<String>,
    origin: Origin<'_>,
) -> Result<Sent> {
    let webhook = find_webhook(config, webhook)?;
    let message = message_text(message)?;
    let sender = sender(agent, origin);
    if !in_scope(&config.scope, &sender, origin.env, origin.run) {
        return Err(out_of_scope(&sender));
    }
    let notification = build_notification(status.into(), message, sender.agent, origin)?;
    let http_status = post_to_webhook(webhook, &notification)?;
    if let Some(pane_id) = origin.env.get("HERDR_PANE_ID") {
        record_self_notified(origin.env, pane_id);
    }
    Ok(Sent {
        webhook: webhook.name.clone(),
        http_status,
        notification,
    })
}

/// The refusal for a sender outside nudge's `[scope]`.
fn out_of_scope(sender: &Sender) -> Error {
    let who = sender
        .agent
        .as_deref()
        .map_or_else(|| "this agent".to_owned(), |agent| format!("agent \"{agent}\""));
    let place = sender
        .workspace_id
        .as_deref()
        .map(|id| format!(" in workspace \"{id}\""))
        .unwrap_or_default();
    Error::Config(format!(
        "{who}{place} is outside nudge's scope, so nothing was sent. Change [scope] in the nudge config, \
         or run the herdr action \"Nudge: notify from all agents\"."
    ))
}

/// Record that the agent in `pane_id` notified the webhook itself, so the herdr plugin doesn't send a duplicate.
fn record_self_notified(env: &Env, pane_id: &str) {
    let now = now_ms();
    // Best effort: a missing record only risks one duplicate automatic notification.
    update_state(env, |state| {
        state.panes.entry(pane_id.to_owned()).or_default().self_notified_at = Some(now);
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
    let from_agent = sender(agent, origin).agent;
    let notification = build_notification(Status::Test, "nudge test ping. No action needed.", from_agent, origin)?;
    let http_status = post_to_webhook(webhook, &notification)?;
    Ok(Sent {
        webhook: webhook.name.clone(),
        http_status,
        notification,
    })
}

#[cfg(test)]
mod tests;
