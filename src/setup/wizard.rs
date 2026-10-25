//! The setup wizard as pure state: four steps, the answers given so far, and what each key does. No terminal, no
//! files, no network: the loop in `view` reads keys and the clipboard, runs the test ping and draws.
use std::borrow::Cow;
use std::fmt;

use super::paste::{check_header, prefill, split_paste};
use super::{Choices, explain};
use crate::config::{Webhook, check_webhook_name, check_webhook_url, validate_webhook};
use crate::error::{Blame, Result, WebhookError};
use crate::tty::Redacted;

/// A step of the wizard, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Step {
    Connect,
    Name,
    Options,
    Save,
}

impl Step {
    pub(super) const ALL: [Self; 4] = [Self::Connect, Self::Name, Self::Options, Self::Save];

    pub(super) const fn title(self) -> &'static str {
        match self {
            Self::Connect => "Connect webhook",
            Self::Name => "Name",
            Self::Options => "Options",
            Self::Save => "Test and save",
        }
    }

    pub(super) const fn number(self) -> usize {
        match self {
            Self::Connect => 1,
            Self::Name => 2,
            Self::Options => 3,
            Self::Save => 4,
        }
    }

    /// The text fields on this step, in order.
    const fn fields(self) -> &'static [Field] {
        match self {
            Self::Connect => &[Field::Url, Field::Authorization],
            Self::Name => &[Field::Name, Field::Description],
            Self::Options | Self::Save => &[],
        }
    }

    const fn previous(self) -> Option<Self> {
        match self {
            Self::Connect => None,
            Self::Name => Some(Self::Connect),
            Self::Options => Some(Self::Name),
            Self::Save => Some(Self::Options),
        }
    }
}

/// A text field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Field {
    Url,
    Authorization,
    Name,
    Description,
}

impl Field {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Url => "Webhook URL",
            Self::Authorization => "Authorization",
            Self::Name => "Name",
            Self::Description => "Description",
        }
    }

    /// What an empty field says on submit; `None` for optional fields.
    fn missing(self) -> Option<String> {
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
pub(super) enum Source {
    Flag,
    File,
    Environment,
    Saved,
    Clipboard,
    Pasted,
}

impl Source {
    pub(super) const fn label(self) -> &'static str {
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
pub(super) type Sourced = Option<(String, Source)>;

/// A text field's value and what is known about it. `Debug` shows only the value's length: the header is one.
#[derive(Clone, Default)]
pub(super) struct Entry {
    pub(super) value: String,
    /// The value passed its check (or is the saved one).
    pub(super) checked: bool,
    /// Why the value can't be used: its check or the test ping.
    pub(super) problem: Option<String>,
    /// Where the value came from, until it's edited.
    pub(super) source: Option<Source>,
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
    fn given(value: Sourced) -> Self {
        value.map_or_else(Self::default, |(value, source)| Self {
            value,
            checked: true,
            problem: None,
            source: Some(source),
        })
    }
}

/// An option offered on step 3.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Choice {
    AutoNotify,
    Command,
    Default,
}

/// Where step 4 stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum SaveState {
    /// Not tested yet (or an earlier answer changed since).
    Ready,
    /// The test ping is running; `frame` turns the spinner.
    Testing {
        frame: usize,
    },
    Failed {
        blame: Option<Blame>,
        choice: FailChoice,
    },
}

/// What to do after a failed test ping.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FailChoice {
    Retry,
    Back,
    SaveAnyway,
}

impl FailChoice {
    pub(super) const ALL: [Self; 3] = [Self::Retry, Self::Back, Self::SaveAnyway];
}

/// Whether there is anything to lose on quit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Edits {
    None,
    Unsaved,
    /// Leave was pressed once with something entered; pressing it again leaves.
    LeaveOnNextPress,
}

/// What the user pressed, independent of the terminal library. `Debug` shows a paste's length only: it may hold
/// the header.
#[derive(Clone, PartialEq, Eq)]
pub(super) enum Key {
    Up,
    Down,
    Tab,
    BackTab,
    Enter,
    Esc,
    Char(char),
    Backspace,
    /// Ctrl-U: empty the field.
    Clear,
    Paste(String),
    /// Ctrl-C: leave now.
    Cancel,
}

