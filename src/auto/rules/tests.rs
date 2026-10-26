use super::{Decision, HerdrStatus, Settled, decide};
use crate::config::Auto;
use crate::state::{PaneState, State};

const P: &str = "w1:p3";
const T0: i64 = 1_800_000_000_000;

fn fresh() -> State {
    State {
        auto: true,
        ..State::default()
    }
}

const fn at(seconds: i64) -> i64 {
    T0 + seconds * 1000
}

/// Feed `(seconds from T0, status)` events; returns each decision.
fn run(state: &mut State, events: &[(i64, HerdrStatus)], focused: bool) -> Vec<Decision> {
    events
        .iter()
        .map(|&(seconds, status)| decide(state, P, status, at(seconds), focused, Auto::default()))
        .collect()
}

fn last(state: &mut State, events: &[(i64, HerdrStatus)]) -> Decision {
    run(state, events, false).pop().unwrap()
}

fn skip(reason: &str) -> Decision {
    Decision::Skip(reason.into())
}

use HerdrStatus::{Blocked, Done, Idle, Working};

#[test]
fn a_long_turn_ending_in_done_notifies() {
    assert_eq!(
        last(&mut fresh(), &[(0, Working), (300, Done)]),
        Decision::Notify(Settled::Done)
    );
}

#[test]
fn a_turn_shorter_than_min_turn_does_not() {
    let mut state = fresh();
    let _started = decide(&mut state, P, Working, T0, false, Auto::default());
    assert_eq!(
        decide(&mut state, P, Done, T0 + 120_000 - 1, false, Auto::default()),
        skip("turn shorter than 120s")
    );
}

#[test]
fn configured_thresholds_replace_the_defaults() {
    let quick = Auto {
        min_turn_seconds: 10,
        cooldown_seconds: 0,
        ..Auto::default()
    };
    let mut state = fresh();
    let mut feed = |seconds, status| decide(&mut state, P, status, at(seconds), false, quick);
    let _started = feed(0, Working);
    assert_eq!(feed(9, Done), skip("turn shorter than 10s"));
    let _again = feed(20, Working);
    assert_eq!(feed(30, Done), Decision::Notify(Settled::Done));
    assert_eq!(feed(31, Blocked), Decision::Notify(Settled::Blocked), "no cooldown");
}

#[test]
fn done_with_no_recorded_start_does_not() {
    assert_eq!(last(&mut fresh(), &[(300, Done)]), skip("turn start unknown"));
}

#[test]
fn turn_length_counts_from_the_first_working_through_blocked() {
    let mut state = fresh();
    let _blocked = run(&mut state, &[(0, Working), (60, Blocked)], false);
    state.panes.get_mut(P).unwrap().last_auto_at = None; // ignore the blocked notification's cooldown
    assert_eq!(
        last(&mut state, &[(90, Working), (360, Done)]),
        Decision::Notify(Settled::Done)
    );
}

#[test]
fn idle_ends_the_turn_without_notifying() {
    let decisions = run(&mut fresh(), &[(0, Working), (300, Idle), (360, Done)], false);
    assert_eq!(decisions[1..], [skip("status idle"), skip("turn start unknown")]);
}

#[test]
fn blocked_notifies_immediately() {
    assert_eq!(
        last(&mut fresh(), &[(0, Working), (6, Blocked)]),
        Decision::Notify(Settled::Blocked)
    );
}

#[test]
fn cooldown_allows_one_automatic_notification_per_pane_per_window() {
    let after_cooldown = 60 + 300 + 120;
    let decisions = run(
        &mut fresh(),
        &[
            (0, Working),
            (60, Blocked),
            (120, Working),
            (240, Done),
            (300, Working),
            (after_cooldown, Done),
        ],
        false,
    );
    assert_eq!(decisions.iter().filter(|d| matches!(d, Decision::Notify(_))).count(), 2);
    assert_eq!(decisions[3], skip("cooldown"));
}

#[test]
fn cooldown_is_per_pane() {
    let mut state = fresh();
    let _blocked = run(&mut state, &[(0, Working), (60, Blocked)], false);
    assert_eq!(
        decide(&mut state, "other:p1", Blocked, at(90), false, Auto::default()),
        Decision::Notify(Settled::Blocked)
    );
}

#[test]
fn skipped_when_the_agent_already_notified_this_turn() {
    let mut state = fresh();
    let _started = run(&mut state, &[(0, Working)], false);
    state.panes.get_mut(P).unwrap().self_notified_at = Some(at(240));
    assert_eq!(
        last(&mut state, &[(300, Done)]),
        skip("agent already notified the webhook this turn")
    );
}

#[test]
fn a_self_notify_from_a_previous_turn_does_not_suppress_this_one() {
    let mut state = fresh();
    let earlier = PaneState {
        self_notified_at: Some(at(-60)),
        ..PaneState::default()
    };
    let _previous = state.panes.insert(P.into(), earlier);
    assert_eq!(
        last(&mut state, &[(0, Working), (300, Done)]),
        Decision::Notify(Settled::Done)
    );
}

#[test]
fn away_mode_notifies_for_the_focused_pane_and_with_auto_off_but_keeps_the_other_rules() {
    let mut state = State {
        auto: false,
        away: true,
        ..State::default()
    };
    let decisions = run(&mut state, &[(0, Working), (60, Blocked)], true);
    assert_eq!(decisions[1], Decision::Notify(Settled::Blocked));
    let quick = run(&mut state, &[(70, Working), (80, Done)], true);
    assert_eq!(
        quick[1],
        skip("turn shorter than 120s"),
        "short turns still don't count"
    );
    state.away = false;
    assert_eq!(last(&mut state, &[(500, Blocked)]), skip("auto-notify is off"));
}

#[test]
fn focused_panes_and_auto_off_are_skipped_without_starting_a_cooldown() {
    let mut state = fresh();
    let focused = run(&mut state, &[(0, Working), (60, Blocked)], true);
    assert_eq!(focused[1], skip("you are looking at this pane"));
    assert_eq!(state.panes[P].last_auto_at, None);
    state.auto = false;
    assert_eq!(last(&mut state, &[(120, Blocked)]), skip("auto-notify is off"));
    state.auto = true;
    assert_eq!(last(&mut state, &[(180, Blocked)]), Decision::Notify(Settled::Blocked));
}
