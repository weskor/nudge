//! herdr as nudge sees it: the herdr TUI as an agent source (the agent in a pane, its output, its workspace, toasts
//! and the reply hint), who a CLI notification comes from (and the names and ids nudge's scope and routes match it
//! by), the plugin's install state and logs, and the `~/.local/bin/nudge` link.
mod agents;
mod ids;
mod link;
mod logs;
mod query;
mod registration;
mod reply;
mod root;
mod sender;
mod tui;
mod workspaces;

pub(crate) use agents::{ListedAgent, herdr_agents};
pub(crate) use ids::SenderIds;
pub(crate) use link::{command_link, link_command, refresh_command_link};
pub(crate) use logs::plugin_logs;
#[cfg(test)]
pub(crate) use registration::PluginInfo;
pub(crate) use registration::{PLUGIN_ID, Plugin, plugin};
pub(crate) use reply::reply_hint;
pub(crate) use root::nudge_root;
pub(crate) use sender::{Sender, detect_sender};
pub(crate) use tui::TuiSource;
pub(crate) use workspaces::{ListedWorkspace, herdr_workspaces, pane_workspace, workspace_label};
