//! The editors' text styles. Colors are the terminal's own palette, so they follow its theme.
use ratatui::style::{Color, Modifier, Style};

pub(crate) const HEADER: Style = Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD);
pub(crate) const VALUE: Style = Style::new().fg(Color::Cyan);
pub(super) const ON: Style = Style::new().fg(Color::Green).add_modifier(Modifier::BOLD);
pub(crate) const GOOD: Style = Style::new().fg(Color::Green);
pub(crate) const BAD: Style = Style::new().fg(Color::Red);
pub(crate) const WARN: Style = Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD);
pub(crate) const DIM: Style = Style::new().add_modifier(Modifier::DIM);
pub(super) const KEY: Style = Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD);
pub(super) const BORDER: Style = Style::new().fg(Color::DarkGray);
/// The row under the cursor: one plain reversed bar, whatever colors its parts have.
pub(super) const CURSOR: Style = Style::new()
    .fg(Color::Reset)
    .bg(Color::Reset)
    .add_modifier(Modifier::REVERSED)
    .add_modifier(Modifier::BOLD)
    .remove_modifier(Modifier::DIM);