impl fmt::Debug for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Paste(text) => f.debug_tuple("Paste").field(&Redacted(text)).finish(),
            Self::Char(typed) => f.debug_tuple("Char").field(typed).finish(),
            Self::Up => f.write_str("Up"),
            Self::Down => f.write_str("Down"),
            Self::Tab => f.write_str("Tab"),
            Self::BackTab => f.write_str("BackTab"),
            Self::Enter => f.write_str("Enter"),
            Self::Esc => f.write_str("Esc"),
            Self::Backspace => f.write_str("Backspace"),
            Self::Clear => f.write_str("Clear"),
            Self::Cancel => f.write_str("Cancel"),
        }
    }
}

/// What the loop should do after a key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Outcome {
    Continue,
    /// Fill the focused step-1 field (or both, from a combined copy) from the clipboard.
    ReadClipboard,
    /// Start the test ping for this webhook; report back with [`Wizard::tested`].
    Test(Webhook),
    /// Leave the screen and save, with the test ping's HTTP status if one passed.
    Save {
        tested: Option<u16>,
    },
    Quit,
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

/// The wizard: every answer so far, the step on screen, and what step 4 is doing.
/// `Debug` shows where the wizard is, with every value redacted (the header and signing secret are among them).
pub(super) struct Wizard {
    pub(super) step: Step,
    /// The furthest step reached; the steps before it are done.
    reached: Step,
    pub(super) url: Entry,
    pub(super) authorization: Entry,
    pub(super) name: Entry,
    pub(super) description: Entry,
    pub(super) focus: Field,
    pub(super) loaded: Option<String>,
    saved: Vec<String>,
    pub(super) default_webhook: Option<String>,
    pub(super) choices: Choices,
    /// Step 3 shows checkboxes instead of the summary.
    pub(super) editing: bool,
    pub(super) option_cursor: usize,
    pub(super) min_turn: String,
    signing_secret: Option<String>,
    pub(super) test: bool,
    pub(super) save: SaveState,
    edits: Edits,
    pub(super) status: Option<String>,
}

impl fmt::Debug for Wizard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Wizard")
            .field("step", &self.step)
            .field("focus", &self.focus)
            .field("url", &self.url)
            .field("authorization", &self.authorization)
            .field("name", &self.name)
            .field("description", &self.description)
            .field("signing_secret", &self.signing_secret.as_deref().map(Redacted))
            .field("save", &self.save)
            .finish_non_exhaustive()
    }
}

impl Wizard {
    pub(super) fn new(start: Start) -> Self {
        Self {
            step: Step::Connect,
            reached: Step::Connect,
            url: Entry::given(start.url),
            authorization: Entry::given(start.authorization),
            // Not checked yet: `--webhook` is checked when the step is left, with the duplicate check.
            name: Entry {
                value: start.name,
                ..Entry::default()
            },
            description: Entry::given(start.description.map(|text| (text, Source::Flag))),
            focus: Field::Url,
            loaded: start.loaded,
            saved: start.saved,
            default_webhook: start.default_webhook,
            choices: start.choices,
            editing: false,
            option_cursor: 0,
            min_turn: start.min_turn,
            signing_secret: start.signing_secret,
            test: start.test,
            save: SaveState::Ready,
            edits: Edits::None,
            status: None,
        }
    }

    /// Where each step stands, for the step list.
    pub(super) fn done(&self, step: Step) -> bool {
        step != self.step && step.number() < self.reached.number()
    }

    pub(super) const fn entry(&self, field: Field) -> &Entry {
        match field {
            Field::Url => &self.url,
            Field::Authorization => &self.authorization,
            Field::Name => &self.name,
            Field::Description => &self.description,
        }
    }

    const fn entry_mut(&mut self, field: Field) -> &mut Entry {
        match field {
            Field::Url => &mut self.url,
            Field::Authorization => &mut self.authorization,
            Field::Name => &mut self.name,
            Field::Description => &mut self.description,
        }
    }

    /// Whether there's a default to choose: other webhooks are saved and this one isn't the default already.
    pub(super) fn offers_default(&self) -> bool {
        let name = self.name.value.as_str();
        self.saved.iter().any(|saved| saved != name) && self.default_webhook.as_deref() != Some(name)
    }

    /// The options step 3 offers, in order.
    pub(super) fn offered(&self) -> Vec<Choice> {
        let mut offered = Vec::new();
        if self.choices.auto_notify.is_some() {
            offered.push(Choice::AutoNotify);
        }
        if self.choices.command.is_none() {
            offered.push(Choice::Command);
        }
        if self.offers_default() {
            offered.push(Choice::Default);
        }
        offered
    }

