use crate::config::Scope;
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
