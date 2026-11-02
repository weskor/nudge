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

const fn pane(working_since: Option<i64>, last_auto_at: Option<i64>, self_notified_at: Option<i64>) -> PaneState {
    PaneState {
        working_since,
        last_auto_at,
        self_notified_at,
    }
}

#[test]
fn an_unprefixed_key_wins_over_its_tui_twin_field_by_field() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.json");
    // "p_1" sorts before "tui:p_1": the twin must not overwrite it.
    std::fs::write(
        &path,
        r#"{"panes":{"p_1":{"last_auto_at":200},"tui:p_1":{"working_since":50,"last_auto_at":100}}}"#,
    )
    .unwrap();
    let env = Env::from([("NUDGE_STATE_DIR", dir.path().to_str().unwrap())]);
    let panes = update_state(&env, |state| state.panes.clone()).unwrap();
    assert_eq!(
        panes.into_iter().collect::<Vec<_>>(),
        [("tui:p_1".to_owned(), pane(Some(50), Some(200), None))]
    );
    let saved: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(
        saved["panes"],
        serde_json::json!({"tui:p_1": {"working_since": 50, "last_auto_at": 200}})
    );
}

#[test]
fn what_an_older_nudge_records_after_the_migration_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.json");
    let env = Env::from([("NUDGE_STATE_DIR", dir.path().to_str().unwrap())]);
    std::fs::write(&path, r#"{"auto":true,"panes":{"p_1":{"last_auto_at":10}}}"#).unwrap();
    // This build migrates the key, then the plugin tracks a turn.
    update_state(&env, |state| {
        state.panes.get_mut("tui:p_1").unwrap().working_since = Some(50);
    })
    .unwrap();

    // An older `nudge notify` keeps the "tui:p_1" entry it doesn't know and records under "p_1", from scratch.
    let mut saved: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    saved["panes"]["p_1"] = serde_json::json!({"self_notified_at": 300});
    std::fs::write(&path, saved.to_string()).unwrap();

    let panes = update_state(&env, |state| state.panes.clone()).unwrap();
    assert_eq!(
        panes.into_iter().collect::<Vec<_>>(),
        [("tui:p_1".to_owned(), pane(Some(50), Some(10), Some(300)))],
        "the notify is recorded and the plugin's turn and cooldown survive"
    );
}
