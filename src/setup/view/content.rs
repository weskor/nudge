//! What the step on screen shows: its lines, which one the cursor is on, and where typing goes.
use ratatui::text::Line;

use super::field::field_lines;
use super::options::options;
use super::save::save;
use crate::setup::wizard::{Field, Step, Wizard};

/// Lines with a cursor stop each; `selected` is where the cursor is, `caret` where typing goes on it.
#[derive(Default)]
pub(super) struct Content {
    pub(super) lines: Vec<Line<'static>>,
    pub(super) selected: Option<usize>,
    pub(super) caret: Option<usize>,
}

impl Content {
    pub(super) fn text(&mut self, lines: &[&'static str]) {
        self.lines.extend(lines.iter().map(|line| Line::raw(*line)));
        self.lines.push(Line::default());
    }

    fn fields(&mut self, wizard: &Wizard, fields: [Field; 2], room: usize) {
        for field in fields {
            let (lines, caret) = field_lines(wizard, field, room);
            if wizard.focus() == Some(field) {
                self.selected = Some(self.lines.len());
                self.caret = caret;
            }
            self.lines.extend(lines);
        }
    }

    /// A line the cursor can sit on, selected when `here`.
    pub(super) fn stop(&mut self, line: Line<'static>, here: bool) {
        if here {
            self.selected = Some(self.lines.len());
        }
        self.lines.push(line);
    }
}

/// The content for the step on screen.
pub(super) fn content(wizard: &Wizard, room: usize) -> Content {
    let mut content = Content::default();
    match wizard.step() {
        Step::Connect => {
            content.text(&[
                "Where should nudge send agent events? Copy the URL and the",
                "Authorization header from your webhook's settings: an AI routine,",
                "a workflow automation or your own service.",
            ]);
            content.fields(wizard, [Field::Url, Field::Authorization], room);
        }
        Step::Name => {
            content.text(&[
                "What should nudge call this webhook? You'll use the name with",
                "--webhook and in nudge list. With one webhook, main is fine.",
            ]);
            content.fields(wizard, [Field::Name, Field::Description], room);
        }
        Step::Options => options(wizard, &mut content, room),
        Step::Save => save(wizard, &mut content),
    }
    content
}
