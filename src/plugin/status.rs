//! The `pane.agent_status_changed` hook: herdr's event, parsed once, becomes a [`Transition`] for the shared
//! pipeline, which decides whether to notify the webhook.
use serde::Deserialize;

use super::{Plugin, log};
use crate::config::{default_config_path, read_config};
use crate::error::Result;
use crate::pipeline::{Outcome, handle};
use crate::source::{AgentRef, AgentStatus, Backend, Transition};
use crate::state::now_ms;

#[derive(Debug, Deserialize)]
struct StatusEvent {
    data: StatusEventData,
}

#[derive(Debug, Deserialize)]
struct StatusEventData {
    pane_id: String,
    agent_status: HerdrStatus,
}

/// An agent status as herdr reports it in `pane.agent_status_changed`.
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum HerdrStatus {
    Idle,
    Working,
    Blocked,
    Done,
    Unknown,
}

impl From<HerdrStatus> for AgentStatus {
    fn from(status: HerdrStatus) -> Self {
        match status {
            HerdrStatus::Idle => Self::Idle,
            HerdrStatus::Working => Self::Working,
            HerdrStatus::Blocked => Self::Blocked,
            HerdrStatus::Done => Self::Done,
            HerdrStatus::Unknown => Self::Unknown,
        }
    }
}

impl Plugin<'_> {
    pub(super) fn on_status_changed(&self) -> Result<()> {
        let Some(event) = self.json_env::<StatusEvent>("HERDR_PLUGIN_EVENT_JSON") else {
            log("ignoring event without pane_id/agent_status");
            return Ok(());
        };
        let pane_id = event.data.pane_id;
        let status = AgentStatus::from(event.data.agent_status);
        let settles = status.settles();
        let source = self.source();
        let path = default_config_path(self.env);
        // Only a settling status can be sent, so only then is herdr asked about the agent and the config read. A
        // missing or broken config only matters once there is something to send; the pipeline's load reports it.
        let config = settles.then(|| read_config(&path).ok().flatten()).flatten();
        let (agent, watched) = settles
            .then(|| source.agent(&pane_id))
            .flatten()
            .unwrap_or_else(|| (AgentRef::in_pane(Backend::Tui, pane_id.clone()), false));
        let transition = Transition {
            agent,
            status,
            watched,
            at_ms: now_ms(),
        };
        match handle(transition, &source, &self.context(&path), config)? {
            Outcome::Skipped(reason) if settles => {
                log(&format!("{pane_id} {}: skipped ({reason})", status.as_str()));
            }
            Outcome::Skipped(_) => {}
            Outcome::Sent(sent) => log(&format!(
                "{pane_id} {}: notified webhook (HTTP {})",
                status.as_str(),
                sent.http_status
            )),
        }
        Ok(())
    }
}
