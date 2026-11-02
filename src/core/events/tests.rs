use std::collections::BTreeMap;

use super::{Change, CoreLine, Snapshot, StatusWord, TerminalRecord, parse_line};
use crate::source::AgentStatus;

fn event(kind: &str, terminal: &str, data: &str) -> String {
    format!(
        r#"{{"jsonrpc":"2.0","method":"event","params":{{"type":"{kind}","terminal":"{terminal}","data":{data},"seq":7}}}}"#
    )
}

fn changed(change: Change) -> CoreLine {
    CoreLine::Event {
        seq: 7,
        terminal: "t3".into(),
        change,
    }
}

#[test]
fn the_first_line_and_missed_are_snapshots() {
    let terminal = r#"{"address":"t3","name":"api","labels":{"herdr.space":"s1"},"cwd":"/srv","argv":["claude"],
        "state":"live","status":{"state":"working","app":"claude"},"status_ms":1800000000000,"turn":4}"#;
    let expected = CoreLine::Snapshot(Snapshot {
        seq: 41,
        terminals: vec![TerminalRecord {
            address: "t3".into(),
            name: Some("api".into()),
            labels: BTreeMap::from([("herdr.space".into(), "s1".into())]),
            cwd: Some("/srv".into()),
            last_cwd: None,
            status: Some(StatusWord {
                state: "working".into(),
            }),
            status_ms: Some(1_800_000_000_000),
        }],
    });
    assert_eq!(
        parse_line(&format!(r#"{{"seq":41,"terminals":[{terminal}]}}"#)),
        expected
    );
    let missed = format!(r#"{{"jsonrpc":"2.0","method":"missed","params":{{"seq":41,"terminals":[{terminal}]}}}}"#);
    assert_eq!(parse_line(&missed), expected);
}

#[test]
fn events_become_changes_and_the_rest_is_skipped() {
    let status = |word: &str| {
        event(
            "status",
            "t3",
            &format!(r#"{{"status":{word},"records":{{}},"reported":true}}"#),
        )
    };
    for (word, expected) in [
        (r#"{"state":"working"}"#, AgentStatus::Working),
        (r#"{"state":"done"}"#, AgentStatus::Done),
        (r#"{"state":"blocked","kind":"permission"}"#, AgentStatus::Blocked),
        (r#"{"state":"error"}"#, AgentStatus::Error),
        (r#"{"state":"idle"}"#, AgentStatus::Idle),
        (r#"{"state":"pondering"}"#, AgentStatus::Unknown),
        ("null", AgentStatus::Unknown),
    ] {
        assert_eq!(parse_line(&status(word)), changed(Change::Status(expected)), "{word}");
    }
    assert_eq!(
        parse_line(&event("labels", "t3", r#"{"labels":{"herdr.space":"s2"}}"#)),
        changed(Change::Labels(BTreeMap::from([("herdr.space".into(), "s2".into())])))
    );
    assert_eq!(
        parse_line(&event("created", "t3", r#"{"address":"t3","name":"new"}"#)),
        changed(Change::Created(TerminalRecord {
            address: "t3".into(),
            name: Some("new".into()),
            ..TerminalRecord::default()
        }))
    );
    for kind in ["finished", "removed"] {
        assert_eq!(parse_line(&event(kind, "t3", "{}")), changed(Change::Gone), "{kind}");
    }
    assert_eq!(
        parse_line(r#"{"jsonrpc":"2.0","method":"heartbeat","params":{"seq":9}}"#),
        CoreLine::Heartbeat { seq: 9 }
    );
    for other in [
        event("turn", "t3", r#"{"turn":5,"state":"done"}"#),
        event("title", "t3", r#"{"title":"x"}"#),
        r#"{"jsonrpc":"2.0","method":"event","params":{"type":"doc","data":{"name":"n"},"seq":3}}"#.to_owned(),
        r#"{"jsonrpc":"2.0","method":"something-new","params":{}}"#.to_owned(),
    ] {
        assert_eq!(parse_line(&other), CoreLine::Other, "{other}");
    }
}

#[test]
fn a_malformed_line_is_reported_never_a_panic() {
    for (line, why) in [
        ("garbage", "not JSON"),
        (r#"{"seq":1}"#, "not a snapshot"),
        (r#"{"method":5}"#, "not a notification"),
        (
            r#"{"jsonrpc":"2.0","method":"event","params":{"terminal":"t3"}}"#,
            "bad event",
        ),
        (&event("status", "t3", r#"{"status":"done"}"#), "bad status event"),
    ] {
        assert_eq!(parse_line(line), CoreLine::Malformed(why.into()), "{line}");
    }
}
