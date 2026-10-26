//! Notification statuses: the ones an agent can send with `nudge notify --status`, and the `status` field itself.
use serde::Serialize;

/// What the agent tells the webhook with `nudge notify --status`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum NotifyStatus {
    Done,
    Blocked,
    Question,
}

impl NotifyStatus {
    pub(crate) const ALL: [Self; 3] = [Self::Done, Self::Blocked, Self::Question];

    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Done => "done",
            Self::Blocked => "blocked",
            Self::Question => "question",
        }
    }

    pub(crate) fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|status| status.as_str() == text)
    }
}

/// The `status` field of a notification.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Status {
    Done,
    Blocked,
    Question,
    /// Only sent by `nudge test` pings.
    Test,
}

impl From<NotifyStatus> for Status {
    fn from(status: NotifyStatus) -> Self {
        match status {
            NotifyStatus::Done => Self::Done,
            NotifyStatus::Blocked => Self::Blocked,
            NotifyStatus::Question => Self::Question,
        }
    }
}
