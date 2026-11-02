use std::fs;

use super::Watcher;
use crate::core::context::save_app_context;
use crate::env::Env;
use crate::env::tests::FakeRun;
use crate::state::{now_ms, update_state};
use crate::webhook::tests::FakeWebhook;

fn status(terminal: &str, word: &str) -> String {
    format!(
        r#"{{"jsonrpc":"2.0","method":"event","params":{{"type":"status","terminal":"{terminal}","data":{{"status":{{"state":"{word}"}}}},"seq":2}}}}"#
    )
}

fn event(kind: &str, terminal: &str) -> String {
    format!(
        r#"{{"jsonrpc":"2.0","method":"event","params":{{"type":"{kind}","terminal":"{terminal}","data":{{}},"seq":3}}}}"#
    )
}

/// A temp state dir (auto-notify on) and a config with one webhook at `base`.
fn setup(base: &str) -> (tempfile::TempDir, Env) {
    let state = tempfile::tempdir().unwrap();
    let config = state.path().join("config.toml");
    fs::write(
        &config,
        format!("[[webhook]]\nname = \"main\"\nurl = \"{base}/hook\"\nauthorization = \"Bearer t0ken-for-tests\"\n"),
    )
    .unwrap();
    let env = Env::from([
        ("NUDGE_STATE_DIR", state.path().to_str().unwrap()),
        ("NUDGE_CONFIG", config.to_str().unwrap()),
        ("HOME", "/home/me"),
    ]);
    update_state(&env, |state| state.auto = true).unwrap();
    (state, env)
}

/// The stream's first line: t1 working for 10 minutes, t2 blocked, and another machine's terminal.
fn snapshot(since: i64) -> String {
    format!(
        r#"{{"seq":1,"terminals":[{{"address":"t1","name":"api","labels":{{"herdr.space":"s1"}},"status":{{"state":"working"}},"status_ms":{since}}},{{"address":"t2","status":{{"state":"blocked"}}}},{{"address":"box/t7","status":{{"state":"working"}}}}]}}"#
    )
}

fn bodies(hook: &FakeWebhook) -> Vec<serde_json::Value> {
    hook.requests()
        .iter()
        .map(|request| serde_json::from_str(&request.body).unwrap())
        .collect()
}

#[test]
fn the_snapshot_never_notifies_but_starts_the_turns() {
    let hook = FakeWebhook::start(|_| (200, String::new()));
    let (_state, env) = setup(&hook.base);
    let run = FakeRun::default();
    let mut watcher = Watcher::new(&env, &run);
    watcher.set_connected(true);
    let since = now_ms() - 600_000;
    assert!(watcher.line(&snapshot(since)));
    assert!(hook.requests().is_empty(), "not even for a blocked terminal");
    let started = update_state(&env, |state| state.panes["core:t1"].working_since).unwrap();
    assert_eq!(started, Some(since), "the turn began when the status did");
}

#[test]
fn settled_terminals_notify_with_the_core_reply_and_an_error_as_blocked() {
    let hook = FakeWebhook::start(|_| (200, String::new()));
    let (_state, env) = setup(&hook.base);
    let run = FakeRun::new([(
        "hc read t1 --lines 200",
        "build ok\n[hc] incomplete: older output may be missing",
    )]);
    let mut watcher = Watcher::new(&env, &run);
    watcher.set_connected(true);
    let _snapshot = watcher.line(&snapshot(now_ms() - 600_000));
    for line in [
        status("t1", "done"),
        status("t3", "error"),
        status("box/t7", "blocked"),
        "not json".to_owned(),
    ] {
        assert!(!watcher.line(&line));
    }
    let bodies = bodies(&hook);
    assert_eq!(
        bodies.len(),
        2,
        "t1 done and t3's error; box/t7 is its own machine's watcher's"
    );
    assert_eq!(
        (&bodies[0]["from_agent"], &bodies[0]["status"], &bodies[0]["reply_hint"]),
        (&"api".into(), &"done".into(), &"reply with: hc send t1 \"...\"".into())
    );
    assert_eq!(
        (&bodies[0]["source"]["backend"], &bodies[0]["source"]["pane"]),
        (&"core".into(), &"t1".into())
    );
    assert!(
        bodies[0]["message"].as_str().unwrap().ends_with("build ok"),
        "hc's own notes are left out"
    );
    assert_eq!(bodies[1]["status"], "blocked");
    assert!(
        bodies[1]["message"]
            .as_str()
            .unwrap()
            .starts_with("Agent reported an error")
    );
}

