//! Drawing one frame of an editor: the box and its title, the list (beside a wizard's step list) with the cursor bar
//! and caret, the rule, the note and the key help.
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, ListState, Padding, Paragraph, Wrap};
use ratatui::{DefaultTerminal, Frame};

use super::layout::{box_area, box_width, note_rows};
use super::steps::{StepMark, render_steps};
use super::style::{BORDER, CURSOR, DIM, HEADER, KEY, WARN};
use crate::error::Result;

/// One frame of an editor.
#[derive(Debug)]
pub(crate) struct View<'a> {
    pub(crate) title: &'a str,
    /// Show `● unsaved` at the top right.
    pub(crate) unsaved: bool,
    /// A wizard's steps, listed left of the content.
    pub(crate) steps: Option<&'a [(&'a str, StepMark)]>,
    pub(crate) lines: Vec<Line<'static>>,
    /// The line the cursor is on, if any.
    pub(crate) selected: Option<usize>,
    /// Where on that line to show the terminal's text cursor, for typing; `None` hides it.
    pub(crate) caret: Option<u16>,
    pub(crate) note: Line<'static>,
    /// Keys and what they do, for the help line.
    pub(crate) help: &'a [(&'a str, &'a str)],
    /// The box's width; `None` sizes it to its widest line.
    pub(crate) width: Option<u16>,
}

/// The help line: each key, then what it does, dimmed.
fn help_line<'a>(help: &[(&'a str, &str)]) -> Line<'a> {
    let mut keys = Vec::new();
    for (index, (key, what)) in help.iter().enumerate() {
        if index > 0 {
            keys.push(Span::raw("  "));
        }
        keys.extend([Span::styled(*key, KEY), Span::styled(format!(" {what}"), DIM)]);
    }
    Line::from(keys)
}

/// Draw the rounded box with its title (and `● unsaved`) in `area`, and split its inside into the body, the rule,
/// `note_rows` of note and the help line.
fn render_box(frame: &mut Frame<'_>, area: Rect, title: &str, unsaved: bool, note_rows: usize) -> [Rect; 4] {
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
    Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(u16::try_from(note_rows).unwrap_or(2)),
        Constraint::Length(1),
    ])
    .areas(inner)
}

/// Draw `lines` in `area` with the cursor bar on `selected`, and the terminal's text cursor at column `caret` of that
/// line when it is in view.
fn render_list(
    frame: &mut Frame<'_>,
    area: Rect,
    lines: Vec<Line<'static>>,
    selected: Option<usize>,
    caret: Option<u16>,
) {
    let list = List::new(lines.into_iter().map(ListItem::new)).highlight_style(CURSOR);
    let mut state = ListState::default().with_selected(selected);
    frame.render_stateful_widget(list, area, &mut state);
    if let (Some(column), Some(selected)) = (caret, selected)
        && let Ok(row) = u16::try_from(selected.saturating_sub(state.offset()))
        && row < area.height
        && column < area.width
    {
        frame.set_cursor_position((area.x + column, area.y + row));
    }
}

pub(crate) fn draw(terminal: &mut DefaultTerminal, view: View<'_>) -> Result<()> {
    let View {
        title,
        unsaved,
        steps,
        lines,
        selected,
        caret,
        note,
        help,
        width,
    } = view;
    let help = help_line(help);
    let note = Paragraph::new(note).wrap(Wrap { trim: true });
    terminal.draw(|frame| {
        let screen = frame.area();
        let width = box_width(width, &lines, &help, screen.width);
        let note_rows = note_rows(&note, width);
        let rows = lines.len().max(steps.map_or(0, <[_]>::len));
        let area = box_area(screen, width, rows, note_rows);
        let [body, rule, note_area, help_area] = render_box(frame, area, title, unsaved, note_rows);
        let content = steps.map_or(body, |steps| render_steps(frame, body, steps));
        render_list(frame, content, lines, selected, caret);
        frame.render_widget(Block::new().borders(Borders::TOP).border_style(BORDER), rule);
        frame.render_widget(note, note_area);
        frame.render_widget(help, help_area);
    })?;
    Ok(())
}
