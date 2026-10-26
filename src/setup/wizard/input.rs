//! What goes into the wizard and what comes out: the keys it understands, and what the loop should do next.
use std::fmt;

use super::field::Field;
use crate::config::Webhook;
use crate::terminal::tty::Redacted;

/// What the user pressed, independent of the terminal library. `Debug` shows a paste's length only: it may hold
/// the header.
#[derive(Clone, PartialEq, Eq)]
pub(in crate::setup) enum Key {
    Up,
    Down,
    Tab,
    BackTab,
    Enter,
    Esc,
    Char(char),
    Backspace,
    /// Ctrl-U: empty the field.
    Clear,
    Paste(String),
    /// Ctrl-C: leave now.
    Cancel,
}

impl fmt::Debug for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Paste(text) => f.debug_tuple("Paste").field(&Redacted(text)).finish(),
            Self::Char(typed) => f.debug_tuple("Char").field(typed).finish(),
            Self::Up => f.write_str("Up"),
            Self::Down => f.write_str("Down"),
            Self::Tab => f.write_str("Tab"),
            Self::BackTab => f.write_str("BackTab"),
            Self::Enter => f.write_str("Enter"),
            Self::Esc => f.write_str("Esc"),
            Self::Backspace => f.write_str("Backspace"),
            Self::Clear => f.write_str("Clear"),
            Self::Cancel => f.write_str("Cancel"),
        }
    }
}

/// What the loop should do after a key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::setup) enum Outcome {
    Continue,
    /// Fill this step-1 field (or both, from a combined copy) from the clipboard; report back with
    /// [`super::Wizard::fill_from_clipboard`].
    ReadClipboard(Field),
    /// Start the test ping for this webhook; report back with [`super::Wizard::tested`].
    Test(Webhook),
    /// Leave the screen and save, with the test ping's HTTP status if one passed.
    Save {
        tested: Option<u16>,
    },
    Quit,
}
