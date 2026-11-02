//! The one place a status change becomes a webhook request: decide, check the scope, send, and give back the
//! cooldown when nothing was sent.
use super::Context;
use super::send::{Sent, locate, send};
use crate::auto::{Decision, Settled, Trigger, decide};
use crate::config::{Auto, Config, load_config};
use crate::error::{Error, Result};
use crate::source::{AgentRef, AgentSource, Transition};
use crate::state::update_state;
use crate::webhook::NotifyStatus;

/// What became of a status change.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// Nothing was sent, for this reason.
    Skipped(String),
    Sent(Sent),
}

/// Apply `transition` to the saved state and, when the rules say so, notify the agent's webhook. `config` is the
/// config if it was read already (its `[auto]` rules apply); otherwise it's loaded once there's something to send,
/// so a missing or broken config only matters then.
pub(crate) fn handle(
    transition: Transition,
    source: &dyn AgentSource,
    ctx: &Context<'_>,
    config: Option<Config>,
) -> Result<Outcome> {
    let rules = config.as_ref().map_or_else(Auto::default, |config| config.auto);
    let decision = update_state(ctx.env, |state| decide(state, &transition, rules))?;
    let settled = match decision {
        Decision::Skip(reason) => return Ok(Outcome::Skipped(reason)),
        Decision::Notify(settled) => settled,
    };
    let key = transition.agent.state_key();
    let outcome = notify(transition.agent, settled, source, ctx, config);
    if !matches!(outcome, Ok(Outcome::Sent(_))) {
        // Nothing reached the webhook: give back the cooldown `decide` reserved, so the next event can notify.
        update_state(ctx.env, |state| {
            if let Some(pane) = state.panes.get_mut(&key)
                && pane.last_auto_at == Some(transition.at_ms)
            {
                pane.last_auto_at = None;
            }
        })?;
    }
    match outcome {
        // Turned off between deciding and sending: nobody asked for this notification, so it's a skip.
        Err(Error::Off) => Ok(Outcome::Skipped("nudge is off".into())),
        other => other,
    }
}

/// Send the notification `decide` asked for, unless the agent is outside the scope.
fn notify(
    agent: AgentRef,
    settled: Settled,
    source: &dyn AgentSource,
    ctx: &Context<'_>,
    config: Option<Config>,
) -> Result<Outcome> {
    let config = match config {
        Some(config) => config,
        None => load_config(ctx.config_path)?,
    };
    let agent = locate(agent, &config, true, source);
    if !agent.in_scope(&config.scope) {
        return Ok(Outcome::Skipped("outside nudge's scope".into()));
    }
    let (status, done) = match settled {
        Settled::Done => (NotifyStatus::Done, "finished a turn"),
        Settled::Blocked => (NotifyStatus::Blocked, "is blocked"),
        Settled::Error => (NotifyStatus::Blocked, "reported an error"),
    };
    let sent = send(&agent, status, Trigger::Settled(settled), source, &config, ctx)?;
    if config.auto.toast {
        source.toast(&format!("Told the webhook: {} {done}", sent.target));
    }
    Ok(Outcome::Sent(sent))
}

#[cfg(test)]
mod tests;
