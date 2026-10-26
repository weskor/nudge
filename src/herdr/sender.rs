//! Who a notification comes from: the herdr agent to reply to, detected from the environment and herdr.
use std::path::Path;

use super::agents::{AgentList, HerdrAgent, agent_in_pane};
use super::query::herdr_json;
use crate::env::{Env, Run};

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

#[cfg(test)]
mod tests;
