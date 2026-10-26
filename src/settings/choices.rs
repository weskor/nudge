//! The workspaces and agents that can be ticked for `[scope]`: those open in herdr, with their details, and the
//! names only the config still mentions.
use std::ffi::OsStr;
use std::path::Path;

use crate::herdr::{ListedAgent, ListedWorkspace};

/// A workspace or agent that can be ticked for `[scope]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Choice {
    /// What is stored in `[scope]`.
    pub(super) name: String,
    pub(super) checked: bool,
    /// Open in herdr right now; `false` for names only the config mentions.
    pub(super) live: bool,
    /// Shown dimmed after the name: the agents a workspace holds, where an agent runs, or `(not open)`.
    pub(super) detail: String,
}

/// A workspace or agent open in herdr: how it is shown and stored, every name `[scope]` may know it by, and its
/// detail.
struct Open {
    name: String,
    ids: Vec<String>,
    detail: String,
}

/// Open ones first (ticked if `[scope]` names them by any of their `ids`), then names only the config has.
fn choices(open: Vec<Open>, saved: &[String]) -> Vec<Choice> {
    let mut list: Vec<Choice> = Vec::new();
    for Open { name, ids, detail } in open {
        if list.iter().any(|choice| choice.name == name) {
            continue;
        }
        let checked = saved.iter().any(|entry| ids.contains(entry));
        list.push(Choice {
            name,
            checked,
            live: true,
            detail,
        });
    }
    for entry in saved {
        if !list.iter().any(|choice| &choice.name == entry) {
            list.push(Choice {
                name: entry.clone(),
                checked: true,
                live: false,
                detail: "(not open)".into(),
            });
        }
    }
    list
}

/// The workspaces to tick: open ones by label (with how many agents each holds), then the `saved` ones not open.
pub(super) fn workspace_choices(
    saved: &[String],
    workspaces: &[ListedWorkspace],
    agents: &[ListedAgent],
) -> Vec<Choice> {
    let open = workspaces
        .iter()
        .map(|workspace| {
            let held = agents
                .iter()
                .filter(|agent| agent.workspace_id.as_deref() == Some(workspace.workspace_id.as_str()))
                .count();
            Open {
                name: workspace.name().to_owned(),
                ids: vec![workspace.name().to_owned(), workspace.workspace_id.clone()],
                detail: match held {
                    0 => String::new(),
                    1 => "1 agent".into(),
                    many => format!("{many} agents"),
                },
            }
        })
        .collect();
    let mut list = choices(open, saved);
    // A saved workspace id that matched a live workspace is already shown under its name.
    list.retain(|choice| choice.live || !workspaces.iter().any(|workspace| workspace.workspace_id == choice.name));
    list
}

/// The agents to tick: open ones (with their workspace and directory), then the `saved` ones not open.
pub(super) fn agent_choices(saved: &[String], workspaces: &[ListedWorkspace], agents: &[ListedAgent]) -> Vec<Choice> {
    let open = agents
        .iter()
        .map(|agent| {
            let workspace = agent.workspace_id.as_deref().map(|id| {
                workspaces
                    .iter()
                    .find(|workspace| workspace.workspace_id == id)
                    .map_or(id, ListedWorkspace::name)
            });
            let dir = agent
                .cwd
                .as_deref()
                .and_then(|cwd| Path::new(cwd).file_name())
                .and_then(OsStr::to_str);
            Open {
                name: agent.name.clone(),
                ids: vec![agent.name.clone()],
                detail: workspace.into_iter().chain(dir).collect::<Vec<_>>().join(" · "),
            }
        })
        .collect();
    choices(open, saved)
}

#[cfg(test)]
mod tests;
