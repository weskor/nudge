//! herdr as nudge sees it: the agent and workspace a notification comes from, nudge's scope, the plugin's install
//! state and logs, and the `~/.local/bin/nudge` link.
mod agents;
mod link;
mod logs;
mod query;
mod registration;
mod root;
mod scope;
mod sender;
mod workspaces;

pub(crate) use agents::{AgentInfo, ListedAgent, herdr_agent, herdr_agents};
pub(crate) use link::{command_link, link_command, refresh_command_link};
pub(crate) use logs::plugin_logs;
#[cfg(test)]
pub(crate) use registration::PluginInfo;
pub(crate) use registration::{PLUGIN_ID, Plugin, plugin};
pub(crate) use root::nudge_root;
pub(crate) use scope::in_scope;
pub(crate) use sender::{Sender, detect_sender};
pub(crate) use workspaces::{ListedWorkspace, herdr_workspaces, pane_workspace, workspace_label};
