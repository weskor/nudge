use super::{Key, Outcome, Row, Settings, cycle, step};
use crate::config::{Auto, Config, Scope, Webhook};
use crate::herdr::{ListedAgent, ListedWorkspace};
use crate::state::State;

fn webhook(name: &str) -> Webhook {
    Webhook {
        name: name.into(),
        description: None,
        url: format!("https://{name}.example/hooks/secret-path"),
        authorization: "Bearer x".into(),
        signing_secret: None,
    }
}

fn workspace(id: &str, label: &str) -> ListedWorkspace {
    ListedWorkspace {
        workspace_id: id.into(),
        label: Some(label.into()),
    }
}

fn settings(scope: Scope) -> Settings {
    let config = Config {
        default_webhook: Some("bridge".into()),
        webhook: vec![webhook("bridge"), webhook("research")],
        scope,
        ..Config::default()
    };
    Settings::new(
        &config,
        &State::default(),
        &[workspace("w1", "Base"), workspace("wD", "team-web")],
        &[
            ListedAgent {
                name: "builder".into(),
                workspace_id: Some("wD".into()),
                cwd: Some("/home/user/src/nudge".into()),
            },
            ListedAgent {
                name: "reviewer".into(),
                workspace_id: Some("wX".into()),
                cwd: None,
            },
        ],
    )
}

/// Move the cursor to `row` and press `key` there.
fn press_on(settings: &mut Settings, row: Row, key: Key) -> Outcome {
    settings.cursor = settings.rows().iter().position(|candidate| *candidate == row).unwrap();
    settings.handle(key)
}

/// Press `key` on `row`, expecting the UI to carry on.
fn tap(settings: &mut Settings, row: Row, key: Key) {
    assert_eq!(press_on(settings, row, key), Outcome::Continue);
}

fn screen_text(settings: &Settings) -> String {
    let (lines, _) = settings.lines();
    lines.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n")
}

fn saved(settings: &Settings) -> Config {
    let mut config = Config {
        webhook: vec![webhook("bridge"), webhook("research")],
        default_webhook: Some("bridge".into()),
        ..Config::default()
    };
    settings.apply(&mut config);
    config
}

#[test]
fn values_step_through_presets_and_snap_from_custom_values() {
    assert_eq!(step(120, &[0, 60, 120, 300], true), 300);
    assert_eq!(step(120, &[0, 60, 120, 300], false), 60);
    assert_eq!(step(300, &[0, 60, 120, 300], true), 300, "stays at the top");
    assert_eq!(step(90, &[0, 60, 120, 300], true), 120, "custom value snaps up");
    assert_eq!(step(90, &[0, 60, 120, 300], false), 60, "custom value snaps down");
    assert_eq!(cycle(300, &[0, 60, 120, 300]), 0, "space wraps around");
}

#[test]
fn edits_land_in_the_config_and_untouched_settings_stay() {
    let mut ui = settings(Scope::default());
    tap(&mut ui, Row::AutoOn, Key::Toggle);
    tap(&mut ui, Row::Toast, Key::Toggle);
    tap(&mut ui, Row::MinTurn, Key::Left);
    tap(&mut ui, Row::TailLines, Key::Right);
    tap(&mut ui, Row::Webhook(1), Key::Toggle);
    let config = saved(&ui);
    let expected = Auto {
        min_turn_seconds: 60,
        tail_lines: 80,
        toast: false,
        ..Auto::default()
    };
    assert_eq!(config.auto, expected);
    let mut state = State::default();
    ui.apply_state(&mut state);
    assert!(state.auto && !state.away);
    tap(&mut ui, Row::Away, Key::Toggle);
    ui.apply_state(&mut state);
    assert!(state.away);
    assert_eq!(config.default_webhook.as_deref(), Some("research"));
    assert!(config.scope.is_everyone());
}

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

#[test]
fn workspace_and_agent_lists_show_only_for_only_these() {
    let mut ui = settings(Scope::default());
    assert!(!screen_text(&ui).contains("builder"));
    tap(&mut ui, Row::OnlyThese, Key::Down);
    assert_eq!(ui.current(), Some(Row::Webhook(0)), "hidden rows are not cursor stops");
    tap(&mut ui, Row::OnlyThese, Key::Toggle);
    tap(&mut ui, Row::OnlyThese, Key::Down);
    assert_eq!(ui.current(), Some(Row::Workspace(0)));
    tap(&mut ui, Row::Workspace(0), Key::Toggle);
    tap(&mut ui, Row::Everyone, Key::Toggle);
    assert!(!screen_text(&ui).contains("Base"));
    tap(&mut ui, Row::OnlyThese, Key::Toggle);
    assert!(screen_text(&ui).contains("[x] Base"), "ticks survive hiding the lists");
}

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
fn only_these_with_nothing_ticked_does_not_save() {
    let mut ui = settings(Scope::default());
    tap(&mut ui, Row::OnlyThese, Key::Toggle);
    assert_eq!(ui.handle(Key::Save), Outcome::Continue);
    assert!(ui.status.as_deref().unwrap().contains("Tick at least one"));
    tap(&mut ui, Row::Agent(1), Key::Toggle);
    assert_eq!(ui.handle(Key::Save), Outcome::Save);
}

#[test]
fn quitting_with_changes_asks_once_more() {
    let mut ui = settings(Scope::default());
    assert_eq!(ui.handle(Key::Quit), Outcome::Quit, "nothing changed");
    tap(&mut ui, Row::Toast, Key::Toggle);
    assert_eq!(ui.handle(Key::Quit), Outcome::Continue);
    assert_eq!(ui.handle(Key::Quit), Outcome::Quit);
    tap(&mut ui, Row::Toast, Key::Toggle);
    assert_eq!(ui.handle(Key::Quit), Outcome::Continue);
    assert_eq!(ui.handle(Key::Down), Outcome::Continue);
    assert_eq!(
        ui.handle(Key::Quit),
        Outcome::Continue,
        "another key disarms the discard"
    );
}

#[test]
fn test_pings_only_from_a_webhook_row_and_webhooks_show_no_url_path() {
    let mut ui = settings(Scope::default());
    assert_eq!(press_on(&mut ui, Row::AutoOn, Key::Test), Outcome::Continue);
    assert_eq!(
        press_on(&mut ui, Row::Webhook(1), Key::Test),
        Outcome::Test("research".into())
    );
    let text = screen_text(&ui);
    assert!(
        text.contains("research.example") && !text.contains("secret-path"),
        "{text}"
    );
}

#[test]
fn the_cursor_stays_on_selectable_rows() {
    let mut ui = settings(Scope::default());
    assert_eq!(ui.handle(Key::Up), Outcome::Continue);
    assert_eq!(ui.current(), Some(Row::AutoOn));
    for _ in 0..100 {
        assert_eq!(ui.handle(Key::Down), Outcome::Continue);
    }
    assert_eq!(ui.current(), Some(Row::Webhook(1)));
}
