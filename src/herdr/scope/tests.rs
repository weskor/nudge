use super::{Env, Sender, in_scope};
use crate::config::Scope;
use crate::env::tests::FakeRun;

#[test]
fn scope_allows_listed_agents_and_workspaces_by_label_or_id() {
    let label = r#"{"result":{"workspace":{"workspace_id":"wD","label":"team-web"}}}"#;
    let run = FakeRun::new([("herdr workspace get wD", label)]);
    let sender = Sender {
        agent: Some("api-refactor".into()),
        pane_id: Some("wD:p1".into()),
        workspace_id: Some("wD".into()),
    };
    let allows = |workspaces: &[&str], agents: &[&str]| {
        let scope = Scope {
            workspaces: workspaces.iter().map(|&w| w.to_owned()).collect(),
            agents: agents.iter().map(|&a| a.to_owned()).collect(),
        };
        in_scope(&scope, &sender, &Env::default(), &run)
    };
    assert!(allows(&[], &[]), "an empty scope allows everyone");
    assert!(allows(&["team-web"], &[]));
    assert!(allows(&["wD"], &[]));
    assert!(allows(&[], &["api-refactor"]));
    assert!(allows(&[], &["wD:p1"]));
    assert!(allows(&["other"], &["api-refactor"]), "either list is enough");
    assert!(!allows(&["other"], &["reviewer"]));
    assert!(!in_scope(
        &Scope {
            workspaces: vec!["team-web".into()],
            agents: Vec::new()
        },
        &Sender::default(),
        &Env::default(),
        &run
    ));
}

#[test]
fn a_pane_without_an_agent_is_placed_in_its_workspace_through_herdr() {
    let pane = r#"{"result":{"pane":{"pane_id":"wD:p3","workspace_id":"wD"}}}"#;
    let label = r#"{"result":{"workspace":{"workspace_id":"wD","label":"api-work"}}}"#;
    let run = FakeRun::new([("herdr pane get wD:p3", pane), ("herdr workspace get wD", label)]);
    let shell = Sender {
        agent: None,
        pane_id: Some("wD:p3".into()),
        workspace_id: None,
    };
    let scope = Scope {
        workspaces: vec!["api-work".into()],
        agents: Vec::new(),
    };
    assert!(in_scope(&scope, &shell, &Env::default(), &run));
}

#[test]
fn scope_with_only_agents_never_asks_herdr_for_the_workspace() {
    let run = FakeRun::default();
    let sender = Sender {
        agent: Some("a".into()),
        pane_id: None,
        workspace_id: Some("wD".into()),
    };
    let scope = Scope {
        workspaces: Vec::new(),
        agents: vec!["a".into()],
    };
    assert!(in_scope(&scope, &sender, &Env::default(), &run));
    assert_eq!(run.calls(), Vec::<String>::new());
}
