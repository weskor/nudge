//! Sending one agent's terminal output to its webhook: the route, the tail, the message, the payload and delivery.
use super::Context;
use crate::auto::{Trigger, auto_message};
use crate::config::{Config, find_webhook, routes_name_workspaces};
use crate::error::Result;
use crate::source::{AgentRef, AgentSource};
use crate::webhook::{Author, NotifyStatus, Origin, build_notification, deliver};

/// Lines read from the terminal: more than are sent, so redaction sees a secret whose start scrolled above the cut.
const READ_LINES: usize = 200;

/// What the webhook accepted from [`send`].
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Sent {
    pub(crate) http_status: u16,
    /// The agent the webhook's automation will reply to: its name, else its pane.
    pub(crate) target: String,
}

/// `agent`, with its workspace and the workspace's name looked up when the config matches by workspace: its
/// routes, or (when `scoped`) its scope. The lookup can cost backend calls, so it happens only then.
pub(crate) fn locate(mut agent: AgentRef, config: &Config, scoped: bool, source: &dyn AgentSource) -> AgentRef {
    let needed =
        (scoped && !config.scope.workspaces.is_empty()) || routes_name_workspaces(&config.route, agent.backend);
    if needed {
        agent.workspace = source.workspace(&agent);
    }
    agent
}

/// Send `agent`'s recent output to its webhook (its route's, else the default) as `status`. `agent` comes from
/// [`locate`], so workspace routes can match.
pub(crate) fn send(
    agent: &AgentRef,
    status: NotifyStatus,
    trigger: Trigger,
    source: &dyn AgentSource,
    config: &Config,
    ctx: &Context<'_>,
) -> Result<Sent> {
    let webhook = find_webhook(config, agent.route(&config.route).map(|route| route.webhook.as_str()))?;
    let raw = source.tail(agent, READ_LINES);
    let lines = usize::try_from(config.auto.tail_lines).unwrap_or(usize::MAX);
    let message = auto_message(trigger, raw.as_deref().unwrap_or_default(), lines);
    let target = agent.target().to_owned();
    let cwd = agent
        .cwd
        .clone()
        .unwrap_or_else(|| ctx.home.to_string_lossy().into_owned());
    let author = Author {
        agent: Some(target.clone()),
        reply_hint: source.reply_hint(agent),
        backend: agent.backend,
        pane: Some(agent.pane.clone()),
    };
    let origin = Origin {
        env: ctx.env,
        cwd: &cwd,
        run: ctx.run,
    };
    let notification = build_notification(status.into(), &message, author, &webhook.name, origin)?;
    let http_status = deliver(ctx.env, webhook, &notification)?;
    Ok(Sent { http_status, target })
}
