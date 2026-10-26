//! What each key does: leaving on a second press, typing on steps 1 and 2, and the step-3 summary and checkboxes.
//! Step 4's keys are with the rest of step 4 (`save`).
use super::field::Field;
use super::input::{Key, Outcome};
use super::step::{At, Choice, Step};
use super::{Edits, Wizard};

impl Wizard {
    pub(in crate::setup) fn handle(&mut self, key: Key) -> Outcome {
        if key == Key::Cancel {
            return Outcome::Quit;
        }
        let step = self.step();
        if self.edits == Edits::LeaveOnNextPress {
            let again = matches!((&key, step), (Key::Esc | Key::BackTab, Step::Connect))
                || (key == Key::Char('q') && matches!(step, Step::Options | Step::Save));
            if again {
                return Outcome::Quit;
            }
            self.edits = Edits::Unsaved;
            self.status = None;
        }
        match step {
            Step::Connect | Step::Name => self.handle_text(key),
            Step::Options => self.handle_options(&key),
            Step::Save => self.handle_save(&key),
        }
    }

    /// Steps 1 and 2: typing goes into the focused field.
    fn handle_text(&mut self, key: Key) -> Outcome {
        let Some(field) = self.focus() else {
            return Outcome::Continue;
        };
        let connect = self.step() == Step::Connect;
        match key {
            Key::Enter if connect && self.entry(field).value.is_empty() => {
                self.status = Some("Reading the clipboard…".into());
                return Outcome::ReadClipboard(field);
            }
            Key::Enter if field == Field::Url => {
                self.check_field(field);
                if self.answers.url.problem.is_none() && self.answers.authorization.value.is_empty() {
                    self.set_focus(Field::Authorization);
                } else if self.answers.url.problem.is_none() {
                    return self.advance();
                }
            }
            Key::Enter => {
                self.check_field(field);
                if self.entry(field).problem.is_none() {
                    return self.advance();
                }
            }
            Key::Tab | Key::Down => self.move_focus(true),
            Key::Up => self.move_focus(false),
            Key::Esc | Key::BackTab if connect => return self.leave("Esc"),
            Key::Esc | Key::BackTab => self.back(),
            Key::Paste(text) => self.paste(field, text.trim()),
            Key::Char(typed) => self.edit(field, |value| value.push(typed)),
            Key::Backspace => self.edit(field, |value| {
                value.pop();
            }),
            Key::Clear => self.edit(field, String::clear),
            Key::Cancel => return Outcome::Quit,
        }
        Outcome::Continue
    }

    /// Step 3: a summary that Enter accepts, or checkboxes after `e`.
    fn handle_options(&mut self, key: &Key) -> Outcome {
        let offered = self.offered();
        let last = offered.len().saturating_sub(1);
        match (key, self.editing()) {
            (Key::Char('q'), Some(_) | None) => return self.leave("q"),
            (Key::Enter | Key::Esc | Key::BackTab, Some(_)) => self.at = At::Options { editing: None },
            (Key::Enter, None) => return self.advance(),
            (Key::Char('e'), None) if !offered.is_empty() => self.at = At::Options { editing: Some(0) },
            (Key::Esc | Key::BackTab, None) => self.back(),
            (Key::Up, Some(cursor)) => {
                self.at = At::Options {
                    editing: Some(cursor.saturating_sub(1)),
                };
            }
            (Key::Down | Key::Tab, Some(cursor)) => {
                self.at = At::Options {
                    editing: Some((cursor + 1).min(last)),
                };
            }
            (Key::Char(' '), Some(cursor)) => {
                let choices = &mut self.answers.choices;
                if let Some(choice) = offered.get(cursor) {
                    match choice {
                        Choice::AutoNotify => choices.auto_notify = choices.auto_notify.map(|on| !on),
                        Choice::Command => choices.link_command = !choices.link_command,
                        Choice::Default => choices.make_default = !choices.make_default,
                    }
                    self.edits = Edits::Unsaved;
                }
            }
            (
                Key::Up
                | Key::Down
                | Key::Tab
                | Key::Char(_)
                | Key::Backspace
                | Key::Clear
                | Key::Paste(_)
                | Key::Cancel,
                Some(_) | None,
            ) => {}
        }
        Outcome::Continue
    }
}
