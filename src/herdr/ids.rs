//! Every name and id a sender is known by, for nudge's `[scope]` and `[[route]]`, asking herdr for its workspace
//! only when a check needs it.
use std::path::Path;

use super::sender::Sender;
use super::workspaces::{pane_workspace, workspace_label};
use crate::config::{Backend, Route, Scope, resolve};
use crate::env::{Env, Run};

/// A herdr TUI sender's names and ids.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct SenderIds {
    /// Its agent name and pane id.
    panes: Vec<String>,
    /// Its workspace id and label; empty unless looked up.
    workspaces: Vec<String>,
}

impl SenderIds {
    /// `sender`'s agent name and pane id. With `workspace`, also its workspace id (from herdr, through its pane, if
    /// not known yet) and that workspace's label.
    pub(crate) fn lookup(sender: &Sender, workspace: bool, env: &Env, run: &dyn Run) -> Self {
        let panes = [&sender.agent, &sender.pane_id]
            .into_iter()
            .flatten()
            .cloned()
            .collect();
        if !workspace {
            return Self {
                panes,
                workspaces: Vec::new(),
            };
        }
        let root = Path::new("/");
        let id = sender.workspace_id.clone().or_else(|| {
            sender
                .pane_id
                .as_deref()
                .and_then(|pane| pane_workspace(pane, env, root, run))
        });
        let label = id.as_deref().and_then(|id| workspace_label(id, env, root, run));
        Self {
            panes,
            workspaces: [id, label].into_iter().flatten().collect(),
        }
    }

    /// Whether this sender may notify under `scope`. Looked up without its workspace, it's only in a scope that
    /// lists it by name or pane id.
    pub(crate) fn in_scope(&self, scope: &Scope) -> bool {
        scope.allows(&strs(&self.panes), &strs(&self.workspaces))
    }

    /// The route its notifications take; `None` means the default webhook. Looked up without its workspace, only
    /// pane routes can match.
    pub(crate) fn route<'a>(&self, routes: &'a [Route]) -> Option<&'a Route> {
        resolve(routes, Backend::Tui, &strs(&self.panes), &strs(&self.workspaces))
    }
}

fn strs(names: &[String]) -> Vec<&str> {
    names.iter().map(String::as_str).collect()
}

#[cfg(test)]
mod tests;
