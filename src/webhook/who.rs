//! Who a CLI or MCP notification is from (a herdr TUI sender, or the herdr-core terminal `nudge notify` runs in),
//! checked against `[scope]`, with the route it takes and how the payload names it.
use super::origin::Origin;
use super::payload::Author;
use crate::config::{Config, routes_name_workspaces};
use crate::core::{core_reply_hint, core_sender};
use crate::error::{Error, Result};
use crate::herdr::{Sender, SenderIds, detect_sender, reply_hint};
use crate::source::{AgentRef, Backend};
use crate::state::tui_key;

/// Who is notifying.
#[derive(Debug)]
pub(super) enum Who {
    Tui(Sender),
    Core(AgentRef),
}

/// A sender that passed the scope check.
#[derive(Debug)]
pub(super) struct Checked {
    /// The webhook its route names, when routes were asked for and one matches.
    pub(super) route: Option<String>,
    /// Its key in state.json, to record that it notified; `None` outside a herdr pane or core terminal.
    pub(super) state_key: Option<String>,
}

/// Who is sending. Inside a core terminal (and not a herdr pane): that terminal, named `agent` or `$NUDGE_AGENT`
/// if given. Else the herdr sender: `agent` if named explicitly, else detected from herdr.
pub(super) fn who(agent: Option<String>, origin: Origin<'_>) -> Who {
    if let Some(mut terminal) = core_sender(origin.env, origin.run) {
        if let Some(name) = agent.or_else(|| origin.env.get("NUDGE_AGENT").map(str::to_owned)) {
            terminal.name = Some(name);
        }
        return Who::Core(terminal);
    }
    Who::Tui(agent.map_or_else(
        || detect_sender(origin.env, origin.cwd, origin.run),
        |agent| Sender::named(agent, origin.env),
    ))
}

impl Who {
    /// How the payload names the sender and says how to reply.
    pub(super) fn author(&self) -> Author {
        match self {
            Self::Tui(sender) => Author {
                agent: sender.agent.clone(),
                reply_hint: reply_hint(sender.agent.as_deref()),
                backend: Backend::Tui,
                pane: sender.pane_id.clone(),
            },
            Self::Core(terminal) => Author {
                agent: Some(terminal.target().to_owned()),
                reply_hint: core_reply_hint(&terminal.pane),
                backend: Backend::Core,
                pane: Some(terminal.pane.clone()),
            },
        }
    }

    /// Refuse a sender outside `[scope]`; else its route (when `routed`) and state key. A herdr sender's workspace
    /// is looked up only when the scope or a route needs it.
    pub(super) fn check(&self, config: &Config, routed: bool, origin: Origin<'_>) -> Result<Checked> {
        match self {
            Self::Tui(sender) => {
                let workspace = !config.scope.workspaces.is_empty()
                    || (routed && routes_name_workspaces(&config.route, Backend::Tui));
                let ids = SenderIds::lookup(sender, workspace, origin.env, origin.run);
                if !ids.in_scope(&config.scope) {
                    return Err(out_of_scope(sender.agent.as_deref(), sender.workspace_id.as_deref()));
                }
                Ok(Checked {
                    route: routed
                        .then(|| ids.route(&config.route).map(|route| route.webhook.clone()))
                        .flatten(),
                    state_key: origin.env.get("HERDR_PANE_ID").map(tui_key),
                })
            }
            Self::Core(terminal) => {
                if !terminal.in_scope(&config.scope) {
                    let space = terminal.workspace.as_ref().map(|space| space.id.as_str());
                    return Err(out_of_scope(Some(terminal.target()), space));
                }
                Ok(Checked {
                    route: routed
                        .then(|| terminal.route(&config.route).map(|route| route.webhook.clone()))
                        .flatten(),
                    state_key: Some(terminal.state_key()),
                })
            }
        }
    }
}

/// The refusal for a sender outside nudge's `[scope]`.
fn out_of_scope(agent: Option<&str>, workspace: Option<&str>) -> Error {
    let who = agent.map_or_else(|| "this agent".to_owned(), |agent| format!("agent \"{agent}\""));
    let place = workspace
        .map(|id| format!(" in workspace \"{id}\""))
        .unwrap_or_default();
    Error::Config(format!(
        "{who}{place} is outside nudge's scope, so nothing was sent. Change [scope] in the nudge config, \
         or run the herdr action \"Nudge: notify from all agents\"."
    ))
}
