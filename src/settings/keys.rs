//! What a key press does: the terminal's keys mapped to the editor's, and each key's effect on the settings.
use ratatui::crossterm::event::{KeyCode, KeyModifiers};

use super::rows::Row;
use super::state::{Edits, Settings};

/// What the user pressed, independent of the terminal library.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Key {
    Up,
    Down,
    Left,
    Right,
    Toggle,
    Test,
    Save,
    Quit,
}

/// What the UI loop should do after a key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    Continue,
    Save,
    Quit,
    /// Send a test ping to this webhook.
    Test(String),
}

pub(super) fn key(code: KeyCode, modifiers: KeyModifiers) -> Option<Key> {
    if modifiers.contains(KeyModifiers::CONTROL) {
        return (code == KeyCode::Char('c')).then_some(Key::Quit);
    }
    #[expect(clippy::wildcard_enum_match_arm, reason = "every other key does nothing")]
    match code {
        KeyCode::Up | KeyCode::Char('k') => Some(Key::Up),
        KeyCode::Down | KeyCode::Char('j') => Some(Key::Down),
        KeyCode::Left | KeyCode::Char('h' | '-') => Some(Key::Left),
        KeyCode::Right | KeyCode::Char('l' | '+') => Some(Key::Right),
        KeyCode::Enter | KeyCode::Char(' ') => Some(Key::Toggle),
        KeyCode::Char('t') => Some(Key::Test),
        KeyCode::Char('s') => Some(Key::Save),
        KeyCode::Esc | KeyCode::Char('q') => Some(Key::Quit),
        _ => None,
    }
}

impl Settings {
    pub(crate) fn handle(&mut self, key: Key) -> Outcome {
        let armed = self.edits == Edits::DiscardOnNextQuit;
        if armed {
            self.edits = Edits::Unsaved;
        }
        let last = self.rows().len().saturating_sub(1);
        match (key, self.current()) {
            (Key::Up, _) => self.cursor = self.cursor.saturating_sub(1),
            (Key::Down, _) => self.cursor = (self.cursor + 1).min(last),
            (Key::Left | Key::Right, Some(row)) => self.adjust(row, key == Key::Right),
            (Key::Toggle, Some(row)) => self.toggle(row),
            (Key::Test, Some(Row::Webhook(index))) => {
                if let Some(webhook) = self.webhooks.get(index) {
                    return Outcome::Test(webhook.name.clone());
                }
            }
            (Key::Test, _) => self.status = Some("Move to a webhook to test it.".into()),
            (Key::Save, _) => {
                if self.only && !self.workspaces.iter().chain(&self.agents).any(|choice| choice.checked) {
                    self.status = Some("Tick at least one workspace or agent, or choose Every agent.".into());
                } else {
                    return Outcome::Save;
                }
            }
            (Key::Quit, _) => {
                if self.edits == Edits::None || armed {
                    return Outcome::Quit;
                }
                self.edits = Edits::DiscardOnNextQuit;
                self.status = Some("Unsaved changes: press q again to discard them, or s to save.".into());
            }
            (Key::Left | Key::Right | Key::Toggle, None) => {}
        }
        Outcome::Continue
    }
}

#[cfg(test)]
mod tests;
