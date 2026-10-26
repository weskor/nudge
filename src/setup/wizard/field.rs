//! The wizard's text fields: which there are, each one's value and what is known about it, and where a step-1 value
//! came from.
use std::fmt;

use crate::terminal::tty::Redacted;

/// A text field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::setup) enum Field {
    Url,
    Authorization,
    Name,
    Description,
}

impl Field {
    pub(in crate::setup) const fn label(self) -> &'static str {
        match self {
            Self::Url => "Webhook URL",
            Self::Authorization => "Authorization",
            Self::Name => "Name",
            Self::Description => "Description",
        }
    }

    /// What an empty field says on submit; `None` for optional fields.
    pub(super) fn missing(self) -> Option<String> {
        match self {
            Self::Url | Self::Authorization => Some(format!("Paste or type the {}.", self.label())),
            Self::Name => Some("Give this webhook a name.".into()),
            Self::Description => None,
        }
    }
}

/// A validation message without its "invalid …" heading line.
pub(super) fn problem(message: &str) -> &str {
    match message.split_once('\n') {
        Some((heading, rest)) if heading.starts_with("invalid ") => rest.trim_start(),
        Some(_) | None => message,
    }
}

/// Where a step-1 value came from, shown under it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::setup) enum Source {
    Flag,
    File,
    Environment,
    Saved,
    Clipboard,
    Pasted,
}

impl Source {
    pub(in crate::setup) const fn label(self) -> &'static str {
        match self {
            Self::Flag => "from the command line",
            Self::File => "from --authorization-file",
            Self::Environment => "from the environment",
            Self::Saved => "saved",
            Self::Clipboard => "from the clipboard",
            Self::Pasted => "pasted",
        }
    }
}

/// A step-1 value and where it came from, if there is one.
pub(in crate::setup) type Sourced = Option<(String, Source)>;

/// A text field's value and what is known about it. `Debug` shows only the value's length: the header is one.
#[derive(Clone, Default)]
pub(in crate::setup) struct Entry {
    pub(in crate::setup) value: String,
    /// The value passed its check (or is the saved one).
    pub(in crate::setup) checked: bool,
    /// Why the value can't be used: its check or the test ping.
    pub(in crate::setup) problem: Option<String>,
    /// Where the value came from, until it's edited.
    pub(in crate::setup) source: Option<Source>,
}

impl fmt::Debug for Entry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Entry")
            .field("value", &Redacted(&self.value))
            .field("checked", &self.checked)
            .field("problem", &self.problem)
            .field("source", &self.source)
            .finish()
    }
}

impl Entry {
    pub(super) fn given(value: Sourced) -> Self {
        value.map_or_else(Self::default, |(value, source)| Self {
            value,
            checked: true,
            problem: None,
            source: Some(source),
        })
    }
}
