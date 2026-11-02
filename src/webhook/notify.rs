//! The two ways to send: `notify_webhook` (an agent reporting in, within `[scope]`, along its route) and
//! `ping_webhook` (a test).
use super::deliver::deliver;
use super::origin::{Origin, sender};
use super::payload::{Notification, build_notification, message_text};
use super::status::{NotifyStatus, Status};
use crate::config::{Backend, Config, find_webhook, routes_name_workspaces};
use crate::env::Env;
use crate::error::{Error, Result};
use crate::herdr::{Sender, SenderIds};
use crate::state::{now_ms, tui_key, update_state};

/// A notification the webhook accepted (`notification.webhook` names it).
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Sent {
    /// HTTP status the webhook answered with.
    pub(crate) http_status: u16,
    pub(crate) notification: Notification,
}

/// Tell a webhook that this agent is done, blocked or has a question: `webhook` if given, else the one the sender's
/// `[[route]]` names, else the default webhook. Refuses agents outside the config's `[scope]`. Inside a herdr pane,
/// records that the agent notified, so the herdr plugin doesn't send a duplicate.
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
    let sender = sender(agent, origin);
    let workspace =
        !config.scope.workspaces.is_empty() || (fixed.is_none() && routes_name_workspaces(&config.route, Backend::Tui));
    let ids = SenderIds::lookup(&sender, workspace, origin.env, origin.run);
    if !ids.in_scope(&config.scope) {
        return Err(out_of_scope(&sender));
    }
    let webhook = match fixed {
        Some(webhook) => webhook,
        None => find_webhook(config, ids.route(&config.route).map(|route| route.webhook.as_str()))?,
    };
    let notification = build_notification(status.into(), message, sender.agent, &webhook.name, origin)?;
    let http_status = deliver(origin.env, webhook, &notification)?;
    if let Some(pane_id) = origin.env.get("HERDR_PANE_ID") {
        record_self_notified(origin.env, pane_id);
    }
    Ok(Sent {
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
        state.panes.entry(tui_key(pane_id)).or_default().self_notified_at = Some(now);
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
    let notification = build_notification(
        Status::Test,
        "nudge test ping. No action needed.",
        from_agent,
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
