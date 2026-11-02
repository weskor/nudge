//! Which agent status changes become webhook notifications: turn tracking, auto-notify, focus, cooldown.
use crate::config::Auto;
use crate::source::{AgentStatus, Transition};
use crate::state::{PaneState, State};

/// A status that ends a turn and can be worth telling the webhook about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Settled {
    Done,
    Blocked,
    /// The program reported an error: notified like `Blocked`, with its own lead line.
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Decision {
    Notify(Settled),
    Skip(String),
}

/// Apply one status change to `state` and decide whether to notify.
/// - `working` starts a turn; `idle` (seen by you) and `done` end it. Turns are tracked even when nothing is sent.
/// - `blocked` always qualifies; `done` only after a turn of at least `rules.min_turn_seconds` with a known start.
/// - Skipped when nudge is off (`nudge off`), auto-notify is off, you are looking at the pane (`watched`), the agent
///   already ran `nudge notify` this turn, or the pane is in its `rules.cooldown_seconds` cooldown.
/// - Away mode (`state.away`) overrides auto-notify being off and the focus skip: you said you're away.
///
/// The agent's entry in `state.panes` is its [`crate::source::AgentRef::state_key`].
pub(crate) fn decide(state: &mut State, transition: &Transition, rules: Auto) -> Decision {
    let now = transition.at_ms;
    let pane = state.panes.entry(transition.agent.state_key()).or_default();
    let turn_start = pane.working_since;
    let settled = match track_turn(pane, transition.status, now) {
        Ok(settled) => settled,
        Err(skip) => return skip,
    };
    if !state.on {
        return Decision::Skip("nudge is off".into());
    }
    if !state.auto && !state.away {
        return Decision::Skip("auto-notify is off".into());
    }
    if transition.watched && !state.away {
        return Decision::Skip("you are looking at this pane".into());
    }
    let self_notified_since = turn_start.unwrap_or(now - rules.min_turn_ms());
    if pane.self_notified_at.is_some_and(|at| at >= self_notified_since) {
        return Decision::Skip("agent already notified the webhook this turn".into());
    }
    if settled == Settled::Done {
        let Some(start) = turn_start else {
            return Decision::Skip("turn start unknown".into());
        };
        if now - start < rules.min_turn_ms() {
            return Decision::Skip(format!("turn shorter than {}s", rules.min_turn_seconds));
        }
    }
    if pane.last_auto_at.is_some_and(|at| now - at < rules.cooldown_ms()) {
        return Decision::Skip("cooldown".into());
    }
    pane.last_auto_at = Some(now);
    Decision::Notify(settled)
}

/// Record the turn `status` starts or ends; the settled status, or why it doesn't settle anything.
fn track_turn(pane: &mut PaneState, status: AgentStatus, now: i64) -> Result<Settled, Decision> {
    match status {
        AgentStatus::Working => {
            pane.working_since.get_or_insert(now);
            Err(Decision::Skip("turn started".into()))
        }
        AgentStatus::Idle | AgentStatus::Unknown => {
            pane.working_since = None;
            Err(Decision::Skip(format!("status {}", status.as_str())))
        }
        AgentStatus::Done => {
            pane.working_since = None;
            Ok(Settled::Done)
        }
        AgentStatus::Blocked => Ok(Settled::Blocked),
        AgentStatus::Error => Ok(Settled::Error),
    }
}

#[cfg(test)]
mod tests;
