//! Terminal events as the wizard's keys.
use ratatui::crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};

use crate::setup::wizard::Key;

/// The wizard key for a terminal event; `None` for events the wizard ignores.
pub(super) fn key(event: Event) -> Option<Key> {
    let pressed = match event {
        Event::Paste(text) => return Some(Key::Paste(text)),
        Event::Key(pressed) if pressed.kind == KeyEventKind::Press => pressed,
        Event::Key(_) | Event::FocusGained | Event::FocusLost | Event::Mouse(_) | Event::Resize(..) => return None,
    };
    match (pressed.modifiers.contains(KeyModifiers::CONTROL), pressed.code) {
        (true, KeyCode::Char('c')) => Some(Key::Cancel),
        (true, KeyCode::Char('u')) => Some(Key::Clear),
        (false, KeyCode::Up) => Some(Key::Up),
        (false, KeyCode::Down) => Some(Key::Down),
        (false, KeyCode::Tab) => Some(Key::Tab),
        (false, KeyCode::BackTab) => Some(Key::BackTab),
        (false, KeyCode::Enter) => Some(Key::Enter),
        (false, KeyCode::Esc) => Some(Key::Esc),
        (false, KeyCode::Backspace) => Some(Key::Backspace),
        (false, KeyCode::Char(typed)) => Some(Key::Char(typed)),
        _ => None,
    }
}
