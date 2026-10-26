//! The box's size and place: its width (sized to the content within limits), the rows its note takes, and where it
//! sits centered on the screen.
use ratatui::layout::{Constraint, Rect};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

/// The box is at least this wide (and at most the screen).
const MIN_WIDTH: u16 = 70;
/// A box sized to its content is at most this wide.
const MAX_WIDTH: u16 = 96;
/// Columns the borders (2) and side padding (4) take from the box.
pub(super) const FRAME_WIDTH: u16 = 6;

/// The box's width: `width` if given, else the widest of `lines` and `help` plus the frame, within the limits; never
/// wider than the screen.
pub(super) fn box_width(width: Option<u16>, lines: &[Line<'_>], help: &Line<'_>, screen: u16) -> u16 {
    width
        .unwrap_or_else(|| {
            let widest = lines.iter().map(Line::width).chain([help.width()]).max().unwrap_or(0);
            u16::try_from(widest)
                .unwrap_or(u16::MAX)
                .saturating_add(FRAME_WIDTH)
                .clamp(MIN_WIDTH, MAX_WIDTH)
        })
        .max(MIN_WIDTH)
        .min(screen)
}

/// Rows for the note in a box `width` wide. It keeps two rows so the box doesn't jump as the cursor moves, and wraps
/// to up to four so long errors and their fixes stay readable in narrow overlays.
pub(super) fn note_rows(note: &Paragraph<'_>, width: u16) -> usize {
    note.line_count(width.saturating_sub(FRAME_WIDTH)).clamp(2, 4)
}

/// The box centered on `screen`, `width` wide and tall enough for `rows` of list and `note_rows` of note.
pub(super) fn box_area(screen: Rect, width: u16, rows: usize, note_rows: usize) -> Rect {
    // Borders and top padding (3), the list (or the step list, if longer), the rule, the note and the help line.
    let height = u16::try_from(rows + note_rows + 5)
        .unwrap_or(u16::MAX)
        .min(screen.height);
    screen.centered(Constraint::Length(width), Constraint::Length(height))
}
