use super::Row;
use crate::config::Scope;
use crate::settings::keys::{Key, Outcome};
use crate::settings::tests::{screen_text, settings, tap};

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
fn the_cursor_stays_on_selectable_rows() {
    let mut ui = settings(Scope::default());
    assert_eq!(ui.handle(Key::Up), Outcome::Continue);
    assert_eq!(ui.current(), Some(Row::AutoOn));
    for _ in 0..100 {
        assert_eq!(ui.handle(Key::Down), Outcome::Continue);
    }
    assert_eq!(ui.current(), Some(Row::Webhook(1)));
}
