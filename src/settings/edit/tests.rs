use crate::config::{Auto, Scope};
use crate::settings::keys::Key;
use crate::settings::rows::Row;
use crate::settings::tests::{saved, settings, tap};
use crate::state::State;

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
