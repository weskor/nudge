//! The notification body: its fields, the reply hint, and building it from a message and where it's sent from.
use serde::Serialize;

use super::id::event_id;
use super::origin::Origin;
use super::status::Status;
use super::time::utc_timestamp;
use crate::error::{Result, WebhookError};
use crate::git::git_info;
use crate::state::now_ms;

/// The JSON body sent to a webhook; the webhook's automation sees it inside `<webhook_event>`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct Notification {
    /// Unique per notification (UUID v4), so the webhook can drop duplicates.
    pub(crate) id: String,
    /// When nudge built the notification, UTC RFC 3339.
    pub(crate) sent_at: String,
    /// herdr agent target the webhook can reply to; `None` if it could not be detected.
    pub(crate) from_agent: Option<String>,
    pub(crate) cwd: String,
    pub(crate) repo: Option<String>,
    pub(crate) branch: Option<String>,
    pub(crate) status: Status,
    pub(crate) message: String,
    pub(crate) reply_hint: String,
}

pub(super) fn reply_hint(agent: Option<&str>) -> String {
    let Some(agent) = agent else {
        return "sender's herdr agent could not be detected, so it cannot be prompted directly".to_owned();
    };
    let shell_safe = !agent.is_empty()
        && agent
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._:@%+=,-".contains(c));
    let target = if shell_safe {
        agent.to_owned()
    } else {
        format!("'{}'", agent.replace('\'', r"'\''"))
    };
    format!("reply with: herdr agent prompt {target} \"...\"")
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

/// Build the notification from `from_agent` (the reply target), with a fresh id and the git repo and branch.
pub(crate) fn build_notification(
    status: Status,
    message: &str,
    from_agent: Option<String>,
    origin: Origin<'_>,
) -> Result<Notification> {
    let message = message_text(message)?;
    let git = git_info(origin.cwd, origin.run);
    Ok(Notification {
        id: event_id()?,
        sent_at: utc_timestamp(now_ms().div_euclid(1000)),
        reply_hint: reply_hint(from_agent.as_deref()),
        from_agent,
        cwd: origin.cwd.to_owned(),
        repo: git.repo,
        branch: git.branch,
        status,
        message: message.to_owned(),
    })
}

#[cfg(test)]
mod tests;
