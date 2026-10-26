use crate::config::Scope;
use crate::settings::keys::Key;
use crate::settings::rows::Row;
use crate::settings::tests::{saved, settings, tap};

#[test]
fn ticking_a_workspace_or_agent_limits_the_scope_to_the_ticked_names() {
    let mut ui = settings(Scope::default());
    tap(&mut ui, Row::OnlyThese, Key::Toggle);
    tap(&mut ui, Row::Workspace(1), Key::Toggle);
    tap(&mut ui, Row::Agent(0), Key::Toggle);
    let scope = saved(&ui).scope;
    assert_eq!(scope.workspaces, ["team-web"]);
    assert_eq!(scope.agents, ["builder"]);
    tap(&mut ui, Row::Everyone, Key::Toggle);
    assert!(
        saved(&ui).scope.is_everyone(),
        "ticks are kept but not applied for every agent"
    );
}
