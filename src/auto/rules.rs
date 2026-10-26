//! Which agent status changes become webhook notifications: turn tracking, auto-notify, focus, cooldown.
use serde::Deserialize;

use crate::config::Auto;
use crate::state::State;

/// An agent status as herdr reports it in `pane.agent_status_changed`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum HerdrStatus {
    Idle,
    Working,
    Blocked,
    Done,
    Unknown,
}

impl HerdrStatus {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Working => "working",
            Self::Blocked => "blocked",
            Self::Done => "done",
            Self::Unknown => "unknown",
        }
    }
}

/// A status that ends a turn and can be worth telling the webhook about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Settled {
    Done,
    Blocked,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Decision {
    Notify(Settled),
    Skip(String),
}

/// Apply one `pane.agent_status_changed` event to `state` and decide whether to notify.
/// - `working` starts a turn; `idle` (seen by you) and `done` end it.
/// - `blocked` always qualifies; `done` only after a turn of at least `rules.min_turn_seconds` with a known start.
/// - Skipped when auto-notify is off, you are looking at the pane (`focused`), the agent already ran
///   `nudge notify` this turn, or the pane is in its `rules.cooldown_seconds` cooldown.
/// - Away mode (`state.away`) overrides the first two: you said you're away.
pub(crate) fn decide(
    state: &mut State,
    pane_id: &str,
    status: HerdrStatus,
    now: i64,
    focused: bool,
    rules: Auto,
) -> Decision {
    let pane = state.panes.entry(pane_id.to_owned()).or_default();
    let turn_start = pane.working_since;
    let settled = match status {
        HerdrStatus::Working => {
            pane.working_since.get_or_insert(now);
            return Decision::Skip("turn started".into());
        }
        HerdrStatus::Idle | HerdrStatus::Unknown => {
            pane.working_since = None;
            return Decision::Skip(format!("status {}", status.as_str()));
        }
        HerdrStatus::Done => {
            pane.working_since = None;
            Settled::Done
        }
        HerdrStatus::Blocked => Settled::Blocked,
    };

    if !state.auto && !state.away {
        return Decision::Skip("auto-notify is off".into());
    }
    if focused && !state.away {
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

#[cfg(test)]
mod tests;
