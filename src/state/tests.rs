use std::thread;

use super::{PaneState, update_state};
use crate::env::Env;

#[test]
fn concurrent_writers_lose_no_updates() {
    let dir = tempfile::tempdir().unwrap();
    let env = Env::from([("NUDGE_STATE_DIR", dir.path().to_str().unwrap())]);
    thread::scope(|scope| {
        for i in 0..20 {
            let env = &env;
            let _writer = scope.spawn(move || {
                update_state(env, |state| {
                    let pane = PaneState {
                        last_auto_at: Some(i),
                        ..PaneState::default()
                    };
                    state.panes.insert(format!("p{i}"), pane)
                })
                .unwrap()
            });
        }
    });
    assert_eq!(update_state(&env, |state| state.panes.len()).unwrap(), 20);
}

#[test]
fn corrupt_state_starts_fresh_with_auto_off() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("state.json"), "{not json").unwrap();
    let env = Env::from([("NUDGE_STATE_DIR", dir.path().to_str().unwrap())]);
    assert!(!update_state(&env, |state| state.auto).unwrap());
}
