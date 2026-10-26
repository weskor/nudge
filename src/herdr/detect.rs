//! The herdr agent a notification comes from, and which agents and workspaces nudge's scope allows.
use std::path::Path;

use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::config::Scope;
use crate::env::{Env, Run};

/// Run `herdr <args>` and parse its JSON output; `None` if herdr fails or answers something unexpected.
pub(crate) fn herdr_json<T: DeserializeOwned>(args: &[&str], env: &Env, cwd: &Path, run: &dyn Run) -> Option<T> {
    let cmd: Vec<&str> = std::iter::once(env.herdr()).chain(args.iter().copied()).collect();
    serde_json::from_str(&run.run(&cmd, cwd)?).ok()
}

#[derive(Debug, Deserialize)]
struct HerdrAgent {
    name: Option<String>,
    pane_id: String,
    workspace_id: Option<String>,
    cwd: Option<String>,
    foreground_cwd: Option<String>,
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
    fn sender(self) -> Sender {
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
struct AgentList {
    result: AgentListResult,
}

#[derive(Debug, Deserialize)]
struct AgentListResult {
    agents: Vec<HerdrAgent>,
}

fn agent_in_pane(pane_id: &str, env: &Env, cwd: &Path, run: &dyn Run) -> Option<HerdrAgent> {
    herdr_json::<AgentGet>(&["agent", "get", pane_id], env, cwd, run).map(|got| got.result.agent)
}

/// Who a notification comes from: the herdr agent to reply to, and where it runs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Sender {
    /// The `herdr agent prompt` target: the agent's name, else its pane id. `None` if unknown.
    pub(crate) agent: Option<String>,
    pub(crate) pane_id: Option<String>,
    pub(crate) workspace_id: Option<String>,
}

impl Sender {
    /// `agent`, named explicitly (`--agent`, `$NUDGE_AGENT`), in this herdr pane if there is one.
    pub(crate) fn named(agent: String, env: &Env) -> Self {
        Self {
            agent: Some(agent),
            pane_id: env.get("HERDR_PANE_ID").map(str::to_owned),
            workspace_id: env.get("HERDR_WORKSPACE_ID").map(str::to_owned),
        }
    }
}

/// Who is sending, in order:
/// 1. `$NUDGE_AGENT`;
/// 2. the agent in this pane (`$HERDR_PANE_ID`): its name, else the pane id;
/// 3. outside herdr panes, the single herdr agent whose cwd is `cwd`: its name, else its pane id.
///
/// The agent is `None` when none applies: a herdr pane without an agent (a plain shell), herdr unreachable,
/// or several agents sharing the cwd. Never guess: the webhook's automation types its reply into this target.
pub(crate) fn detect_sender(env: &Env, cwd: &str, run: &dyn Run) -> Sender {
    if let Some(agent) = env.get("NUDGE_AGENT") {
        return Sender::named(agent.to_owned(), env);
    }
    let unknown = || Sender {
        agent: None,
        pane_id: env.get("HERDR_PANE_ID").map(str::to_owned),
        workspace_id: env.get("HERDR_WORKSPACE_ID").map(str::to_owned),
    };
    if let Some(pane_id) = env.get("HERDR_PANE_ID") {
        return agent_in_pane(pane_id, env, Path::new(cwd), run).map_or_else(unknown, HerdrAgent::sender);
    }
    let Some(list) = herdr_json::<AgentList>(&["agent", "list"], env, Path::new(cwd), run) else {
        return unknown();
    };
    let mut here = list
        .result
        .agents
        .into_iter()
        .filter(|agent| agent.cwd.as_deref() == Some(cwd) || agent.foreground_cwd.as_deref() == Some(cwd));
    match (here.next(), here.next()) {
        (Some(agent), None) => agent.sender(),
        (Some(_), Some(_)) | (None, _) => unknown(),
    }
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
