//! Lines the editors build their screens from: check and radio marks, toggle rows, rows lined up on their names,
//! and the note under the list.
use std::borrow::Cow;

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use super::style::{DIM, ON};

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
