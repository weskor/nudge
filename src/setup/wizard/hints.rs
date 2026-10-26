//! The wizard's words under the step: the status (or the focused field's problem), and what the focused element does.
use std::borrow::Cow;

use super::Wizard;
use super::field::Field;
use super::step::{At, Choice, FailChoice, SaveState};

impl Wizard {
    /// The status, or the problem with the focused field.
    pub(in crate::setup) fn status(&self) -> Option<String> {
        self.status.clone().or_else(|| {
            self.focus()
                .and_then(|field| self.entry(field).problem.as_ref())
                .map(|problem| format!("✗ {problem}"))
        })
    }

    /// What the focused element does.
    pub(in crate::setup) fn hint(&self) -> Cow<'static, str> {
        let answers = &self.answers;
        match &self.at {
            At::Connect { focus: Field::Url } if answers.url.value.is_empty() => {
                "Copy the webhook URL from your webhook's settings, then press Enter here to paste it, or type it."
                    .into()
            }
            At::Connect {
                focus: Field::Authorization,
            } if answers.authorization.value.is_empty() => {
                "Copy the Authorization header from the same page, then press Enter here to paste it. It's never shown."
                    .into()
            }
            At::Connect {
                focus: Field::Authorization,
            } if answers.authorization.checked => {
                "Never shown. To replace it, paste or type a new one; Ctrl-U empties the field.".into()
            }
            At::Connect { .. } => "Paste or type to change it; Ctrl-U empties the field. Enter goes on.".into(),
            At::Name {
                focus: Field::Description,
            } => "Optional. Shown by nudge list and to agents choosing a webhook.".into(),
            At::Name { .. } => "Letters, digits, - and _. Enter goes on; Tab moves to the description.".into(),
            At::Options { editing: Some(cursor) } => match self.offered().get(*cursor) {
                Some(Choice::AutoNotify) => format!(
                    "Sends an agent's last terminal lines (secrets redacted) to the webhook when it's blocked, or \
                     finishes a turn longer than {} while you're not watching it.",
                    self.min_turn
                )
                .into(),
                Some(Choice::Command) => "Links ~/.local/bin/nudge so agents can run nudge notify.".into(),
                Some(Choice::Default) => "Notifications go to the default webhook unless they name another one.".into(),
                None => "".into(),
            },
            At::Options { editing: None } if self.offered().is_empty() => {
                "Nothing to choose here. Enter goes on.".into()
            }
            At::Options { editing: None } => "Enter accepts these. e changes them.".into(),
            At::Save(SaveState::Failed { choice, .. }) => match choice {
                FailChoice::Retry => "Sends the test ping again.".into(),
                FailChoice::Back => "Goes back to step 1 to fix the URL or the header.".into(),
                FailChoice::SaveAnyway => "Saves without a working test ping.".into(),
            },
            At::Save(SaveState::NotSaved { .. }) => {
                "Enter tries saving again. Your answers are kept; Shift+Tab goes back to change them.".into()
            }
            At::Save(SaveState::Testing { .. }) => "Ctrl-C stops waiting; nothing is saved.".into(),
            At::Save(SaveState::Saving { .. }) => "Saving…".into(),
        }
    }
}
