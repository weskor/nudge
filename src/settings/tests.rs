//! Fixtures shared by the settings tests: a settings editor over two webhooks, two open workspaces and two agents,
//! and helpers to press keys on it and read what it shows and saves.
use super::keys::{Key, Outcome};
use super::rows::Row;
use super::state::Settings;
use crate::config::{Config, Scope, Webhook};
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

pub(super) fn settings(scope: Scope) -> Settings {
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
pub(super) fn press_on(settings: &mut Settings, row: Row, key: Key) -> Outcome {
    settings.cursor = settings.rows().iter().position(|candidate| *candidate == row).unwrap();
    settings.handle(key)
}

/// Press `key` on `row`, expecting the UI to carry on.
pub(super) fn tap(settings: &mut Settings, row: Row, key: Key) {
    assert_eq!(press_on(settings, row, key), Outcome::Continue);
}

pub(super) fn screen_text(settings: &Settings) -> String {
    let (lines, _) = settings.lines();
    lines.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n")
}

pub(super) fn saved(settings: &Settings) -> Config {
    let mut config = Config {
        webhook: vec![webhook("bridge"), webhook("research")],
        default_webhook: Some("bridge".into()),
        ..Config::default()
    };
    settings.apply(&mut config);
    config
}
