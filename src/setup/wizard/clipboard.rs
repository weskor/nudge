//! Values that arrive as a whole rather than typed: the clipboard read when the wizard opens, the one Enter asks for,
//! and bracketed pastes. The header never lands in a visible field.
use super::field::{Entry, Field, Source, problem};
use super::step::Step;
use super::{Edits, Wizard};
use crate::config::check_webhook_url;
use crate::setup::paste::{check_header, prefill, split_paste};

impl Wizard {
    /// Prefill empty step-1 fields from the clipboard read when the wizard opened: only values that pass their check,
    /// and a header only if it says it's one. Anything else is ignored and never shown. The read runs in the
    /// background, so the user may have typed meanwhile: typed fields, the focus and the "anything to lose?" state
    /// are kept.
    pub(in crate::setup) fn prefill(&mut self, clipboard: Option<&str>) {
        let untouched = self.edits == Edits::None;
        let found = clipboard.map(prefill).unwrap_or_default();
        let url = found.url.filter(|_| self.answers.url.value.is_empty());
        let authorization = found
            .authorization
            .filter(|_| self.answers.authorization.value.is_empty());
        self.fill(url, authorization, Source::Clipboard);
        if untouched {
            // Nothing typed yet: leaving doesn't need a second press.
            self.edits = Edits::None;
            if self.answers.url.checked && !self.answers.authorization.checked {
                self.set_focus(Field::Authorization);
            }
        }
    }

    /// The clipboard read Enter asked for on `field` (`None`: the clipboard is empty), or both fields from a combined
    /// copy. The read runs in the background, so it fills only fields that are still empty, and nothing once the user
    /// has left step 1.
    pub(in crate::setup) fn fill_from_clipboard(&mut self, field: Field, text: Option<String>) {
        if self.step() != Step::Connect || !self.entry(field).value.is_empty() {
            return;
        }
        let label = field.label();
        let Some(text) = text else {
            self.status = Some(format!("No clipboard text found. Type or paste the {label} here."));
            return;
        };
        let found = split_paste(&text);
        if found.both() {
            let url = found.url.filter(|_| self.answers.url.value.is_empty());
            let authorization = found
                .authorization
                .filter(|_| self.answers.authorization.value.is_empty());
            self.fill(url, authorization, Source::Clipboard);
            self.status = Some("✓ Found the URL and the Authorization header on the clipboard.".into());
            return;
        }
        let checked = match field {
            Field::Url => check_webhook_url(&text),
            // The header found in a copied command line or header line, else the whole text as the header.
            Field::Authorization | Field::Name | Field::Description => {
                found.authorization.map_or_else(|| check_header(&text), Ok)
            }
        };
        match checked {
            Ok(value) => {
                *self.entry_mut(field) = Entry::given(Some((value, Source::Clipboard)));
                self.changed();
                if field == Field::Url
                    && self.focus() == Some(Field::Url)
                    && self.answers.authorization.value.is_empty()
                {
                    self.set_focus(Field::Authorization);
                }
            }
            // The clipboard may hold anything, a secret included, so a value that fails its check isn't shown.
            Err(error) => {
                self.status = Some(format!(
                    "✗ The clipboard doesn't hold the {label}: {}",
                    problem(&error.to_string())
                ));
            }
        }
    }

    /// A bracketed paste into `field`. The header must never land in a visible field: a paste holding both values
    /// fills step 1 from any step, a header pasted into either step-1 field is stored as the header found (not the
    /// command line around it), anything else pasted into an empty URL field that isn't the start of a URL goes to the
    /// Authorization field, and a header pasted into the name or description is refused.
    pub(super) fn paste(&mut self, field: Field, text: &str) {
        let found = split_paste(text);
        if found.both() {
            self.fill(found.url, found.authorization, Source::Pasted);
            self.status = Some(if self.step() == Step::Connect {
                "✓ Found the URL and the Authorization header in what you pasted.".into()
            } else {
                "✓ Found the URL and the Authorization header in what you pasted; step 1 has them now.".into()
            });
            return;
        }
        let moved = "That isn't a URL, so it went into the Authorization field, where it's never shown.";
        match (field, found.authorization) {
            (Field::Name | Field::Description, Some(_)) => {
                self.status = Some(
                    "✗ That looks like an Authorization header, so it wasn't pasted here. It belongs in step 1.".into(),
                );
            }
            (Field::Url | Field::Authorization, Some(header)) => {
                self.fill(None, Some(header), Source::Pasted);
                if field == Field::Url {
                    self.set_focus(Field::Authorization);
                    self.status = Some(moved.into());
                }
            }
            (Field::Url, None) if self.answers.url.value.is_empty() && !text.starts_with("http") => {
                self.edit(Field::Authorization, |value| {
                    value.clear();
                    value.push_str(text);
                });
                self.set_focus(Field::Authorization);
                self.status = Some(moved.into());
            }
            (Field::Url | Field::Authorization | Field::Name | Field::Description, None) => {
                self.edit(field, |value| value.push_str(text));
            }
        }
    }
}

#[cfg(test)]
mod tests;
