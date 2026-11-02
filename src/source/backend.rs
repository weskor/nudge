//! What a backend is (`Backend`) and what it can do for the pipeline once an agent's status settles
//! (`AgentSource`).
use serde::Serialize;

use super::agent::{AgentRef, WorkspaceRef};

/// Which herdr an agent lives in: the herdr TUI (`tui`) or herdr-core (`core`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Backend {
    Tui,
    Core,
}

impl Backend {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Tui => "tui",
            Self::Core => "core",
        }
    }

    /// `tui` or `core`; `None` for anything else.
    pub(crate) fn parse(text: &str) -> Option<Self> {
        [Self::Tui, Self::Core]
            .into_iter()
            .find(|backend| backend.as_str() == text)
    }
}

/// What a backend can do for the pipeline once a transition settles: the pipeline itself knows nothing about the
/// herdr TUI or herdr-core.
pub(crate) trait AgentSource {
    /// The last `lines` lines of the agent's terminal, raw (ANSI included); redaction happens in the pipeline.
    fn tail(&self, agent: &AgentRef, lines: usize) -> Option<String>;
    /// How the webhook's automation replies to `agent`, e.g. `reply with: herdr agent prompt api-refactor "..."`.
    fn reply_hint(&self, agent: &AgentRef) -> String;
    /// `agent`'s workspace with its name, looked up only when the scope or a route needs it.
    fn workspace(&self, agent: &AgentRef) -> Option<WorkspaceRef>;
    /// Show a short message where the person works, if this backend has a way (the TUI's toast).
    fn toast(&self, text: &str);
}
