//! Step 4: the test ping, saving, and what to do when either fails.
use super::Wizard;
use super::field::{Field, problem};
use super::input::{Key, Outcome};
use super::step::{At, FailChoice, SaveState, Step};
use crate::error::{Blame, Error, WebhookError};
use crate::setup::ping::explain;

impl Wizard {
    /// Where step 4 stands, on step 4.
    pub(in crate::setup) const fn save(&self) -> Option<&SaveState> {
        match &self.at {
            At::Save(save) => Some(save),
            At::Connect { .. } | At::Name { .. } | At::Options { .. } => None,
        }
    }

    /// Step 4: start the test ping, or save at once with `--no-test`.
    pub(super) fn enter_save(&mut self) -> Outcome {
        match self.webhook() {
            Ok(webhook) => {
                self.go(Step::Save);
                if self.test {
                    self.at = At::Save(SaveState::Testing { frame: 0 });
                    Outcome::Test(webhook)
                } else {
                    self.saving(None)
                }
            }
            Err(error) => {
                self.status = Some(format!("✗ {}", problem(&error.to_string())));
                Outcome::Continue
            }
        }
    }

    fn saving(&mut self, tested: Option<u16>) -> Outcome {
        self.status = None;
        self.at = At::Save(SaveState::Saving { tested });
        Outcome::Save { tested }
    }

    /// The test ping's result: save on a pass; otherwise offer the way out the failure points to.
    pub(in crate::setup) fn tested(&mut self, result: Result<u16, WebhookError>) -> Outcome {
        match result {
            Ok(status) => self.saving(Some(status)),
            Err(error) => {
                let blame = error.blames();
                self.status = Some(format!("✗ {}", explain(&error)));
                if let Some(blame) = blame {
                    let field = match blame {
                        Blame::Url => Field::Url,
                        Blame::Authorization => Field::Authorization,
                    };
                    self.entry_mut(field).problem = Some(explain(&error));
                }
                let choice = if blame.is_some() {
                    FailChoice::Back
                } else {
                    FailChoice::Retry
                };
                self.at = At::Save(SaveState::Failed { blame, choice });
                Outcome::Continue
            }
        }
    }

    /// Saving failed: stay on step 4 with every answer, and let Enter try again.
    pub(in crate::setup) fn not_saved(&mut self, error: &Error) {
        let tested = match self.save() {
            Some(SaveState::Saving { tested } | SaveState::NotSaved { tested }) => *tested,
            Some(SaveState::Testing { .. } | SaveState::Failed { .. }) | None => None,
        };
        let reason = error.to_string();
        let reason = reason.lines().next().unwrap_or_default();
        self.status = Some(format!("✗ Not saved: {reason}"));
        self.at = At::Save(SaveState::NotSaved { tested });
    }

    /// Turn the spinner while the test ping runs.
    pub(in crate::setup) const fn tick(&mut self) {
        if let At::Save(SaveState::Testing { frame }) = &mut self.at {
            *frame = frame.wrapping_add(1);
        }
    }

    /// Step 4: wait for the test ping or the save, then pick what to do about a failure.
    pub(super) fn handle_save(&mut self, key: &Key) -> Outcome {
        let Some(save) = self.save().cloned() else {
            return Outcome::Continue;
        };
        match save {
            // While the ping or the save runs, only Ctrl-C does anything.
            SaveState::Testing { .. } | SaveState::Saving { .. } => Outcome::Continue,
            SaveState::NotSaved { tested } => match key {
                Key::Enter => self.saving(tested),
                Key::Char('q') => self.leave("q"),
                Key::Esc | Key::BackTab => {
                    self.back();
                    Outcome::Continue
                }
                Key::Up
                | Key::Down
                | Key::Tab
                | Key::Char(_)
                | Key::Backspace
                | Key::Clear
                | Key::Paste(_)
                | Key::Cancel => Outcome::Continue,
            },
            SaveState::Failed { blame, choice } => self.handle_failed(key, blame, choice),
        }
    }

    /// Step 4 after a failed test ping: Try again, Back to step 1, or Save anyway.
    fn handle_failed(&mut self, key: &Key, blame: Option<Blame>, choice: FailChoice) -> Outcome {
        let index = FailChoice::ALL
            .iter()
            .position(|candidate| *candidate == choice)
            .unwrap_or(0);
        let pick = |choice: FailChoice| At::Save(SaveState::Failed { blame, choice });
        match key {
            Key::Up => self.at = pick(FailChoice::ALL.get(index.saturating_sub(1)).copied().unwrap_or(choice)),
            Key::Down | Key::Tab => self.at = pick(FailChoice::ALL.get(index + 1).copied().unwrap_or(choice)),
            Key::Char('q') => return self.leave("q"),
            Key::Esc | Key::BackTab => self.back(),
            Key::Enter => match choice {
                FailChoice::Retry => {
                    self.status = None;
                    return self.enter_save();
                }
                FailChoice::Back => {
                    let field = match blame {
                        Some(Blame::Authorization) => Field::Authorization,
                        Some(Blame::Url) | None => Field::Url,
                    };
                    self.go(Step::Connect);
                    self.set_focus(field);
                    // The reason stays on the field (and in the note while it has the focus).
                    self.status = None;
                }
                FailChoice::SaveAnyway => return self.saving(None),
            },
            Key::Char(_) | Key::Backspace | Key::Clear | Key::Paste(_) | Key::Cancel => {}
        }
        Outcome::Continue
    }
}

#[cfg(test)]
mod tests;
