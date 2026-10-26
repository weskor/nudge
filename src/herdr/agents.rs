//! herdr's agents: the one in a pane (`herdr agent get`) and all running ones (`herdr agent list`).
use std::path::Path;

use serde::Deserialize;

use super::query::herdr_json;
use super::sender::Sender;
use crate::env::{Env, Run};

#[derive(Debug, Deserialize)]
pub(super) struct HerdrAgent {
    name: Option<String>,
    pane_id: String,
    workspace_id: Option<String>,
    pub(super) cwd: Option<String>,
    pub(super) foreground_cwd: Option<String>,
    focused: Option<bool>,
}

impl HerdrAgent {
    /// What `herdr agent prompt` accepts for this agent: its name, else its pane id.
    fn target(&self) -> String {
        self.name
            .clone()
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| self.pane_id.clone())
    }

    /// The sender this agent is. Its workspace is herdr's answer only: the environment describes nudge's own
    /// pane, which may not be this one.
    pub(super) fn sender(self) -> Sender {
        Sender {
            agent: Some(self.target()),
            workspace_id: self.workspace_id,
            pane_id: Some(self.pane_id),
        }
    }
}

#[derive(Debug, Deserialize)]
struct AgentGet {
    result: AgentGetResult,
}

#[derive(Debug, Deserialize)]
struct AgentGetResult {
    agent: HerdrAgent,
}

#[derive(Debug, Deserialize)]
pub(super) struct AgentList {
    pub(super) result: AgentListResult,
}

#[derive(Debug, Deserialize)]
pub(super) struct AgentListResult {
    pub(super) agents: Vec<HerdrAgent>,
}

pub(super) fn agent_in_pane(pane_id: &str, env: &Env, cwd: &Path, run: &dyn Run) -> Option<HerdrAgent> {
    herdr_json::<AgentGet>(&["agent", "get", pane_id], env, cwd, run).map(|got| got.result.agent)
}

/// The herdr agent in a pane, as the plugin needs it.
#[derive(Debug)]
pub(crate) struct AgentInfo {
    pub(crate) sender: Sender,
    /// The agent's working directory, when herdr knows it.
    pub(crate) cwd: Option<String>,
    /// Whether the pane is focused in the herdr client.
    pub(crate) focused: bool,
}

/// Look up the herdr agent in `pane_id`; `None` if herdr is unavailable or there is no agent there.
pub(crate) fn herdr_agent(pane_id: &str, env: &Env, cwd: &Path, run: &dyn Run) -> Option<AgentInfo> {
    let mut agent = agent_in_pane(pane_id, env, cwd, run)?;
    let agent_cwd = agent
        .foreground_cwd
        .take()
        .filter(|dir| !dir.is_empty())
        .or_else(|| agent.cwd.take());
    Some(AgentInfo {
        cwd: agent_cwd.filter(|dir| !dir.is_empty()),
        focused: agent.focused == Some(true),
        sender: agent.sender(),
    })
}

/// A running agent from `herdr agent list`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ListedAgent {
    /// Its `herdr agent prompt` target: its name, else its pane id.
    pub(crate) name: String,
    pub(crate) workspace_id: Option<String>,
    pub(crate) cwd: Option<String>,
}

/// All running agents; empty if herdr is unavailable.
pub(crate) fn herdr_agents(env: &Env, run: &dyn Run) -> Vec<ListedAgent> {
    herdr_json::<AgentList>(&["agent", "list"], env, Path::new("/"), run)
        .map(|list| {
            list.result
                .agents
                .into_iter()
                .map(|agent| ListedAgent {
                    name: agent.target(),
                    workspace_id: agent.workspace_id,
                    cwd: agent.cwd.or(agent.foreground_cwd),
                })
                .collect()
        })
        .unwrap_or_default()
}
