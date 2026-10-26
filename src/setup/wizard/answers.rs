//! The wizard's answers: the text fields and the step-3 choices, checking and editing them, and the webhook and
//! choices they add up to.
use super::field::{Entry, Field, Source, problem};
use super::step::Choice;
use super::{Edits, Wizard};
use crate::config::{Webhook, check_webhook_name, check_webhook_url, validate_webhook};
use crate::error::Result;
use crate::setup::choices::Choices;
use crate::setup::paste::check_header;

/// Every answer given so far. Only the wizard's answer-changing methods write it; no step change touches it.
#[derive(Debug)]
pub(super) struct Answers {
    pub(super) url: Entry,
    pub(super) authorization: Entry,
    pub(super) name: Entry,
    pub(super) description: Entry,
    pub(super) choices: Choices,
}

impl Wizard {
    pub(in crate::setup) const fn choices(&self) -> &Choices {
        &self.answers.choices
    }

    pub(in crate::setup) const fn entry(&self, field: Field) -> &Entry {
        match field {
            Field::Url => &self.answers.url,
            Field::Authorization => &self.answers.authorization,
            Field::Name => &self.answers.name,
            Field::Description => &self.answers.description,
        }
    }

    pub(super) const fn entry_mut(&mut self, field: Field) -> &mut Entry {
        match field {
            Field::Url => &mut self.answers.url,
            Field::Authorization => &mut self.answers.authorization,
            Field::Name => &mut self.answers.name,
            Field::Description => &mut self.answers.description,
        }
    }

    /// Whether there's a default to choose: other webhooks are saved and this one isn't the default already.
    fn offers_default(&self) -> bool {
        let name = self.answers.name.value.as_str();
        self.saved.iter().any(|saved| saved != name) && self.default_webhook.as_deref() != Some(name)
    }

    /// The options step 3 offers, in order.
    pub(in crate::setup) fn offered(&self) -> Vec<Choice> {
        let choices = &self.answers.choices;
        let mut offered = Vec::new();
        if choices.auto_notify.is_some() {
            offered.push(Choice::AutoNotify);
        }
        if choices.command.is_none() {
            offered.push(Choice::Command);
        }
        if self.offers_default() {
            offered.push(Choice::Default);
        }
        offered
    }

    /// The choices as they apply: the default only when it was offered, else `--default` (kept in `choices`).
    pub(in crate::setup) fn final_choices(&self, flag_default: bool) -> Choices {
        Choices {
            make_default: if self.offers_default() {
                self.answers.choices.make_default
            } else {
                flag_default
            },
            ..self.answers.choices.clone()
        }
    }

    pub(in crate::setup) fn webhook(&self) -> Result<Webhook> {
        let answers = &self.answers;
        let description = Some(answers.description.value.as_str()).filter(|text| !text.is_empty());
        validate_webhook(
            &answers.name.value,
            &answers.url.value,
            &answers.authorization.value,
            description,
            self.signing_secret.as_deref(),
        )
    }

    pub(super) fn changed(&mut self) {
        self.edits = Edits::Unsaved;
        self.status = None;
    }

    /// Check `field` unless it's empty or already checked. A name another saved webhook uses is refused, except the
    /// one this run updates; the check uses the name as it will be saved (trimmed).
    pub(super) fn check_field(&mut self, field: Field) {
        let entry = self.entry(field);
        if entry.checked || entry.value.is_empty() {
            return;
        }
        let checked = match field {
            Field::Url => check_webhook_url(&entry.value),
            Field::Authorization => check_header(&entry.value),
            Field::Name => check_webhook_name(&entry.value),
            Field::Description => Ok(entry.value.trim().to_owned()),
        };
        let taken =
            |name: &String| field == Field::Name && self.loaded.as_ref() != Some(name) && self.saved.contains(name);
        let checked = match checked {
            Ok(name) if taken(&name) => Err(format!(
                "\"{name}\" is already a webhook; pick another name, or run nudge setup --webhook {name} to change it"
            )),
            Ok(value) => Ok(value),
            Err(error) => Err(problem(&error.to_string()).to_owned()),
        };
        let entry = self.entry_mut(field);
        match checked {
            Ok(value) => {
                entry.value = value;
                entry.checked = true;
            }
            Err(problem) => entry.problem = Some(problem),
        }
    }

    pub(super) fn edit(&mut self, field: Field, change: impl FnOnce(&mut String)) {
        let entry = self.entry_mut(field);
        // A checked header isn't shown, so typing into it starts a new one instead of changing it unseen.
        if field == Field::Authorization && entry.checked {
            entry.value.clear();
        }
        change(&mut entry.value);
        entry.checked = false;
        entry.problem = None;
        entry.source = None;
        self.changed();
    }

    /// Fill the step-1 fields that have a value, from `source`.
    pub(super) fn fill(&mut self, url: Option<String>, authorization: Option<String>, source: Source) {
        for (field, value) in [(Field::Url, url), (Field::Authorization, authorization)] {
            if let Some(value) = value {
                *self.entry_mut(field) = Entry::given(Some((value, source)));
                self.changed();
            }
        }
    }
}

#[cfg(test)]
mod tests;
