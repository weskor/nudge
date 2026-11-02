use crate::config::{Backend, Route, Scope, Selector};
use crate::settings::tests::{screen_text, settings};

#[test]
fn open_agents_show_their_workspace_and_directory() {
    let ui = settings(Scope {
        workspaces: vec![],
        agents: vec!["gone".into()],
    });
    let text = screen_text(&ui);
    for expected in [
        "[ ] Base\n",
        "[ ] team-web  1 agent",
        "[ ] builder   team-web · nudge",
        "[ ] reviewer  wX",
        "[x] gone      (not open)",
    ] {
        assert!(text.contains(expected), "{expected:?} in\n{text}");
    }
}

#[test]
fn routes_are_listed_read_only_and_only_when_there_are_any() {
    let mut ui = settings(Scope::default());
    assert!(!screen_text(&ui).contains("Routes"));
    let rows = ui.rows();
    ui.routes = vec![
        Route {
            selector: Selector::Pane("builder".into()),
            webhook: "research".into(),
            backend: None,
        },
        Route {
            selector: Selector::Workspace("team-web".into()),
            webhook: "bridge".into(),
            backend: Some(Backend::Core),
        },
    ];
    let text = screen_text(&ui);
    let expected = "Routes\n  pane \"builder\" → research\n  workspace \"team-web\" → bridge (core only)\n  \
                    change with nudge route set / nudge route rm";
    assert!(text.ends_with(expected), "{text}");
    assert_eq!(ui.rows(), rows, "the cursor never stops on a route");
}
