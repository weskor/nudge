//! The `pane.agent_status_changed` hook: auto-notify the webhook when an agent settles (done or blocked).
use std::path::Path;

use serde::Deserialize;

use super::{Plugin, log};
use crate::auto::{Decision, HerdrStatus, Settled, Trigger, decide};
use crate::config::{Auto, Config, default_config_path, load_config, read_config};
use crate::env::System;
use crate::error::Result;
use crate::herdr::{AgentInfo, Sender, herdr_agent, in_scope};
use crate::state::{now_ms, update_state};
use crate::webhook::NotifyStatus;

#[derive(Debug, Deserialize)]
struct StatusEvent {
    data: StatusEventData,
}

#[derive(Debug, Deserialize)]
struct StatusEventData {
    pane_id: String,
    agent_status: HerdrStatus,
}

impl Plugin<'_> {
    pub(super) fn on_status_changed(&self) -> Result<()> {
        let Some(event) = self.json_env::<StatusEvent>("HERDR_PLUGIN_EVENT_JSON") else {
            log("ignoring event without pane_id/agent_status");
            return Ok(());
        };
        let StatusEventData {
            pane_id,
            agent_status: status,
        } = event.data;
        let settles = matches!(status, HerdrStatus::Done | HerdrStatus::Blocked);
        let path = default_config_path(self.env);
        // A missing or broken config only matters once there is something to send; that load reports it.
        let config = if settles {
            read_config(&path).ok().flatten()
        } else {
            None
        };
        let rules = config.as_ref().map_or_else(Auto::default, |config| config.auto);
        let agent = if settles {
            herdr_agent(&pane_id, self.env, &self.home, &System)
        } else {
            None
        };
        let focused = agent.as_ref().is_some_and(|agent| agent.focused);
        let now = now_ms();
        let decision = update_state(self.env, |state| decide(state, &pane_id, status, now, focused, rules))?;
        let settled = match decision {
            Decision::Skip(reason) => {
                if settles {
                    log(&format!("{pane_id} {}: skipped ({reason})", status.as_str()));
                }
                return Ok(());
            }
            Decision::Notify(settled) => settled,
        };
        let sent = self.notify_settled(&path, config, agent, &pane_id, settled);
        if !matches!(sent, Ok(true)) {
            // Nothing reached the webhook: give back the cooldown `decide` reserved, so the next event can notify.
            update_state(self.env, |state| {
                if let Some(pane) = state.panes.get_mut(&pane_id)
                    && pane.last_auto_at == Some(now)
                {
                    pane.last_auto_at = None;
                }
            })?;
        }
        sent.map(|_| ())
    }

    /// Send the notification `decide` asked for; `false` when the pane is outside nudge's scope.
    fn notify_settled(
        &self,
        path: &Path,
        config: Option<Config>,
        agent: Option<AgentInfo>,
        pane_id: &str,
        settled: Settled,
    ) -> Result<bool> {
        let config = match config {
            Some(config) => config,
            None => load_config(path)?,
        };
        let sender = agent.map_or_else(
            || Sender {
                agent: None,
                workspace_id: None,
                pane_id: Some(pane_id.to_owned()),
            },
            |agent| agent.sender,
        );
        let (notify_status, status, done) = match settled {
            Settled::Done => (NotifyStatus::Done, "done", "finished a turn"),
            Settled::Blocked => (NotifyStatus::Blocked, "blocked", "is blocked"),
        };
        if !in_scope(&config.scope, &sender, self.env, &System) {
            log(&format!("{pane_id} {status}: skipped (outside nudge's scope)"));
            return Ok(false);
        }
        let sent = self.send_pane(&config, pane_id, notify_status, Trigger::Settled(settled))?;
        log(&format!(
            "{pane_id} {status}: notified webhook (HTTP {})",
            sent.http_status
        ));
        if config.auto.toast {
            self.toast(&format!("Told the webhook: {} {done}", sent.target));
        }
        Ok(true)
    }
}
