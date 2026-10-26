//! The setup wizard as pure state: four steps, the answers given so far, and what each key does. No terminal, no
//! files, no network: the loop in `view` reads keys and the clipboard, runs the test ping and draws. This file holds
//! the wizard itself and where it starts; its methods live with what they're about.
mod answers;
mod clipboard;
mod field;
mod hints;
mod input;
mod keys;
mod navigation;
mod save;
mod step;

use std::fmt;

use self::answers::Answers;
use self::field::Entry;
pub(super) use self::field::{Field, Source, Sourced};
pub(super) use self::input::{Key, Outcome};
use self::step::At;
pub(super) use self::step::{Choice, FailChoice, SaveState, Step};
use super::choices::Choices;
use crate::terminal::tty::Redacted;

/// Whether there is anything to lose on quit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Edits {
    None,
    Unsaved,
    /// Leave was pressed once with something entered; pressing it again leaves.
    LeaveOnNextPress,
}

/// Where the wizard starts: flags, the environment and the saved webhook, and what herdr and PATH look like.
/// `Debug` leaves out the values: the header and signing secret are among them.
pub(super) struct Start {
    pub(super) name: String,
    /// The saved webhook this run updates, if `name` is one.
    pub(super) loaded: Option<String>,
    pub(super) url: Sourced,
    pub(super) authorization: Sourced,
    pub(super) description: Option<String>,
    /// The names of the saved webhooks, and which one is the default.
    pub(super) saved: Vec<String>,
    pub(super) default_webhook: Option<String>,
    pub(super) choices: Choices,
    /// The shortest turn auto-notify sends, in words.
    pub(super) min_turn: String,
    pub(super) signing_secret: Option<String>,
    /// Test-ping before saving (off with `--no-test`).
    pub(super) test: bool,
}

impl fmt::Debug for Start {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Start")
            .field("name", &self.name)
            .field("loaded", &self.loaded)
            .field("test", &self.test)
            .finish_non_exhaustive()
    }
}

/// The wizard: every answer so far ([`Answers`]), where it is ([`At`]), and what it knows about this machine.
/// `Debug` shows where the wizard is, with every value redacted (the header and signing secret are among them).
pub(super) struct Wizard {
    at: At,
    /// The furthest step reached; the steps before it are done.
    reached: Step,
    answers: Answers,
    pub(super) loaded: Option<String>,
    saved: Vec<String>,
    pub(super) default_webhook: Option<String>,
    min_turn: String,
    signing_secret: Option<String>,
    test: bool,
    edits: Edits,
    status: Option<String>,
}

impl fmt::Debug for Wizard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Wizard")
            .field("at", &self.at)
            .field("answers", &self.answers)
            .field("signing_secret", &self.signing_secret.as_deref().map(Redacted))
            .finish_non_exhaustive()
    }
}

impl Wizard {
    pub(super) fn new(start: Start) -> Self {
        Self {
            at: At::start(Step::Connect),
            reached: Step::Connect,
            answers: Answers {
                url: Entry::given(start.url),
                authorization: Entry::given(start.authorization),
                // Not checked yet: `--webhook` is checked when the step is left, with the duplicate check.
                name: Entry {
                    value: start.name,
                    ..Entry::default()
                },
                description: Entry::given(start.description.map(|text| (text, Source::Flag))),
                choices: start.choices,
            },
            loaded: start.loaded,
            saved: start.saved,
            default_webhook: start.default_webhook,
            min_turn: start.min_turn,
            signing_secret: start.signing_secret,
            test: start.test,
            edits: Edits::None,
            status: None,
        }
    }
}

#[cfg(test)]
mod tests;
