//! A status change every backend reduces its events to (`Transition`), which `decide` consumes.
use super::agent::AgentRef;

/// An agent's status, in words both backends share.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AgentStatus {
    Idle,
    Working,
    Blocked,
    Done,
    /// The program reported an error (herdr-core); handled like `Blocked`, with its own lead line.
    Error,
    Unknown,
}

impl AgentStatus {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Working => "working",
            Self::Blocked => "blocked",
            Self::Done => "done",
            Self::Error => "error",
            Self::Unknown => "unknown",
        }
    }

    /// Whether this status ends a turn in a way worth telling the webhook about.
    pub(crate) const fn settles(self) -> bool {
        match self {
            Self::Blocked | Self::Done | Self::Error => true,
            Self::Idle | Self::Working | Self::Unknown => false,
        }
    }
}

/// One agent's status change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Transition {
    pub(crate) agent: AgentRef,
    pub(crate) status: AgentStatus,
    /// Whether the person is looking at this agent's pane right now.
    pub(crate) watched: bool,
    /// When it happened, epoch ms.
    pub(crate) at_ms: i64,
}
