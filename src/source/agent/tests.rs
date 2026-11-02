use super::{AgentRef, Backend, WorkspaceRef};
use crate::config::{Route, Scope, Selector};

fn agent(name: Option<&str>, workspace: Option<(&str, Option<&str>)>) -> AgentRef {
    AgentRef {
        name: name.map(str::to_owned),
        workspace: workspace.map(|(id, name)| WorkspaceRef {
            id: id.to_owned(),
            name: name.map(str::to_owned),
        }),
        ..AgentRef::in_pane(Backend::Tui, "w1:p3".into())
    }
}

#[test]
fn state_key_and_reply_target() {
    let named = agent(Some("api-refactor"), None);
    assert_eq!(named.state_key(), "tui:w1:p3");
    assert_eq!(named.target(), "api-refactor");
    assert_eq!(
        agent(None, None).target(),
        "w1:p3",
        "an unnamed agent is replied to by its pane"
    );
}

#[test]
fn scope_and_routes_match_the_agent_by_name_or_pane_and_its_workspace_by_name_or_id() {
    let scope = |workspaces: &[&str], agents: &[&str]| Scope {
        workspaces: workspaces.iter().map(|&w| w.to_owned()).collect(),
        agents: agents.iter().map(|&a| a.to_owned()).collect(),
    };
    let located = agent(Some("api-refactor"), Some(("wD", Some("api-work"))));
    for (allowed, agents, workspaces) in [
        (true, &["api-refactor"][..], &[][..]),
        (true, &["w1:p3"], &[]),
        (true, &[], &["wD"]),
        (true, &[], &["api-work"]),
        (false, &["reviewer"], &["web-work"]),
    ] {
        assert_eq!(
            located.in_scope(&scope(workspaces, agents)),
            allowed,
            "{agents:?} {workspaces:?}"
        );
    }
    let unlocated = agent(Some("api-refactor"), None);
    assert!(
        !unlocated.in_scope(&scope(&["api-work"], &[])),
        "no workspace, no workspace match"
    );

    let routes = [Route {
        selector: Selector::Workspace("api-work".into()),
        webhook: "client-a".into(),
        backend: Some(Backend::Tui),
    }];
    assert_eq!(located.route(&routes), routes.first());
    assert_eq!(unlocated.route(&routes), None);
    let core = AgentRef {
        backend: Backend::Core,
        ..located
    };
    assert_eq!(core.route(&routes), None, "a tui-only route doesn't apply to core");
    assert_eq!(core.state_key(), "core:w1:p3");
}
