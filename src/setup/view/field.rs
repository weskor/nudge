//! A text field on screen: its label, its value (the header masked), a placeholder, its mark and where it came from.
use ratatui::text::{Line, Span};

use crate::setup::wizard::{Field, Wizard};
use crate::terminal::tty::mask_secret;
use crate::terminal::ui::{BAD, DIM, GOOD};

/// Columns a field's label takes before its value.
pub(super) const LABEL: usize = 15;

/// `text` in at most `room` columns: its end (where typing happens) when `typing`, else its start, with `…` for the
/// rest.
pub(super) fn fit(text: &str, room: usize, typing: bool) -> String {
    let count = text.chars().count();
    if count <= room {
        return text.to_owned();
    }
    let keep = room.saturating_sub(1);
    if typing {
        format!("…{}", text.chars().skip(count - keep).collect::<String>())
    } else {
        format!("{}…", text.chars().take(keep).collect::<String>())
    }
}

/// A text field's lines (its value, then where it came from, if known) and where the text cursor goes when it's
/// focused; values get `room` columns.
pub(super) fn field_lines(wizard: &Wizard, field: Field, room: usize) -> (Vec<Line<'static>>, Option<usize>) {
    let entry = wizard.entry(field);
    let focused = wizard.focus() == Some(field);
    // A header shows only its masked preview once checked; while it's typed, only dots.
    let shown = match field {
        Field::Authorization if entry.checked => format!(
            "{}  ({} characters)",
            mask_secret(&entry.value),
            entry.value.chars().count()
        ),
        Field::Authorization => "•".repeat(entry.value.chars().count().min(room)),
        Field::Url | Field::Name | Field::Description => fit(&entry.value, room, focused),
    };
    let shown = Span::raw(shown);
    // A checked header can't be edited in place (typing replaces it), so it gets no text cursor.
    let caret = (focused && !(field == Field::Authorization && entry.checked)).then(|| LABEL + shown.width());
    let mut spans = vec![Span::raw(format!("{:<LABEL$}", field.label())), shown];
    if entry.value.is_empty() {
        let placeholder = match (field, focused) {
            (Field::Url | Field::Authorization, true) => "press Enter to paste, or type",
            (Field::Url | Field::Authorization | Field::Name, false) => "not set",
            (Field::Name, true) => "",
            (Field::Description, _) => "optional",
        };
        spans.push(Span::styled(placeholder, DIM));
    }
    if entry.problem.is_some() {
        spans.push(Span::styled("  ✗", BAD));
    } else if entry.checked && !entry.value.is_empty() && matches!(field, Field::Url | Field::Authorization) {
        spans.push(Span::styled("  ✓", GOOD));
    } else if field == Field::Name && wizard.loaded.as_ref() == Some(&entry.value) {
        spans.push(Span::styled("  saved: saving updates it", DIM));
    }
    let mut lines = vec![Line::from(spans)];
    if let Some(source) = entry
        .source
        .filter(|_| matches!(field, Field::Url | Field::Authorization))
    {
        lines.push(Line::styled(format!("{:LABEL$}{}", "", source.label()), DIM));
    }
    (lines, caret)
}
