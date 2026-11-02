use std::collections::VecDeque;
use std::fs;

use super::core_status;
use crate::config::read_config;
use crate::core::context::save_app_context;
use crate::core::status_file::{DecisionRecord, TerminalStatus, WatchStatus, write_status};
use crate::env::Env;
use crate::env::tests::FakeRun;
use crate::state::now_ms;

const CONFIG: &str = r#"default_webhook = "main"

[[webhook]]
name = "main"
url = "https://hooks.example.com/main?token=s3cret-query"
authorization = "Bearer t0ken-for-tests"

[[webhook]]
name = "client-a"
url = "https://client.example.com/hook"
authorization = "Bearer t0ken-for-tests"

[[route]]
workspace = "client"
webhook = "client-a"
"#;

fn terminal(address: &str, space: Option<&str>) -> TerminalStatus {
    TerminalStatus {
        address: address.into(),
        name: None,
        status: Some("working".into()),
        space: space.map(str::to_owned),
        webhook: None,
    }
}

/// A state dir with a config, a context naming space s1 "client", and a `watch.json` from watcher `pid`.
fn setup(pid: u32) -> (tempfile::TempDir, Env) {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("config.toml"), CONFIG).unwrap();
    let env = Env::from([
        ("NUDGE_STATE_DIR", dir.path().to_str().unwrap()),
        ("HOME", dir.path().to_str().unwrap()),
    ]);
    let context = format!(
        r#"{{"terminal":"t1","window_active":true,"spaces":[{{"id":"s1","name":"client"}},{{"id":"s2","name":"docs"}}],"at":{}}}"#,
        now_ms()
    );
    save_app_context(&env, &context).unwrap();
    let decisions = VecDeque::from([DecisionRecord {
        at: 1,
        terminal: "t1".into(),
        status: "done".into(),
        outcome: "skipped (you are looking at this pane)".into(),
    }]);
    let file = WatchStatus {
        pid,
        started_at: 5,
        started_by: "service".into(),
        connected: true,
        last_seq: Some(9),
        last_event_at: Some(7),
        terminals: vec![terminal("t1", Some("s1")), terminal("t2", None)],
        decisions: std::borrow::Cow::Owned(decisions),
    };
    write_status(dir.path(), &file).unwrap();
    fs::write(dir.path().join("watch.lock"), pid.to_string()).unwrap();
    (dir, env)
}

fn status(env: &Env) -> serde_json::Value {
    let config = read_config(&env.home().join("config.toml")).unwrap();
    serde_json::to_value(core_status(env, &FakeRun::default(), config.as_ref())).unwrap()
}

#[test]
fn spaces_and_terminals_show_where_they_notify_using_the_apps_space_names() {
    let (_dir, env) = setup(std::process::id());
    let status = status(&env);
    assert_eq!(
        status["spaces"],
        serde_json::json!([
            {"id": "s1", "name": "client", "webhook": "client-a", "via": "workspace"},
            {"id": "s2", "name": "docs", "webhook": "main", "via": "default"},
        ])
    );
    let terminals = &status["terminals"];
    assert_eq!(
        (&terminals[0]["space_name"], &terminals[0]["webhook"]),
        (&"client".into(), &"client-a".into())
    );
    assert_eq!(terminals[1]["via"], "default");
    assert_eq!(status["context"]["fresh"], true);
    assert_eq!(
        status["decisions"][0]["outcome"],
        "skipped (you are looking at this pane)"
    );
    let text = status.to_string();
    assert!(
        !text.contains("t0ken") && !text.contains("s3cret"),
        "no secrets: {text}"
    );
}

#[test]
fn the_watcher_runs_only_while_the_lock_names_a_live_process() {
    let (_dir, env) = setup(std::process::id());
    let watcher = &status(&env)["watcher"];
    assert_eq!(
        (&watcher["running"], &watcher["connected"]),
        (&true.into(), &true.into())
    );
    // pid 0 is never a watcher; nor is a pid no process has.
    for dead in [0, 2_000_000_000] {
        let (_dir, env) = setup(dead);
        let watcher = &status(&env)["watcher"];
        assert_eq!(
            (&watcher["running"], &watcher["connected"], &watcher["pid"]),
            (&false.into(), &false.into(), &serde_json::Value::Null),
            "{dead}"
        );
    }
}
