//! The herdr TUI as an [`AgentSource`]: the agent in a pane, its terminal tail, the reply hint, its workspace
//! and toasts, all through the herdr CLI.
use std::path::Path;

use super::agents::{HerdrAgent, agent_in_pane};
use super::reply::reply_hint;
use super::workspaces::{pane_workspace, workspace_label};
use crate::env::{Env, Run};
use crate::source::{AgentRef, AgentSource, WorkspaceRef};

/// The herdr TUI, reached through the herdr CLI from `home`.
#[derive(Clone, Copy)]
pub(crate) struct TuiSource<'a> {
    pub(crate) env: &'a Env,
    pub(crate) home: &'a Path,
    pub(crate) run: &'a dyn Run,
}

impl TuiSource<'_> {
    /// The agent in `pane_id` as herdr knows it, and whether its pane is focused; `None` when there's no agent there
    /// or herdr can't be reached.
    pub(crate) fn agent(&self, pane_id: &str) -> Option<(AgentRef, bool)> {
        agent_in_pane(pane_id, self.env, self.home, self.run).map(HerdrAgent::into_agent_ref)
    }
}

impl AgentSource for TuiSource<'_> {
    fn tail(&self, agent: &AgentRef, lines: usize) -> Option<String> {
        let lines = lines.to_string();
        let read = [
            self.env.herdr(),
            "pane",
            "read",
            &agent.pane,
            "--source",
            "recent-unwrapped",
            "--lines",
            &lines,
            "--format",
            "text",
        ];
        self.run.run(&read, self.home)
    }

    fn reply_hint(&self, agent: &AgentRef) -> String {
        reply_hint(Some(agent.target()))
    }

    /// herdr's workspace for the agent (from its pane when `agent get` didn't say), with its label.
    fn workspace(&self, agent: &AgentRef) -> Option<WorkspaceRef> {
        let root = Path::new("/");
        let id = match &agent.workspace {
            Some(workspace) => workspace.id.clone(),
            None => pane_workspace(&agent.pane, self.env, root, self.run)?,
        };
        let name = workspace_label(&id, self.env, root, self.run);
        Some(WorkspaceRef { id, name })
    }

    fn toast(&self, text: &str) {
        let _shown = self.run.run(
            &[self.env.herdr(), "notification", "show", "nudge", "--body", text],
            self.home,
        );
    }
}

#[cfg(test)]
mod tests;