    /// The choices as they apply: the default only when it was offered, else `--default` (kept in `choices`).
    pub(super) fn final_choices(&self, flag_default: bool) -> Choices {
        Choices {
            make_default: if self.offers_default() {
                self.choices.make_default
            } else {
                flag_default
            },
            ..self.choices.clone()
        }
    }

    pub(super) fn webhook(&self) -> Result<Webhook> {
        let description = Some(self.description.value.as_str()).filter(|text| !text.is_empty());
        validate_webhook(
            &self.name.value,
            &self.url.value,
            &self.authorization.value,
            description,
            self.signing_secret.as_deref(),
        )
    }

    fn changed(&mut self) {
        self.edits = Edits::Unsaved;
        self.status = None;
        // A new answer needs a new test ping.
        self.save = SaveState::Ready;
    }

    /// Check `field` unless it's empty or already checked. A name another saved webhook uses is refused, except the
    /// one this run updates.
    fn check_field(&mut self, field: Field) {
        let taken = field == Field::Name
            && self.loaded.as_ref() != Some(&self.name.value)
            && self.saved.contains(&self.name.value);
        let entry = self.entry_mut(field);
        if entry.checked || entry.value.is_empty() {
            return;
        }
        let checked = match field {
            Field::Url => check_webhook_url(&entry.value),
            Field::Authorization => check_header(&entry.value),
            Field::Name => check_webhook_name(&entry.value),
            Field::Description => Ok(entry.value.trim().to_owned()),
        };
        match checked {
            Ok(_) if taken => {
                entry.problem = Some(format!(
                    "\"{0}\" is already a webhook; pick another name, or run nudge setup --webhook {0} to change it",
                    entry.value
                ));
            }
            Ok(value) => {
                entry.value = value;
                entry.checked = true;
            }
            Err(error) => entry.problem = Some(problem(&error.to_string()).to_owned()),
        }
    }

