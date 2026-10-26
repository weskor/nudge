use super::{Key, Outcome};
use crate::config::Scope;
use crate::settings::rows::Row;
use crate::settings::tests::{press_on, screen_text, settings, tap};

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
