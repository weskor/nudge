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
fn corrupt_state_starts_fresh_with_nudge_on_and_auto_off() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("state.json"), "{not json").unwrap();
    let env = Env::from([("NUDGE_STATE_DIR", dir.path().to_str().unwrap())]);
    assert_eq!(
        update_state(&env, |state| (state.on, state.auto)).unwrap(),
        (true, false)
    );
}

#[test]
fn old_pane_keys_become_tui_keys_and_the_switch_is_saved_only_when_off() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.json");
    std::fs::write(
        &path,
        r#"{"auto":true,"panes":{"w1:p3":{"last_auto_at":1},"tui:p_2":{"last_auto_at":2},"core:t7":{"last_auto_at":3}}}"#,
    )
    .unwrap();
    let env = Env::from([("NUDGE_STATE_DIR", dir.path().to_str().unwrap())]);
    let keys = update_state(&env, |state| state.panes.keys().cloned().collect::<Vec<_>>()).unwrap();
    assert_eq!(keys, ["core:t7", "tui:p_2", "tui:w1:p3"]);
    let saved: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(saved["panes"]["tui:w1:p3"]["last_auto_at"], 1);
    assert_eq!(saved.get("on"), None, "on is the default, so it isn't written");

    update_state(&env, |state| state.on = false).unwrap();
    let saved: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(saved["on"], false);
    assert!(!update_state(&env, |state| state.on).unwrap(), "off survives a reload");
}
