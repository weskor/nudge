//! The look shared by the full-screen editors (`nudge settings`, `nudge setup`): a centered rounded box holding a
//! list, a rule, a note (the status, or what the row under the cursor does) and the key help.
use std::borrow::Cow;

use ratatui::DefaultTerminal;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, ListState, Padding, Paragraph, Wrap};

use crate::error::Result;

// Colors are the terminal's own palette, so they follow its theme.
pub(crate) const HEADER: Style = Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD);
pub(crate) const VALUE: Style = Style::new().fg(Color::Cyan);
pub(crate) const ON: Style = Style::new().fg(Color::Green).add_modifier(Modifier::BOLD);
pub(crate) const GOOD: Style = Style::new().fg(Color::Green);
pub(crate) const BAD: Style = Style::new().fg(Color::Red);
pub(crate) const WARN: Style = Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD);
pub(crate) const DIM: Style = Style::new().add_modifier(Modifier::DIM);
const KEY: Style = Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD);
const BORDER: Style = Style::new().fg(Color::DarkGray);
/// The row under the cursor: one plain reversed bar, whatever colors its parts have.
const CURSOR: Style = Style::new()
    .fg(Color::Reset)
    .bg(Color::Reset)
    .add_modifier(Modifier::REVERSED)
    .add_modifier(Modifier::BOLD)
    .remove_modifier(Modifier::DIM);

/// The box is at least this wide (and at most the screen).
const MIN_WIDTH: u16 = 70;
/// A box sized to its content is at most this wide.
const MAX_WIDTH: u16 = 96;
/// Columns the borders (2) and side padding (4) take from the box.
pub(crate) const FRAME_WIDTH: u16 = 6;

pub(crate) fn check(checked: bool) -> Span<'static> {
    if checked {
        Span::styled("[x]", ON)
    } else {
        Span::styled("[ ]", DIM)
    }
}

pub(crate) fn radio(picked: bool) -> Span<'static> {
    if picked {
        Span::styled("(•)", ON)
    } else {
        Span::styled("( )", DIM)
    }
}

/// A checkbox or radio row: `mark` then `label`, which starts with a space.
pub(crate) fn toggle(mark: Span<'static>, label: impl Into<Cow<'static, str>>) -> Line<'static> {
    Line::from(vec![Span::raw("  "), mark, Span::raw(label)])
}

/// A list row before alignment: its mark, its name, and what follows the name.
pub(crate) type ListRow = (Span<'static>, Span<'static>, Vec<Span<'static>>);

/// One line per row after `indent`, with what follows each name lined up after the widest name.
pub(crate) fn aligned(indent: &'static str, rows: Vec<ListRow>) -> Vec<Line<'static>> {
    let widest = rows.iter().map(|(_, name, _)| name.width()).max().unwrap_or(0);
    rows.into_iter()
        .map(|(mark, name, rest)| {
            let gap = " ".repeat(widest.saturating_sub(name.width()) + 2);
            let mut spans = vec![Span::raw(indent), mark, Span::raw(" "), name];
            if !rest.is_empty() {
                spans.push(Span::raw(gap));
                spans.extend(rest);
            }
            Line::from(spans)
        })
        .collect()
}

/// The status if there is one (✓ green, ✗ red, anything else yellow), else `hint`, dimmed.
pub(crate) fn note(status: Option<&str>, hint: &str) -> Line<'static> {
    let Some(status) = status else {
        return Line::styled(hint.to_owned(), DIM);
    };
    let color = if status.starts_with('✓') {
        Color::Green
    } else if status.starts_with('✗') {
        Color::Red
    } else {
        Color::Yellow
    };
    Line::styled(status.to_owned(), Style::new().fg(color).add_modifier(Modifier::BOLD))
}

/// One frame of an editor.
#[derive(Debug)]
pub(crate) struct View<'a> {
    pub(crate) title: &'a str,
    /// Show `● unsaved` at the top right.
    pub(crate) unsaved: bool,
    pub(crate) lines: Vec<Line<'static>>,
    /// The line the cursor is on.
    pub(crate) selected: usize,
    /// Where on that line to show the terminal's text cursor, for typing; `None` hides it.
    pub(crate) caret: Option<u16>,
    pub(crate) note: Line<'static>,
    /// Keys and what they do, for the help line.
    pub(crate) help: &'a [(&'a str, &'a str)],
    /// The box's width; `None` sizes it to its widest line.
    pub(crate) width: Option<u16>,
}

pub(crate) fn draw(terminal: &mut DefaultTerminal, view: View<'_>) -> Result<()> {
    let View {
        title,
        unsaved,
        lines,
        selected,
        caret,
        note,
        help,
        width,
    } = view;
    let mut keys = Vec::new();
    for (index, (key, what)) in help.iter().enumerate() {
        if index > 0 {
            keys.push(Span::raw("  "));
        }
        keys.extend([Span::styled(*key, KEY), Span::styled(format!(" {what}"), DIM)]);
    }
    let help = Line::from(keys);
    let note = Paragraph::new(note).wrap(Wrap { trim: true });
    terminal.draw(|frame| {
        let screen = frame.area();
        let width = width
            .unwrap_or_else(|| {
                let widest = lines.iter().map(Line::width).chain([help.width()]).max().unwrap_or(0);
                u16::try_from(widest)
                    .unwrap_or(u16::MAX)
                    .saturating_add(FRAME_WIDTH)
                    .clamp(MIN_WIDTH, MAX_WIDTH)
            })
            .max(MIN_WIDTH)
            .min(screen.width);
        // The note keeps two rows so the box doesn't jump as the cursor moves, and wraps to up to four so long
        // errors and their fixes stay readable in narrow overlays.
        let note_rows = note.line_count(width.saturating_sub(FRAME_WIDTH)).clamp(2, 4);
        // Borders and top padding (3), the list, the rule, the note and the help line.
        let height = u16::try_from(lines.len() + note_rows + 5)
            .unwrap_or(u16::MAX)
            .min(screen.height);
        let area = screen.centered(Constraint::Length(width), Constraint::Length(height));
        let mut block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(BORDER)
            .padding(Padding::new(2, 2, 1, 0))
            .title_top(Line::styled(format!(" {title} "), HEADER));
        if unsaved {
            block = block.title_top(Line::styled(" ● unsaved ", WARN).right_aligned());
        }
        let inner = block.inner(area);
        frame.render_widget(block, area);
        let [body, rule, note_area, help_area] = Layout::vertical([
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(u16::try_from(note_rows).unwrap_or(2)),
            Constraint::Length(1),
        ])
        .areas(inner);
        let list = List::new(lines.into_iter().map(ListItem::new)).highlight_style(CURSOR);
        let mut state = ListState::default().with_selected(Some(selected));
        frame.render_stateful_widget(list, body, &mut state);
        let row = selected.saturating_sub(state.offset());
        if let (Some(column), Ok(row)) = (caret, u16::try_from(row))
            && row < body.height
            && column < body.width
        {
            frame.set_cursor_position((body.x + column, body.y + row));
        }
        frame.render_widget(Block::new().borders(Borders::TOP).border_style(BORDER), rule);
        frame.render_widget(note, note_area);
        frame.render_widget(help, help_area);
    })?;
    Ok(())
}