#[test]
fn decisions_and_terminals_are_in_watch_json_and_a_removed_terminal_is_forgotten() {
    let hook = FakeWebhook::start(|_| (200, String::new()));
    let (state, env) = setup(&hook.base);
    let run = FakeRun::default();
    let mut watcher = Watcher::new(&env, &run);
    watcher.set_connected(true);
    let _snapshot = watcher.line(&snapshot(now_ms() - 600_000));
    for line in [status("t2", "blocked"), status("t2", "blocked"), event("removed", "t1")] {
        let _event = watcher.line(&line);
    }
    assert_eq!(hook.requests().len(), 1, "the cooldown applies");
    assert!(!update_state(&env, |state| state.panes.contains_key("core:t1")).unwrap());
    let text = fs::read_to_string(state.path().join("watch.json")).unwrap();
    assert!(!text.contains("t0ken"), "no secrets");
    let file: serde_json::Value = serde_json::from_str(&text).unwrap();
    let outcomes: Vec<&str> = file["decisions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|decision| decision["outcome"].as_str().unwrap())
        .collect();
    assert_eq!(outcomes, ["notified webhook (HTTP 200)", "skipped (cooldown)"]);
    let terminals: Vec<(&str, &str)> = file["terminals"]
        .as_array()
        .unwrap()
        .iter()
        .map(|terminal| {
            (
                terminal["address"].as_str().unwrap(),
                terminal["webhook"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(terminals, [("t2", "main")]);
}

#[test]
fn missed_reconciles_without_notifying_and_drops_terminals_that_went_away() {
    let hook = FakeWebhook::start(|_| (200, String::new()));
    let (_state, env) = setup(&hook.base);
    let run = FakeRun::default();
    let mut watcher = Watcher::new(&env, &run);
    watcher.set_connected(true);
    let _first = watcher.line(r#"{"seq":1,"terminals":[{"address":"t1"},{"address":"t2"}]}"#);
    let _working = watcher.line(&status("t1", "working"));
    let _working = watcher.line(&status("t2", "working"));
    let missed = r#"{"jsonrpc":"2.0","method":"missed","params":{"seq":9,"terminals":[{"address":"t2","status":{"state":"blocked"}}]}}"#;
    assert!(watcher.line(missed));
    assert!(hook.requests().is_empty());
    let panes = update_state(&env, |state| state.panes.clone()).unwrap();
    assert_eq!(
        panes.keys().collect::<Vec<_>>(),
        ["core:t2"],
        "t1 went away while the watcher fell behind"
    );
    assert_eq!(
        panes["core:t2"].working_since, None,
        "a turn the watcher couldn't follow isn't counted"
    );
}

#[test]
fn the_terminal_in_focus_in_the_app_is_skipped_unless_the_context_is_stale() {
    let hook = FakeWebhook::start(|_| (200, String::new()));
    let (state, env) = setup(&hook.base);
    let run = FakeRun::default();
    let mut watcher = Watcher::new(&env, &run);
    watcher.set_connected(true);
    let _snapshot = watcher.line(&snapshot(now_ms() - 600_000));
    save_app_context(&env, r#"{"terminal":"t1","window_active":true,"spaces":[]}"#).unwrap();
    let _done = watcher.line(&status("t1", "done"));
    assert!(hook.requests().is_empty(), "you were looking at t1");
    // A stored context received 90 s ago, and one dated a day ahead (this clock went back): neither watches.
    for (terminal, at) in [("t2", now_ms() - 90_000), ("t3", now_ms() + 86_400_000)] {
        let stored = format!(r#"{{"terminal":"{terminal}","window_active":true,"spaces":[],"at":{at}}}"#);
        fs::write(state.path().join("context.json"), stored).unwrap();
        let _blocked = watcher.line(&status(terminal, "blocked"));
    }
    assert_eq!(hook.requests().len(), 2, "t2 and t3 notified");
}

#[test]
fn a_route_can_name_the_space_the_app_calls_it() {
    let hook = FakeWebhook::start(|_| (200, String::new()));
    let (state, env) = setup(&hook.base);
    let config = state.path().join("config.toml");
    let text = format!(
        "{}\n[[webhook]]\nname = \"client-a\"\nurl = \"{}/client\"\nauthorization = \"Bearer t0ken-for-tests\"\n\n[[route]]\nworkspace = \"client\"\nwebhook = \"client-a\"\n",
        fs::read_to_string(&config).unwrap(),
        hook.base
    );
    fs::write(&config, text).unwrap();
    let context = r#"{"terminal":"t1","window_active":false,"spaces":[{"id":"s1","name":"client"}]}"#;
    save_app_context(&env, context).unwrap();
    let run = FakeRun::default();
    let mut watcher = Watcher::new(&env, &run);
    watcher.set_connected(true);
    let _snapshot = watcher.line(&snapshot(now_ms() - 600_000));
    let _done = watcher.line(&status("t1", "done"));
    let requests = hook.requests();
    assert_eq!(requests.len(), 1, "an inactive window watches nothing");
    assert_eq!(requests[0].path, "/client");
}
