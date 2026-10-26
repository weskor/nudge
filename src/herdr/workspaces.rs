//! herdr's workspaces: the one a pane belongs to, a workspace's label, and the open ones in sidebar order.
use std::path::Path;

use serde::Deserialize;

use super::query::herdr_json;
use crate::env::{Env, Run};

#[derive(Debug, Deserialize)]
struct PaneGet {
    result: PaneGetResult,
}

#[derive(Debug, Deserialize)]
struct PaneGetResult {
    pane: PaneInfo,
}

#[derive(Debug, Deserialize)]
struct PaneInfo {
    workspace_id: String,
}

/// The id of the workspace that `pane_id` belongs to.
pub(crate) fn pane_workspace(pane_id: &str, env: &Env, cwd: &Path, run: &dyn Run) -> Option<String> {
    herdr_json::<PaneGet>(&["pane", "get", pane_id], env, cwd, run).map(|got| got.result.pane.workspace_id)
}

#[derive(Debug, Deserialize)]
struct WorkspaceGet {
    result: WorkspaceGetResult,
}

#[derive(Debug, Deserialize)]
struct WorkspaceGetResult {
    workspace: WorkspaceInfo,
}

#[derive(Debug, Deserialize)]
struct WorkspaceInfo {
    label: Option<String>,
}

/// The label a herdr workspace shows in the sidebar.
pub(crate) fn workspace_label(workspace_id: &str, env: &Env, cwd: &Path, run: &dyn Run) -> Option<String> {
    herdr_json::<WorkspaceGet>(&["workspace", "get", workspace_id], env, cwd, run)
        .and_then(|got| got.result.workspace.label)
        .filter(|label| !label.is_empty())
}

#[derive(Debug, Deserialize)]
struct WorkspaceList {
    result: WorkspaceListResult,
}

#[derive(Debug, Deserialize)]
struct WorkspaceListResult {
    workspaces: Vec<ListedWorkspace>,
}

/// A workspace from `herdr workspace list`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub(crate) struct ListedWorkspace {
    pub(crate) workspace_id: String,
    pub(crate) label: Option<String>,
}

impl ListedWorkspace {
    /// How the workspace is shown and stored in `[scope]`: its label, else its id.
    pub(crate) fn name(&self) -> &str {
        self.label
            .as_deref()
            .filter(|label| !label.is_empty())
            .unwrap_or(&self.workspace_id)
    }
}

/// Open herdr workspaces, in sidebar order; empty if herdr is unavailable.
pub(crate) fn herdr_workspaces(env: &Env, run: &dyn Run) -> Vec<ListedWorkspace> {
    herdr_json::<WorkspaceList>(&["workspace", "list"], env, Path::new("/"), run)
        .map(|list| list.result.workspaces)
        .unwrap_or_default()
}
