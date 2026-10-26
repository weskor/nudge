//! The config data model: webhooks, `[auto]`, `[scope]` and the config that holds them.
use std::fmt;

use crate::terminal::tty::Redacted;

/// A configured webhook. `Debug` redacts the Authorization header and the signing secret.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Webhook {
    pub(crate) name: String,
    pub(crate) description: Option<String>,
    pub(crate) url: String,
    /// Sent verbatim as the webhook's Authorization header value.
    pub(crate) authorization: String,
    /// When set, requests are signed with HMAC-SHA256 (`x-nudge-signature`) so the webhook can verify them.
    pub(crate) signing_secret: Option<String>,
}

impl fmt::Debug for Webhook {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Webhook")
            .field("name", &self.name)
            .field("description", &self.description)
            .field("url", &self.url)
            .field("authorization", &Redacted(&self.authorization))
            .field("signing_secret", &self.signing_secret.as_deref().map(Redacted))
            .finish()
    }
}

/// `[auto]`: when the herdr plugin notifies the webhook on its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Auto {
    /// A `done` turn shorter than this is not worth a notification.
    pub(crate) min_turn_seconds: u32,
    /// At most one automatic notification per pane in this window.
    pub(crate) cooldown_seconds: u32,
    /// Lines of terminal output sent with an automatic notification.
    pub(crate) tail_lines: u32,
    /// Show a herdr toast when an automatic notification was sent.
    pub(crate) toast: bool,
}

impl Default for Auto {
    fn default() -> Self {
        Self {
            min_turn_seconds: 120,
            cooldown_seconds: 300,
            tail_lines: 40,
            toast: true,
        }
    }
}

impl Auto {
    pub(crate) fn min_turn_ms(self) -> i64 {
        i64::from(self.min_turn_seconds) * 1000
    }

    pub(crate) fn cooldown_ms(self) -> i64 {
        i64::from(self.cooldown_seconds) * 1000
    }
}

/// A duration in words: "2 minutes", "1 minute", "90 seconds", "0 seconds".
pub(crate) fn seconds_in_words(seconds: u32) -> String {
    match (seconds / 60, seconds % 60) {
        (1, 0) => "1 minute".to_owned(),
        (minutes @ 2.., 0) => format!("{minutes} minutes"),
        (_, _) if seconds == 1 => "1 second".to_owned(),
        (_, _) => format!("{seconds} seconds"),
    }
}

/// `[scope]`: which agents may notify the webhook. Empty means every agent.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Scope {
    /// herdr workspace labels or ids.
    pub(crate) workspaces: Vec<String>,
    /// herdr agent names or pane ids.
    pub(crate) agents: Vec<String>,
}

impl Scope {
    pub(crate) const fn is_everyone(&self) -> bool {
        self.workspaces.is_empty() && self.agents.is_empty()
    }

    /// Whether an agent known by any of `agent_ids` (name, pane id), in a workspace known by any of
    /// `workspace_ids` (label, id), may notify.
    pub(crate) fn allows(&self, agent_ids: &[&str], workspace_ids: &[&str]) -> bool {
        self.is_everyone()
            || self.agents.iter().any(|agent| agent_ids.contains(&agent.as_str()))
            || self
                .workspaces
                .iter()
                .any(|workspace| workspace_ids.contains(&workspace.as_str()))
    }
}

/// A config as read from disk. `Default` is the empty config `nudge setup` starts from.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Config {
    pub(crate) default_webhook: Option<String>,
    pub(crate) webhook: Vec<Webhook>,
    pub(crate) auto: Auto,
    pub(crate) scope: Scope,
}

#[cfg(test)]
mod tests;
