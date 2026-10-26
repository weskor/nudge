use crate::config::Scope;
use crate::settings::tests::{saved, settings};

#[test]
fn saved_scope_shows_ticked_by_label_or_id_and_keeps_names_that_are_not_open() {
    let ui = settings(Scope {
        workspaces: vec!["wD".into(), "old-project".into()],
        agents: vec!["reviewer".into()],
    });
    let ticked: Vec<(&str, bool, bool)> = ui
        .workspaces
        .iter()
        .map(|choice| (choice.name.as_str(), choice.checked, choice.live))
        .collect();
    assert_eq!(
        ticked,
        [
            ("Base", false, true),
            ("team-web", true, true),
            ("old-project", true, false)
        ]
    );
    let scope = saved(&ui).scope;
    assert_eq!(scope.workspaces, ["team-web", "old-project"]);
    assert_eq!(scope.agents, ["reviewer"]);
}
