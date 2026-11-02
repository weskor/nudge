//! The notification body: its fields, who and where it comes from, and building it from a message.
use serde::Serialize;

use super::id::event_id;
use super::origin::Origin;
use super::status::Status;
use super::time::utc_timestamp;
use crate::error::{Result, WebhookError};
use crate::git::git_info;
use crate::source::{Backend, host_name};
use crate::state::now_ms;

/// The JSON body sent to a webhook; the webhook's automation sees it inside `<webhook_event>`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct Notification {
    /// Unique per notification (UUID v4), so the webhook can drop duplicates.
    pub(crate) id: String,
    /// When nudge built the notification, UTC RFC 3339.
    pub(crate) sent_at: String,
    /// The agent target the webhook can reply to; `None` if it could not be detected.
    pub(crate) from_agent: Option<String>,
    pub(crate) cwd: String,
    pub(crate) repo: Option<String>,
    pub(crate) branch: Option<String>,
    pub(crate) status: Status,
    pub(crate) message: String,
    pub(crate) reply_hint: String,
    pub(crate) source: Source,
    /// The configured webhook's name: the one named, else the sender's route, else the default.
    pub(crate) webhook: String,
}

/// Where a notification comes from: the backend, this machine, and the agent's pane when known.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct Source {
    pub(crate) backend: Backend,
    /// This machine's hostname, so a webhook serving several machines knows where to reply.
    pub(crate) host: String,
    /// The herdr TUI pane id; `None` when the sender isn't in a known pane.
    pub(crate) pane: Option<String>,
}

/// Who a notification is from, as its backend describes them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Author {
    /// The reply target (`from_agent`).
    pub(crate) agent: Option<String>,
    pub(crate) reply_hint: String,
    pub(crate) backend: Backend,
    pub(crate) pane: Option<String>,
}

/// The message as sent: trimmed and non-empty.
pub(super) fn message_text(message: &str) -> Result<&str> {
    let message = message.trim();
    if message.is_empty() {
        return Err(WebhookError {
            message: "message is empty".into(),
            http_status: None,
        }
        .into());
    }
    Ok(message)
}

/// Build the notification for `webhook` from `author`, with a fresh id and the git repo and branch.
pub(crate) fn build_notification(
    status: Status,
    message: &str,
    author: Author,
    webhook: &str,
    origin: Origin<'_>,
) -> Result<Notification> {
    let message = message_text(message)?;
    let git = git_info(origin.cwd, origin.run);
    Ok(Notification {
        id: event_id()?,
        sent_at: utc_timestamp(now_ms().div_euclid(1000)),
        from_agent: author.agent,
        cwd: origin.cwd.to_owned(),
        repo: git.repo,
        branch: git.branch,
        status,
        message: message.to_owned(),
        reply_hint: author.reply_hint,
        source: Source {
            backend: author.backend,
            host: host_name(),
            pane: author.pane,
        },
        webhook: webhook.to_owned(),
    })
}
