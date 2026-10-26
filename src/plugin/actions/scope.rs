//! The scope actions: limit who may notify the webhook to the focused pane's workspace (`only-workspace`) or
//! agent (`only-agent`), or to everyone (`all-agents`).
use crate::config::{Scope, default_config_path, load_config, write_config};
use crate::env::System;
use crate::error::Result;
use crate::herdr::{herdr_agent, pane_workspace, workspace_label};
use crate::plugin::Plugin;

impl Plugin<'_> {
    /// Replace the config's `[scope]` and confirm with a toast.
    pub(super) fn set_scope(&self, scope: Scope, confirmation: &str) -> Result<()> {
        let path = default_config_path(self.env);
        let mut config = load_config(&path)?;
        config.scope = scope;
        write_config(&path, &config)?;
        self.toast(confirmation);
        Ok(())
    }

    /// Let only the focused pane's workspace notify, stored by its label (else its id).
    pub(super) fn only_workspace(&self, pane_id: Option<String>) -> Result<()> {
        let Some(workspace_id) = pane_id.and_then(|pane| pane_workspace(&pane, self.env, &self.home, &System)) else {
            self.toast("No workspace for this pane");
            return Ok(());
        };
        let name = workspace_label(&workspace_id, self.env, &self.home, &System).unwrap_or(workspace_id);
        let confirmation = format!("Only workspace \"{name}\" can notify the webhook");
        self.set_scope(
            Scope {
                workspaces: vec![name],
                agents: Vec::new(),
            },
            &confirmation,
        )
    }

    /// Let only the agent in the focused pane notify.
    pub(super) fn only_agent(&self, pane_id: Option<String>) -> Result<()> {
        let agent = pane_id
            .and_then(|pane| herdr_agent(&pane, self.env, &self.home, &System))
            .and_then(|agent| agent.sender.agent);
        let Some(name) = agent else {
            self.toast("No agent in this pane");
            return Ok(());
        };
        let confirmation = format!("Only agent \"{name}\" can notify the webhook");
        self.set_scope(
            Scope {
                workspaces: Vec::new(),
                agents: vec![name],
            },
            &confirmation,
        )
    }
}
