use std::collections::BTreeMap;

use super::{Effect, Tracker};
use crate::core::events::{Change, CoreLine, Snapshot, StatusWord, TerminalRecord};
use crate::source::{AgentRef, AgentStatus, Backend, Transition, WorkspaceRef};

const NOW: i64 = 1_800_000_600_000;

fn record(address: &str, status: Option<&str>, status_ms: Option<i64>) -> TerminalRecord {
    TerminalRecord {
        address: address.into(),
        name: Some(format!("agent-{address}")),
        labels: BTreeMap::from([("herdr.space".into(), "s1".into())]),
        cwd: Some("/start".into()),
        last_cwd: Some("/now".into()),
        status: status.map(|state| StatusWord { state: state.into() }),
        status_ms,
    }
}

fn status(terminal: &str, status: AgentStatus) -> CoreLine {
    CoreLine::Event {
        seq: 2,
        terminal: terminal.into(),
        change: Change::Status(status),
    }
}

fn snapshot(terminals: Vec<TerminalRecord>) -> CoreLine {
    CoreLine::Snapshot(Snapshot { seq: 1, terminals })
}

#[test]
fn a_snapshot_starts_over_without_notifying() {
    let mut tracker = Tracker::new(false);
    let effect = tracker.apply(
        snapshot(vec![
            record("t1", Some("working"), Some(NOW - 600_000)),
            record("t2", Some("blocked"), Some(NOW - 5_000)),
            record("t3", Some("working"), None),
            record("box/t7", Some("working"), Some(NOW)),
        ]),
        NOW,
    );
    assert_eq!(
        effect,
        Effect::Reconcile(vec![
            ("core:t1".into(), Some(NOW - 600_000)),
            ("core:t2".into(), None),
            ("core:t3".into(), Some(NOW)),
        ]),
        "a working terminal's turn started when its status did; another machine's terminal isn't followed"
    );
    let followed: Vec<&str> = tracker.terminals().map(|record| record.address.as_str()).collect();
    assert_eq!(followed, ["t1", "t2", "t3"]);

    let mut connected = Tracker::new(true);
    let Effect::Reconcile(turns) = connected.apply(snapshot(vec![record("box/t7", None, None)]), NOW) else {
        panic!("a snapshot reconciles");
    };
    assert_eq!(turns, [("core:box/t7".into(), None)]);
}

#[test]
fn a_status_change_is_a_transition_for_the_terminal_as_last_described() {
    let mut tracker = Tracker::new(false);
    let _reconciled = tracker.apply(snapshot(vec![record("t1", Some("working"), Some(NOW))]), NOW);
    let _relabelled = tracker.apply(
        CoreLine::Event {
            seq: 2,
            terminal: "t1".into(),
            change: Change::Labels(BTreeMap::from([("herdr.space".into(), "s9".into())])),
        },
        NOW,
    );
    let expected = Transition {
        agent: AgentRef {
            backend: Backend::Core,
            pane: "t1".into(),
            name: Some("agent-t1".into()),
            workspace: Some(WorkspaceRef {
                id: "s9".into(),
                name: None,
            }),
            cwd: Some("/now".into()),
        },
        status: AgentStatus::Done,
        watched: false,
        at_ms: NOW + 1,
    };
    assert_eq!(
        tracker.apply(status("t1", AgentStatus::Done), NOW + 1),
        Effect::Transition(expected)
    );
    let Effect::Transition(unknown) = tracker.apply(status("t4", AgentStatus::Blocked), NOW) else {
        panic!("a terminal the snapshot didn't have still counts");
    };
    assert_eq!(unknown.agent, AgentRef::in_pane(Backend::Core, "t4".into()));
    assert_eq!(
        tracker.apply(status("box/t7", AgentStatus::Blocked), NOW),
        Effect::Nothing
    );
}

#[test]
fn a_finished_or_removed_terminal_is_forgotten() {
    let mut tracker = Tracker::new(false);
    let _reconciled = tracker.apply(snapshot(vec![record("t1", None, None)]), NOW);
    let gone = CoreLine::Event {
        seq: 3,
        terminal: "t1".into(),
        change: Change::Gone,
    };
    assert_eq!(tracker.apply(gone, NOW), Effect::Forget("core:t1".into()));
    assert_eq!(tracker.terminals().count(), 0);
    for nothing in [
        CoreLine::Heartbeat { seq: 4 },
        CoreLine::Other,
        CoreLine::Malformed("x".into()),
    ] {
        assert_eq!(tracker.apply(nothing, NOW), Effect::Nothing);
    }
}
