//! The editor's lines, section by section, each tagged with the row it is (if the cursor can stop there).
use ratatui::style::Style;
use ratatui::text::{Line, Span};

use super::choices::Choice;
use super::rows::Row;
use super::state::Settings;
use crate::config::seconds_in_words;
use crate::terminal::ui::{DIM, GOOD, HEADER, VALUE, WARN, aligned, check, radio, toggle};

/// A line to draw, and the row it is if the cursor can stop on it.
type Item = (Option<Row>, Line<'static>);

/// A titled list of choices in the "Who can notify" section, and the row each entry is on.
type Section<'a> = (&'a str, &'a [Choice], fn(usize) -> Row);

/// A section's dimmed title, then its choices lined up on their names (or a note that none are open).
fn section_items((label, list, row): Section<'_>) -> Vec<Item> {
    let mut items = vec![(None, Line::styled(format!("      {label}"), DIM))];
    if list.is_empty() {
        items.push((None, Line::styled("        none open in herdr", DIM)));
    }
    let rows = list
        .iter()
        .map(|choice| {
            let name = Span::styled(choice.name.clone(), if choice.live { Style::new() } else { DIM });
            let detail = Some(&choice.detail)
                .filter(|detail| !detail.is_empty())
                .map(|detail| Span::styled(detail.clone(), DIM));
            (check(choice.checked), name, detail.into_iter().collect())
        })
        .collect();
    let lines = aligned("        ", rows).into_iter().enumerate();
    items.extend(lines.map(|(index, line)| (Some(row(index)), line)));
    items
}

impl Settings {
    /// The lines to draw, and which one the cursor is on.
    pub(super) fn lines(&self) -> (Vec<Line<'static>>, usize) {
        let cursor_row = self.current();
        let mut items = self.mode_items();
        items.extend(self.auto_items(cursor_row));
        items.extend(self.scope_items());
        items.extend(self.webhook_items());
        let selected = items
            .iter()
            .position(|(row, _)| row.is_some() && *row == cursor_row)
            .unwrap_or(0);
        (items.into_iter().map(|(_, line)| line).collect(), selected)
    }

    /// "Mode": the auto-notify and away switches.
    fn mode_items(&self) -> Vec<Item> {
        let away = if self.away {
            Span::styled("[x]", WARN)
        } else {
            check(false)
        };
        vec![
            (None, Line::styled("Mode", HEADER)),
            (Some(Row::AutoOn), toggle(check(self.auto_on), " Auto-notify")),
            (Some(Row::Away), toggle(away, " I'm away")),
            (None, Line::default()),
        ]
    }

    /// "Automatic notifications": the toast switch, then the values, with ◂ ▸ on the one under the cursor.
    fn auto_items(&self, cursor_row: Option<Row>) -> Vec<Item> {
        let mut items = vec![
            (None, Line::styled("Automatic notifications", HEADER)),
            (
                Some(Row::Toast),
                toggle(check(self.auto.toast), " Toast when one is sent"),
            ),
        ];
        // Values line up with the checkbox labels; only the row under the cursor shows the ◂ ▸ that change it.
        for (row, label, text) in [
            (
                Row::MinTurn,
                "Shortest turn sent",
                seconds_in_words(self.auto.min_turn_seconds),
            ),
            (
                Row::Cooldown,
                "Cooldown per pane",
                seconds_in_words(self.auto.cooldown_seconds),
            ),
            (Row::TailLines, "Terminal lines sent", self.auto.tail_lines.to_string()),
        ] {
            let (before, after) = if cursor_row == Some(row) {
                ("◂ ", " ▸")
            } else {
                ("  ", "  ")
            };
            let line = Line::from(vec![
                Span::raw(format!("      {label:<22}{before}")),
                Span::styled(text, VALUE),
                Span::raw(after),
            ]);
            items.push((Some(row), line));
        }
        items
    }

    /// "Who can notify": every agent or only these, and for only these the workspace and agent lists.
    fn scope_items(&self) -> Vec<Item> {
        let only = if self.only { " Only these:" } else { " Only these…" };
        let mut items = vec![
            (None, Line::default()),
            (None, Line::styled("Who can notify", HEADER)),
            (Some(Row::Everyone), toggle(radio(!self.only), " Every agent")),
            (Some(Row::OnlyThese), toggle(radio(self.only), only)),
        ];
        if self.only {
            let sections: [Section<'_>; 2] = [
                ("Workspaces", &self.workspaces, Row::Workspace),
                ("Agents", &self.agents, Row::Agent),
            ];
            items.extend(sections.into_iter().flat_map(section_items));
        }
        items
    }

    /// "Default webhook": one radio row per webhook with its host, and whether it is signed.
    fn webhook_items(&self) -> Vec<Item> {
        let mut items = vec![(None, Line::default()), (None, Line::styled("Default webhook", HEADER))];
        let rows = self
            .webhooks
            .iter()
            .map(|webhook| {
                let is_default = self.default_webhook.as_deref() == Some(webhook.name.as_str());
                let mut rest = vec![Span::styled(webhook.host.clone(), DIM)];
                if webhook.signed {
                    rest.push(Span::styled("  signed", GOOD));
                }
                (radio(is_default), Span::raw(webhook.name.clone()), rest)
            })
            .collect();
        let lines = aligned("  ", rows).into_iter().enumerate();
        items.extend(lines.map(|(index, line)| (Some(Row::Webhook(index)), line)));
        items
    }
}

#[cfg(test)]
mod tests;
