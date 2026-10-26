//! Whether a sender may notify under nudge's `[scope]`, asking herdr for its workspace only when needed.
use std::path::Path;

use super::sender::Sender;
use super::workspaces::{pane_workspace, workspace_label};
use crate::config::Scope;
use crate::env::{Env, Run};

/// Whether `sender` may notify under `scope`. Only when the scope names workspaces does it ask herdr for the
/// sender's workspace (from its pane, if not known yet) and that workspace's label.
pub(crate) fn in_scope(scope: &Scope, sender: &Sender, env: &Env, run: &dyn Run) -> bool {
    if scope.is_everyone() {
        return true;
    }
    let root = Path::new("/");
    let (workspace_id, label) = if scope.workspaces.is_empty() {
        (None, None)
    } else {
        let id = sender.workspace_id.clone().or_else(|| {
            sender
                .pane_id
                .as_deref()
                .and_then(|pane| pane_workspace(pane, env, root, run))
        });
        let label = id.as_deref().and_then(|id| workspace_label(id, env, root, run));
        (id, label)
    };
    let agent_ids: Vec<&str> = [&sender.agent, &sender.pane_id]
        .into_iter()
        .flatten()
        .map(String::as_str)
        .collect();
    let workspace_ids: Vec<&str> = [&workspace_id, &label]
        .into_iter()
        .flatten()
        .map(String::as_str)
        .collect();
    scope.allows(&agent_ids, &workspace_ids)
}

#[cfg(test)]
mod tests;
