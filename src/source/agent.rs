//! An agent as the pipeline sees it (`AgentRef`): who it is, where it runs, and what `[scope]`, `[[route]]` and
//! state.json know it by.
use super::backend::Backend;
use crate::config::{Route, Scope, resolve};
use crate::state::tui_key;

/// A workspace an agent runs in: its id, and its name when known.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WorkspaceRef {
    pub(crate) id: String,
    pub(crate) name: Option<String>,
}

/// An agent as the pipeline sees it. Built once per event at the backend's boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AgentRef {
    pub(crate) backend: Backend,
    /// The herdr TUI pane id (`w1:p3`).
    pub(crate) pane: String,
    /// The agent's name, if it has one.
    pub(crate) name: Option<String>,
    /// Its workspace, when known; the name is filled in on demand (see `AgentSource::workspace`).
    pub(crate) workspace: Option<WorkspaceRef>,
    /// Its working directory, when known.
    pub(crate) cwd: Option<String>,
}

impl AgentRef {
    /// An agent in `pane` that nothing else is known about yet.
    pub(crate) const fn in_pane(backend: Backend, pane: String) -> Self {
        Self {
            backend,
            pane,
            name: None,
            workspace: None,
            cwd: None,
        }
    }

    /// Its key in state.json: `tui:w1:p3`.
    pub(crate) fn state_key(&self) -> String {
        match self.backend {
            Backend::Tui => tui_key(&self.pane),
            Backend::Core => format!("core:{}", self.pane),
        }
    }

    /// The reply target the webhook's automation should use: its name, else its pane.
    pub(crate) fn target(&self) -> &str {
        self.name.as_deref().unwrap_or(&self.pane)
    }

    /// Its name and pane id.
    fn names(&self) -> Vec<&str> {
        self.name.iter().chain([&self.pane]).map(String::as_str).collect()
    }

    /// Its workspace's id and name, when known.
    fn workspace_names(&self) -> Vec<&str> {
        self.workspace
            .iter()
            .flat_map(|workspace| [Some(&workspace.id), workspace.name.as_ref()])
            .flatten()
            .map(String::as_str)
            .collect()
    }

    /// Whether it may notify under `scope`. Without its workspace's name, only a scope naming the workspace id (or
    /// the agent) lets it through.
    pub(crate) fn in_scope(&self, scope: &Scope) -> bool {
        scope.allows(&self.names(), &self.workspace_names())
    }

    /// The route its notifications take; `None` means the default webhook.
    pub(crate) fn route<'a>(&self, routes: &'a [Route]) -> Option<&'a Route> {
        resolve(routes, self.backend, &self.names(), &self.workspace_names())
    }
}

#[cfg(test)]
mod tests;
