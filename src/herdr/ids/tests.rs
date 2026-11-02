use super::{Env, Sender, SenderIds};
use crate::config::{Route, Scope, Selector};
use crate::env::tests::FakeRun;

fn scope(workspaces: &[&str], agents: &[&str]) -> Scope {
    Scope {
        workspaces: workspaces.iter().map(|&w| w.to_owned()).collect(),
        agents: agents.iter().map(|&a| a.to_owned()).collect(),
    }
}

#[test]
fn scope_allows_listed_agents_and_workspaces_by_label_or_id() {
    let label = r#"{"result":{"workspace":{"workspace_id":"wD","label":"team-web"}}}"#;
    let run = FakeRun::new([("herdr workspace get wD", label)]);
    let sender = Sender {
        agent: Some("api-refactor".into()),
        pane_id: Some("wD:p1".into()),
        workspace_id: Some("wD".into()),
    };
    let ids = SenderIds::lookup(&sender, true, &Env::default(), &run);
    let allows = |workspaces: &[&str], agents: &[&str]| ids.in_scope(&scope(workspaces, agents));
    assert!(allows(&[], &[]), "an empty scope allows everyone");
    assert!(allows(&["team-web"], &[]));
    assert!(allows(&["wD"], &[]));
    assert!(allows(&[], &["api-refactor"]));
    assert!(allows(&[], &["wD:p1"]));
    assert!(allows(&["other"], &["api-refactor"]), "either list is enough");
    assert!(!allows(&["other"], &["reviewer"]));
    let unknown = SenderIds::lookup(&Sender::default(), true, &Env::default(), &run);
    assert!(!unknown.in_scope(&scope(&["team-web"], &[])));
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
    let ids = SenderIds::lookup(&shell, true, &Env::default(), &run);
    assert!(ids.in_scope(&scope(&["api-work"], &[])));
    let routes = [Route {
        selector: Selector::Workspace("api-work".into()),
        webhook: "client-a".into(),
        backend: None,
    }];
    assert_eq!(ids.route(&routes), routes.first());
}

#[test]
fn without_the_workspace_herdr_is_not_asked_and_only_names_match() {
    let run = FakeRun::default();
    let sender = Sender {
        agent: Some("a".into()),
        pane_id: None,
        workspace_id: Some("wD".into()),
    };
    let ids = SenderIds::lookup(&sender, false, &Env::default(), &run);
    assert!(ids.in_scope(&scope(&[], &["a"])));
    assert!(!ids.in_scope(&scope(&["wD"], &[])));
    let by_workspace = [Route {
        selector: Selector::Workspace("wD".into()),
        webhook: "client-a".into(),
        backend: None,
    }];
    assert_eq!(ids.route(&by_workspace), None);
    assert_eq!(run.calls(), Vec::<String>::new());
}
