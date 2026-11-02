use std::fs;

use super::{AppContext, FRESH_MS, SpaceRef, name_space, read_app_context, save_app_context};
use crate::env::Env;
use crate::error::Error;
use crate::source::{AgentRef, Backend, WorkspaceRef};
use crate::state::now_ms;

fn env(dir: &tempfile::TempDir) -> Env {
    Env::from([("NUDGE_STATE_DIR", dir.path().to_str().unwrap())])
}

/// Sent with an `at` from a panel whose clock is a day ahead.
const SAVED: &str =
    r#"{"terminal":"t7","window_active":true,"spaces":[{"id":"s1","name":"client-a"}],"at":4102444800000}"#;

#[test]
fn the_context_is_stored_stamped_with_when_it_arrived_not_the_panels_clock() {
    let dir = tempfile::tempdir().unwrap();
    let env = env(&dir);
    assert_eq!(read_app_context(&env), None);
    let before = now_ms();
    assert_eq!(save_app_context(&env, SAVED).unwrap(), 0);
    let after = now_ms();
    let context = read_app_context(&env).unwrap();
    assert!(
        (before..=after).contains(&context.at),
        "{} not in {before}..={after}",
        context.at
    );
    assert!(context.watches("t7", after));
    assert!(
        !context.watches("t7", context.at + FRESH_MS),
        "stale 90 s after it arrived, whatever the panel said"
    );
    assert_eq!(context.space_name("s1").as_deref(), Some("client-a"));
    assert_eq!(context.space_name("s2"), None);
}

#[test]
fn malformed_context_is_refused_and_the_stored_one_kept() {
    let dir = tempfile::tempdir().unwrap();
    let env = env(&dir);
    save_app_context(&env, SAVED).unwrap();
    for bad in [
        "not json",
        r#"{"terminal":"t7","window_active":"yes","spaces":[],"at":1}"#,
        r#"{"terminal":"t7","spaces":[]}"#,
        r#"{"terminal":"t7","window_active":true,"spaces":[{"id":"s1"}],"at":1}"#,
    ] {
        let error = save_app_context(&env, bad).unwrap_err();
        assert!(matches!(error, Error::Usage(_)), "{bad}: {error}");
    }
    assert_eq!(read_app_context(&env).unwrap().terminal.as_deref(), Some("t7"));
    let names: Vec<_> = fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(names, ["context.json"], "no temp file left behind");
}

#[test]
fn a_terminal_is_watched_only_when_focused_in_an_active_window_with_a_fresh_context() {
    let at = 1_700_000_000_000;
    let context = AppContext {
        terminal: Some("t7".into()),
        window_active: true,
        spaces: Vec::new(),
        at,
    };
    assert!(context.watches("t7", at));
    assert!(context.watches("t7", at + 89_999), "89.999 s old is fresh");
    assert!(!context.watches("t7", at + 90_000), "90.000 s old is stale");
    assert!(
        !context.watches("t7", at - 1),
        "a context dated in the future isn't fresh"
    );
    assert!(!context.watches("t8", at));
    let inactive = AppContext {
        window_active: false,
        ..context.clone()
    };
    assert!(!inactive.watches("t7", at));
    let nothing_focused = AppContext {
        terminal: None,
        ..context
    };
    assert!(!nothing_focused.watches("t7", at));
}

#[test]
fn a_space_gets_its_name_from_the_context_unless_it_has_one() {
    let context = AppContext {
        terminal: None,
        window_active: false,
        spaces: vec![SpaceRef {
            id: "s1".into(),
            name: "client-a".into(),
        }],
        at: 0,
    };
    let in_space = |name: Option<&str>| {
        let mut agent = AgentRef::in_pane(Backend::Core, "t7".into());
        agent.workspace = Some(WorkspaceRef {
            id: "s1".into(),
            name: name.map(str::to_owned),
        });
        agent
    };
    let mut agent = in_space(None);
    name_space(&mut agent, Some(&context));
    assert_eq!(agent.workspace.unwrap().name.as_deref(), Some("client-a"));
    let mut named = in_space(Some("kept"));
    name_space(&mut named, Some(&context));
    assert_eq!(named.workspace.unwrap().name.as_deref(), Some("kept"));
    let mut no_context = in_space(None);
    name_space(&mut no_context, None);
    assert_eq!(no_context.workspace.unwrap().name, None);
}
