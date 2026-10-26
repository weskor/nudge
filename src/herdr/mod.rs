//! herdr as nudge sees it: the agent and workspace a notification comes from, nudge's scope, the plugin's install
//! state and logs, and the `~/.local/bin/nudge` link.
mod detect;
mod install;

pub(crate) use detect::{
    AgentInfo, ListedAgent, ListedWorkspace, Sender, detect_sender, herdr_agent, herdr_agents, herdr_workspaces,
    in_scope, pane_workspace, workspace_label,
};
#[cfg(test)]
pub(crate) use install::PluginInfo;
pub(crate) use install::{
    PLUGIN_ID, Plugin, command_link, link_command, nudge_root, plugin, plugin_logs, refresh_command_link,
};
