use std::io::Cursor;
use std::time::Duration;

use super::{Backoff, follow};
use crate::core::follow::Watcher;
use crate::env::Env;
use crate::env::tests::FakeRun;

#[test]
fn retries_back_off_to_every_30_seconds_and_start_over_after_a_good_stream() {
    let mut backoff = Backoff::default();
    let waits: Vec<u64> = (0..7).map(|_| backoff.next().as_secs()).collect();
    assert_eq!(waits, [1, 2, 5, 10, 30, 30, 30]);
    backoff.reset();
    assert_eq!(backoff.next(), Duration::from_secs(1));
}

#[test]
fn a_stream_counts_as_connected_once_its_snapshot_arrives() {
    let state = tempfile::tempdir().unwrap();
    let env = Env::from([("NUDGE_STATE_DIR", state.path().to_str().unwrap())]);
    let run = FakeRun::default();
    let mut watcher = Watcher::new(&env, &run);
    let heartbeat = r#"{"jsonrpc":"2.0","method":"heartbeat","params":{"seq":1}}"#;
    assert!(!follow(Cursor::new(format!("{heartbeat}\n")), &mut watcher));
    assert!(follow(
        Cursor::new(format!("{{\"seq\":1,\"terminals\":[]}}\n\n{heartbeat}\n")),
        &mut watcher
    ));
    let status: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(state.path().join("watch.json")).unwrap()).unwrap();
    assert_eq!(
        (&status["connected"], &status["last_seq"]),
        (&false.into(), &1.into()),
        "{status}"
    );
}