    fn edit(&mut self, field: Field, change: impl FnOnce(&mut String)) {
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
    fn fill(&mut self, url: Option<String>, authorization: Option<String>, source: Source) {
        for (field, value) in [(Field::Url, url), (Field::Authorization, authorization)] {
            if let Some(value) = value {
                *self.entry_mut(field) = Entry::given(Some((value, source)));
                self.changed();
            }
        }
    }

    /// Prefill empty step-1 fields from the clipboard, once, when the wizard opens: only values that pass their
    /// check, and a header only if it says it's one. Anything else is ignored and never shown.
    pub(super) fn prefill(&mut self, clipboard: Option<&str>) {
        let found = clipboard.map(prefill).unwrap_or_default();
        let url = found.url.filter(|_| self.url.value.is_empty());
        let authorization = found.authorization.filter(|_| self.authorization.value.is_empty());
        self.fill(url, authorization, Source::Clipboard);
        // Nothing typed yet: leaving doesn't need a second press.
        self.edits = Edits::None;
        self.focus = if self.url.checked && !self.authorization.checked {
            Field::Authorization
        } else {
            Field::Url
        };
    }

    /// Fill the focused step-1 field from the clipboard (`None`: it's empty), or both from a combined copy.
    pub(super) fn fill_from_clipboard(&mut self, text: Option<String>) {
        let field = self.focus;
        let label = field.label();
        let Some(text) = text else {
            self.status = Some(format!("No clipboard text found. Type or paste the {label} here."));
            return;
        };
        let found = split_paste(&text);
        if found.both() {
            self.fill(found.url, found.authorization, Source::Clipboard);
            self.status = Some("✓ Found the URL and the Authorization header on the clipboard.".into());
            return;
        }
        let checked = match field {
            Field::Url => check_webhook_url(&text),
            Field::Authorization | Field::Name | Field::Description => check_header(&text),
        };
        match checked {
            Ok(value) => {
                *self.entry_mut(field) = Entry::given(Some((value, Source::Clipboard)));
                self.changed();
                self.next_field_or_stay();
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

    /// After filling the URL, go on to an empty header.
    fn next_field_or_stay(&mut self) {
        if self.focus == Field::Url && self.authorization.value.is_empty() {
            self.focus = Field::Authorization;
        }
    }

    /// A bracketed paste into `field`. The header must never land in a visible field: a paste holding both values
    /// fills step 1 from any step, a header (or anything that isn't the start of a URL) pasted into the URL field
    /// goes to the Authorization field, and a header pasted into the name or description is refused.
    fn paste(&mut self, field: Field, text: &str) {
        let found = split_paste(text);
        if found.both() {
            self.fill(found.url, found.authorization, Source::Pasted);
            self.status = Some(if self.step == Step::Connect {
                "✓ Found the URL and the Authorization header in what you pasted.".into()
            } else {
                "✓ Found the URL and the Authorization header in what you pasted; step 1 has them now.".into()
            });
            return;
        }
        let header_shaped = found.authorization.is_some();
        let push = |value: &mut String| value.push_str(text);
        match field {
            Field::Url if header_shaped || (self.url.value.is_empty() && !text.starts_with("http")) => {
                self.edit(Field::Authorization, |value| {
                    value.clear();
                    value.push_str(text);
                });
                self.focus = Field::Authorization;
                self.status =
                    Some("That isn't a URL, so it went into the Authorization field, where it's never shown.".into());
            }
            Field::Name | Field::Description if header_shaped => {
                self.status = Some(
                    "✗ That looks like an Authorization header, so it wasn't pasted here. It belongs in step 1.".into(),
                );
            }
            Field::Url | Field::Authorization | Field::Name | Field::Description => self.edit(field, push),
        }
    }

    /// Move the focus within the step's fields (wrapping), checking the field left.
    fn move_focus(&mut self, forward: bool) {
        let fields = self.step.fields();
        let Some(index) = fields.iter().position(|field| *field == self.focus) else {
            return;
        };
        self.check_field(self.focus);
        let next = if forward {
            (index + 1) % fields.len()
        } else {
            (index + fields.len() - 1) % fields.len()
        };
        if let Some(field) = fields.get(next) {
            self.focus = *field;
        }
    }

    /// Check the step and go on to the next one; a field that needs fixing gets the focus instead.
    fn advance(&mut self) -> Outcome {
        for &field in self.step.fields() {
            self.check_field(field);
            let entry = self.entry_mut(field);
            if entry.value.is_empty() {
                entry.problem = field.missing();
            }
            if entry.problem.is_some() {
                self.focus = field;
                return Outcome::Continue;
            }
        }
        self.status = None;
        match self.step {
            Step::Connect => self.go(Step::Name),
            Step::Name => self.go(Step::Options),
            Step::Options => return self.enter_save(),
            Step::Save => {}
        }
        Outcome::Continue
    }

    fn go(&mut self, step: Step) {
        self.step = step;
        if step.number() > self.reached.number() {
            self.reached = step;
        }
        self.focus = step.fields().first().copied().unwrap_or(self.focus);
        self.editing = false;
        self.option_cursor = 0;
    }

    fn back(&mut self) {
        if let Some(previous) = self.step.previous() {
            self.status = None;
            self.save = SaveState::Ready;
            self.go(previous);
        }
    }

    /// Step 4: start the test ping, or save at once with `--no-test`.
    fn enter_save(&mut self) -> Outcome {
        match self.webhook() {
            Ok(webhook) => {
                self.go(Step::Save);
                if self.test {
                    self.save = SaveState::Testing { frame: 0 };
                    Outcome::Test(webhook)
                } else {
                    Outcome::Save { tested: None }
                }
            }
            Err(error) => {
                self.status = Some(format!("✗ {}", problem(&error.to_string())));
                Outcome::Continue
            }
        }
    }

    /// The test ping's result: save on a pass; otherwise offer the way out the failure points to.
    pub(super) fn tested(&mut self, result: Result<u16, WebhookError>) -> Outcome {
        match result {
            Ok(status) => Outcome::Save { tested: Some(status) },
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
                self.save = SaveState::Failed {
                    blame,
                    choice: if blame.is_some() {
                        FailChoice::Back
                    } else {
                        FailChoice::Retry
                    },
                };
                Outcome::Continue
            }
        }
    }

    /// Turn the spinner while the test ping runs.
    pub(super) const fn tick(&mut self) {
        if let SaveState::Testing { frame } = &mut self.save {
            *frame = frame.wrapping_add(1);
        }
    }

    /// Leave, asking once more if anything was entered.
    fn leave(&mut self, key: &str) -> Outcome {
        if self.edits == Edits::Unsaved {
            self.edits = Edits::LeaveOnNextPress;
            self.status = Some(format!("Press {key} again to leave without saving."));
            return Outcome::Continue;
        }
        Outcome::Quit
    }

    pub(super) fn handle(&mut self, key: Key) -> Outcome {
        if key == Key::Cancel {
            return Outcome::Quit;
        }
        if self.edits == Edits::LeaveOnNextPress {
            let again = matches!((&key, self.step), (Key::Esc | Key::BackTab, Step::Connect))
                || (key == Key::Char('q') && matches!(self.step, Step::Options | Step::Save));
            if again {
                return Outcome::Quit;
            }
            self.edits = Edits::Unsaved;
            self.status = None;
        }
        match self.step {
            Step::Connect | Step::Name => self.handle_text(key),
            Step::Options => self.handle_options(&key),
            Step::Save => self.handle_save(&key),
        }
    }

    /// Steps 1 and 2: typing goes into the focused field.
    fn handle_text(&mut self, key: Key) -> Outcome {
        let field = self.focus;
        match key {
            Key::Enter if self.step == Step::Connect && self.entry(field).value.is_empty() => {
                return Outcome::ReadClipboard;
            }
            Key::Enter if field == Field::Url => {
                self.check_field(field);
                if self.url.problem.is_none() && self.authorization.value.is_empty() {
                    self.focus = Field::Authorization;
                } else if self.url.problem.is_none() {
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
            Key::Esc | Key::BackTab if self.step == Step::Connect => return self.leave("Esc"),
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
        match key {
            Key::Char('q') => return self.leave("q"),
            Key::Enter | Key::Esc | Key::BackTab if self.editing => self.editing = false,
            Key::Enter => return self.advance(),
            Key::Char('e') if !self.editing && !offered.is_empty() => self.editing = true,
            Key::Esc | Key::BackTab => self.back(),
            Key::Up if self.editing => self.option_cursor = self.option_cursor.saturating_sub(1),
            Key::Down | Key::Tab if self.editing => {
                self.option_cursor = (self.option_cursor + 1).min(offered.len().saturating_sub(1));
            }
            Key::Char(' ') if self.editing => {
                if let Some(choice) = offered.get(self.option_cursor) {
                    match choice {
                        Choice::AutoNotify => self.choices.auto_notify = self.choices.auto_notify.map(|on| !on),
                        Choice::Command => self.choices.link_command = !self.choices.link_command,
                        Choice::Default => self.choices.make_default = !self.choices.make_default,
                    }
                    self.edits = Edits::Unsaved;
                }
            }
            Key::Up
            | Key::Down
            | Key::Tab
            | Key::Char(_)
            | Key::Backspace
            | Key::Clear
            | Key::Paste(_)
            | Key::Cancel => {}
        }
        Outcome::Continue
    }

    /// Step 4: wait for the test ping, then pick what to do about a failure.
    fn handle_save(&mut self, key: &Key) -> Outcome {
        let SaveState::Failed { blame, choice } = &mut self.save else {
            // While the ping runs (or before it starts), only Ctrl-C does anything.
            return Outcome::Continue;
        };
        let index = FailChoice::ALL
            .iter()
            .position(|candidate| candidate == choice)
            .unwrap_or(0);
        match key {
            Key::Up => {
                *choice = FailChoice::ALL.get(index.saturating_sub(1)).copied().unwrap_or(*choice);
            }
            Key::Down | Key::Tab => *choice = FailChoice::ALL.get(index + 1).copied().unwrap_or(*choice),
            Key::Char('q') => return self.leave("q"),
            Key::Esc | Key::BackTab => self.back(),
            Key::Enter => match *choice {
                FailChoice::Retry => {
                    self.status = None;
                    return self.enter_save();
                }
                FailChoice::Back => {
                    let field = match blame {
                        Some(Blame::Authorization) => Field::Authorization,
                        Some(Blame::Url) | None => Field::Url,
                    };
                    self.back_to_connect(field);
                }
                FailChoice::SaveAnyway => return Outcome::Save { tested: None },
            },
            Key::Char(_) | Key::Backspace | Key::Clear | Key::Paste(_) | Key::Cancel => {}
        }
        Outcome::Continue
    }

    fn back_to_connect(&mut self, field: Field) {
        self.save = SaveState::Ready;
        self.go(Step::Connect);
        self.focus = field;
        // The reason stays on the field (and in the note while it has the focus).
        self.status = None;
    }

    /// The status, or the problem with the focused field.
    pub(super) fn status(&self) -> Option<String> {
        self.status.clone().or_else(|| match self.step {
            Step::Connect | Step::Name => self
                .entry(self.focus)
                .problem
                .as_ref()
                .map(|problem| format!("✗ {problem}")),
            Step::Options | Step::Save => None,
        })
    }

    /// What the focused element does.
    pub(super) fn hint(&self) -> Cow<'static, str> {
        match (self.step, self.focus) {
            (Step::Connect, Field::Url) if self.url.value.is_empty() => {
                "Copy the webhook URL from your webhook's settings, then press Enter here to paste it, or type it."
                    .into()
            }
            (Step::Connect, Field::Authorization) if self.authorization.value.is_empty() => {
                "Copy the Authorization header from the same page, then press Enter here to paste it. It's never shown."
                    .into()
            }
            (Step::Connect, Field::Authorization) if self.authorization.checked => {
                "Never shown. To replace it, paste or type a new one; Ctrl-U empties the field.".into()
            }
            (Step::Connect, _) => "Paste or type to change it; Ctrl-U empties the field. Enter goes on.".into(),
            (Step::Name, Field::Description) => {
                "Optional. Shown by nudge list and to agents choosing a webhook.".into()
            }
            (Step::Name, _) => "Letters, digits, - and _. Enter goes on; Tab moves to the description.".into(),
            (Step::Options, _) if self.editing => match self.offered().get(self.option_cursor) {
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
            (Step::Options, _) if self.offered().is_empty() => "Nothing to choose here. Enter goes on.".into(),
            (Step::Options, _) => "Enter accepts these. e changes them.".into(),
            (Step::Save, _) => match &self.save {
                SaveState::Failed { choice, .. } => match choice {
                    FailChoice::Retry => "Sends the test ping again.".into(),
                    FailChoice::Back => "Goes back to step 1 to fix the URL or the header.".into(),
                    FailChoice::SaveAnyway => "Saves without a working test ping.".into(),
                },
                SaveState::Ready | SaveState::Testing { .. } => "Ctrl-C stops waiting; nothing is saved.".into(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Choice, FailChoice, Field, Key, Outcome, SaveState, Source, Start, Step, Wizard};
    use crate::error::{Blame, WebhookError};
    use crate::install::{Plugin, PluginInfo};
    use crate::setup::Choices;

    const URL: &str = "https://hooks.example.com/h/7f3c2a";
    const HEADER: &str = "Bearer token-0123456789abcdef";

    fn start(saved: &[&str], loaded: Option<&str>) -> Start {
        Start {
            name: loaded.unwrap_or("main").to_owned(),
            loaded: loaded.map(str::to_owned),
            url: None,
            authorization: None,
            description: None,
            saved: saved.iter().map(|name| (*name).to_owned()).collect(),
            default_webhook: saved.first().map(|name| (*name).to_owned()),
            choices: Choices {
                plugin: Plugin::Installed(PluginInfo {
                    enabled: true,
                    plugin_root: "/plugin".into(),
                    version: "0.9.0".into(),
                }),
                auto_notify: Some(true),
                command: None,
                link_command: true,
                make_default: false,
            },
            min_turn: "2 minutes".into(),
            signing_secret: None,
            test: true,
        }
    }

    fn type_text(wizard: &mut Wizard, text: &str) {
        for typed in text.chars() {
            assert_eq!(wizard.handle(Key::Char(typed)), Outcome::Continue);
        }
    }

    /// A wizard on step 1 with both values typed in.
    fn connected(saved: &[&str]) -> Wizard {
        let mut wizard = Wizard::new(start(saved, None));
        type_text(&mut wizard, URL);
        wizard.handle(Key::Tab);
        type_text(&mut wizard, HEADER);
        wizard
    }

    fn failure(status: u16) -> WebhookError {
        WebhookError {
            message: format!("webhook \"main\" returned HTTP {status}: nope"),
            http_status: Some(status),
        }
    }

    #[test]
    fn step_one_stays_until_both_values_check_out() {
        let mut wizard = Wizard::new(start(&[], None));
        type_text(&mut wizard, "http://example.com/hook");
        wizard.handle(Key::Enter);
        assert_eq!((wizard.step, wizard.focus), (Step::Connect, Field::Url));
        assert!(wizard.status().unwrap().contains("https URL"));
        wizard.handle(Key::Clear);
        type_text(&mut wizard, URL);
        wizard.handle(Key::Enter);
        assert_eq!(
            (wizard.step, wizard.focus),
            (Step::Connect, Field::Authorization),
            "a valid URL moves on to the empty header, not the next step"
        );
        assert_eq!(
            wizard.handle(Key::Enter),
            Outcome::ReadClipboard,
            "Enter on an empty field pastes"
        );
        type_text(&mut wizard, HEADER);
        wizard.handle(Key::Enter);
        assert_eq!(wizard.step, Step::Name);
    }

    #[test]
    fn going_back_keeps_every_answer_and_a_new_url_needs_a_new_test() {
        let mut wizard = connected(&[]);
        wizard.handle(Key::Enter);
        wizard.handle(Key::Tab);
        type_text(&mut wizard, "Main webhook");
        wizard.handle(Key::Enter);
        assert_eq!(wizard.step, Step::Options);
        wizard.handle(Key::Char('e'));
        wizard.handle(Key::Char(' '));
        wizard.handle(Key::Enter);
        assert_eq!(wizard.choices.auto_notify, Some(false));
        let Outcome::Test(first) = wizard.handle(Key::Enter) else {
            panic!("expected a test ping");
        };
        wizard.tested(Err(failure(500)));
        for _ in 0..3 {
            wizard.handle(Key::BackTab);
        }
        assert_eq!(wizard.step, Step::Connect);
        assert_eq!(wizard.save, SaveState::Ready);
        for _ in 0..2 {
            wizard.handle(Key::Enter);
        }
        let Outcome::Test(again) = wizard.handle(Key::Enter) else {
            panic!("expected a test ping");
        };
        assert_eq!(again, first, "nothing was lost on the way back");
        assert_eq!(wizard.choices.auto_notify, Some(false));
        assert!(wizard.done(Step::Connect) && wizard.done(Step::Options) && !wizard.done(Step::Save));
    }

    #[test]
    fn the_clipboard_prefills_only_what_checks_out_and_a_combined_paste_fills_both() {
        let mut wizard = Wizard::new(start(&[], None));
        wizard.prefill(Some("my password is hunter2-0123456789"));
        assert!(wizard.url.value.is_empty() && wizard.authorization.value.is_empty());
        wizard.prefill(Some(URL));
        assert_eq!(
            (wizard.url.value.as_str(), wizard.url.source, wizard.focus),
            (URL, Some(Source::Clipboard), Field::Authorization)
        );
        let mut wizard = Wizard::new(start(&[], None));
        wizard.handle(Key::Paste(format!("curl -H 'Authorization: {HEADER}' {URL}")));
        assert_eq!(
            (wizard.url.value.as_str(), wizard.authorization.value.as_str()),
            (URL, HEADER)
        );
        assert_eq!(wizard.authorization.source, Some(Source::Pasted));
        let mut wizard = Wizard::new(start(&[], None));
        wizard.fill_from_clipboard(Some("Bearer not-a-url-0123456789".into()));
        assert!(wizard.url.value.is_empty());
        assert!(
            wizard
                .status()
                .unwrap()
                .starts_with("✗ The clipboard doesn't hold the Webhook URL")
        );
    }

    #[test]
    fn a_name_must_be_valid_and_not_another_webhooks() {
        let mut wizard = connected(&["research"]);
        wizard.handle(Key::Enter);
        wizard.handle(Key::Clear);
        wizard.handle(Key::Enter);
        assert!(wizard.status().unwrap().contains("Give this webhook a name."));
        type_text(&mut wizard, "research");
        wizard.handle(Key::Enter);
        assert_eq!(wizard.step, Step::Name);
        assert!(wizard.status().unwrap().contains("\"research\" is already a webhook"));
        wizard.handle(Key::Clear);
        type_text(&mut wizard, "my laptop");
        wizard.handle(Key::Enter);
        assert_eq!(wizard.step, Step::Name);
        wizard.handle(Key::Clear);
        type_text(&mut wizard, "laptop");
        wizard.handle(Key::Enter);
        assert_eq!(wizard.step, Step::Options);
        assert_eq!(wizard.offered(), [Choice::AutoNotify, Choice::Command, Choice::Default]);

        let mut update = Wizard::new(start(&["main", "research"], Some("research")));
        type_text(&mut update, URL);
        update.handle(Key::Tab);
        type_text(&mut update, HEADER);
        update.handle(Key::Enter);
        update.handle(Key::Enter);
        assert_eq!(update.step, Step::Options, "the webhook being updated keeps its name");
    }

    #[test]
    fn a_failed_test_offers_the_way_out_it_points_to() {
        let mut wizard = connected(&[]);
        for _ in 0..2 {
            wizard.handle(Key::Enter);
        }
        assert!(matches!(wizard.handle(Key::Enter), Outcome::Test(_)));
        wizard.tested(Err(failure(401)));
        assert!(matches!(
            wizard.save,
            SaveState::Failed {
                blame: Some(Blame::Authorization),
                choice: FailChoice::Back
            }
        ));
        wizard.handle(Key::Enter);
        assert_eq!((wizard.step, wizard.focus), (Step::Connect, Field::Authorization));
        assert!(wizard.status().unwrap().contains("rejected the Authorization header"));
        assert_eq!(
            wizard.handle(Key::Enter),
            Outcome::Continue,
            "the rejected header isn't sent again"
        );
        assert_eq!(wizard.step, Step::Connect);
        type_text(&mut wizard, "Bearer other-0123456789abcdef");
        wizard.handle(Key::Enter);
        wizard.handle(Key::Enter);
        assert!(matches!(wizard.handle(Key::Enter), Outcome::Test(_)));
        wizard.tested(Err(failure(500)));
        assert!(matches!(
            wizard.save,
            SaveState::Failed {
                choice: FailChoice::Retry,
                ..
            }
        ));
        assert!(matches!(wizard.handle(Key::Enter), Outcome::Test(_)), "retry");
        wizard.tested(Err(failure(503)));
        wizard.handle(Key::Down);
        wizard.handle(Key::Down);
        assert_eq!(wizard.handle(Key::Enter), Outcome::Save { tested: None });
        assert_eq!(wizard.tested(Ok(200)), Outcome::Save { tested: Some(200) });
    }

    #[test]
    fn q_types_on_text_steps_and_leaves_on_later_ones_ctrl_c_always_leaves() {
        let mut wizard = Wizard::new(start(&[], None));
        assert_eq!(wizard.handle(Key::Esc), Outcome::Quit, "nothing entered");
        let mut wizard = connected(&[]);
        assert_eq!(wizard.handle(Key::Esc), Outcome::Continue);
        assert!(wizard.status().unwrap().contains("Press Esc again"));
        assert_eq!(wizard.handle(Key::Esc), Outcome::Quit);
        let mut wizard = connected(&[]);
        wizard.handle(Key::Enter);
        wizard.handle(Key::Char('q'));
        assert_eq!(wizard.name.value, "mainq", "q is text on step 2");
        wizard.handle(Key::Backspace);
        wizard.handle(Key::Enter);
        assert_eq!(wizard.handle(Key::Char('q')), Outcome::Continue);
        assert_eq!(wizard.handle(Key::Char('q')), Outcome::Quit);
        let mut wizard = connected(&[]);
        assert_eq!(wizard.handle(Key::Cancel), Outcome::Quit);
    }

    #[test]
    fn a_pasted_header_never_lands_in_a_visible_field() {
        let mut wizard = Wizard::new(start(&[], None));
        wizard.handle(Key::Paste(format!("  {HEADER}\n")));
        assert!(wizard.url.value.is_empty(), "the URL field never holds the header");
        assert_eq!(
            (wizard.authorization.value.as_str(), wizard.focus),
            (HEADER, Field::Authorization)
        );
        let mut wizard = Wizard::new(start(&[], None));
        wizard.handle(Key::Paste("sk-live-0123456789abcdef".into()));
        assert!(
            wizard.url.value.is_empty(),
            "text that isn't a URL goes to the hidden field"
        );

        let mut wizard = connected(&[]);
        wizard.handle(Key::Enter);
        assert_eq!(wizard.step, Step::Name);
        wizard.handle(Key::Paste(format!("Authorization: {HEADER}")));
        assert_eq!(wizard.name.value, "main", "a header pasted into the name is refused");
        assert!(wizard.status().unwrap().contains("looks like an Authorization header"));
        wizard.handle(Key::Tab);
        let other = "Bearer other-0123456789abcdef";
        wizard.handle(Key::Paste(format!("https://other.example/h\n{other}")));
        assert!(
            wizard.description.value.is_empty(),
            "a URL and header pasted on step 2 go to step 1"
        );
        assert_eq!(
            (wizard.url.value.as_str(), wizard.authorization.value.as_str()),
            ("https://other.example/h", other)
        );
    }

    #[test]
    fn debug_output_never_contains_the_header() {
        let mut wizard = connected(&[]);
        let pasted = format!("{:?}", Key::Paste(HEADER.into()));
        for _ in 0..2 {
            wizard.handle(Key::Enter);
        }
        let test = wizard.handle(Key::Enter);
        assert!(matches!(test, Outcome::Test(_)));
        for shown in [pasted, format!("{wizard:?}"), format!("{test:?}")] {
            assert!(!shown.contains("token-0123456789abcdef"), "{shown}");
        }
    }
}
