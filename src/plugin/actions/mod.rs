//! `nudge plugin action <id>`: read herdr's action context and dispatch to the action.
mod panes;
mod scope;
mod switches;

use serde::Deserialize;

use super::Plugin;
use crate::config::Scope;
use crate::error::{Error, Result};

#[derive(Debug, Default, Deserialize)]
struct ActionContext {
    focused_pane_id: Option<String>,
    focused_pane_status: Option<String>,
}

impl Plugin<'_> {
    pub(super) fn on_action(&self, action: &str) -> Result<()> {
        let context = self
            .json_env::<ActionContext>("HERDR_PLUGIN_CONTEXT_JSON")
            .unwrap_or_default();
        let pane_id = context
            .focused_pane_id
            .or_else(|| self.env.get("HERDR_PANE_ID").map(str::to_owned))
            .filter(|id| !id.is_empty());
        match action {
            "enable" | "disable" | "toggle" => self.switch_auto(action),
            "away" | "away-on" | "away-off" => self.switch_away(action),
            "send-pane" => self.send_focused_pane(pane_id, context.focused_pane_status.as_deref()),
            "setup" | "status" | "settings" => self.open_pane(action),
            "only-workspace" => self.only_workspace(pane_id),
            "only-agent" => self.only_agent(pane_id),
            "all-agents" => self.set_scope(Scope::default(), "All agents can notify the webhook"),
            other => Err(Error::Other(format!("unknown action \"{other}\""))),
        }
    }
}
