//! The interactive `nudge setup`: a full-screen form (clipboard-first), then the summary of what was saved and done.
use std::borrow::Cow;
use std::path::Path;

use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{
    self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEventKind, KeyModifiers,
};
use ratatui::crossterm::execute;
use ratatui::text::{Line, Span};

use super::{SetupOptions, explain, ping, save};
use crate::clipboard::{read_clipboard, runtime_dir};
use crate::config::{
    Auto, Webhook, check_authorization, check_webhook_name, check_webhook_url, default_config_path, read_config,
    seconds_in_words, validate_webhook,
};
use crate::context::{Env, System};
use crate::error::{Blame, Error, Result, WebhookError};
use crate::install::{Plugin, command_link, link_command, nudge_root, plugin};
use crate::state::{current_state, update_state};
use crate::tty::{Mark, Style, mask_secret};
use crate::ui::{self, BAD, DIM, GOOD, HEADER, View, WARN, check};

const HELP: [(&str, &str); 5] = [
    ("↑↓", "move"),
    ("enter", "paste/next"),
    ("space", "tick"),
    ("ctrl-s", "test & save"),
    ("esc", "quit"),
];
const HELP_NO_TEST: [(&str, &str); 5] = [
    ("↑↓", "move"),
    ("enter", "paste/next"),
    ("space", "tick"),
    ("ctrl-s", "save"),
    ("esc", "quit"),
];
/// The form's width (or the screen's, if narrower): room for a typical webhook URL.
const FORM_WIDTH: u16 = 84;
/// Where field labels, checkboxes and the button start: under a section's title, after its number.
const INDENT: &str = "   ";
/// Columns before a field's value: indent and label.
const LABEL_WIDTH: usize = 18;

/// A text field of the form.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Field {
    Url,
    Authorization,
    Name,
    Description,
}

/// The text fields, in the order they're shown and checked.
const FIELDS: [Field; 4] = [Field::Url, Field::Authorization, Field::Name, Field::Description];

impl Field {
    const fn label(self) -> &'static str {
        match self {
            Self::Url => "Webhook URL",
            Self::Authorization => "Authorization",
            Self::Name => "Name",
            Self::Description => "Description",
        }
    }

    /// The value to keep, or why it can't be used.
    fn check(self, value: &str) -> Result<String> {
        match self {
            Self::Url => check_webhook_url(value),
            Self::Authorization => {
                let checked = check_authorization(value)?;
                let lower = checked.to_ascii_lowercase();
                if lower.starts_with("http://") || lower.starts_with("https://") {
                    return Err(Error::Config(
                        "that's a URL, not the Authorization header. Copy the header value from your webhook's settings."
                            .into(),
                    ));
                }
                Ok(checked)
            }
            Self::Name => check_webhook_name(value),
            Self::Description => Ok(value.trim().to_owned()),
        }
    }
}

/// A validation message without its "invalid …" heading line.
fn problem(message: &str) -> &str {
    match message.split_once('\n') {
        Some((heading, rest)) if heading.starts_with("invalid ") => rest.trim_start(),
        Some(_) | None => message,
    }
}

/// A text field's value and what is known about it.
#[derive(Clone, Debug, Default)]
struct Entry {
    value: String,
    /// The value passed its check (or is the saved one).
    checked: bool,
    /// Why the value can't be used: its check or the test ping.
    problem: Option<String>,
}

impl Entry {
    fn new(value: Option<String>) -> Self {
        Self {
            checked: value.is_some(),
            value: value.unwrap_or_default(),
            problem: None,
        }
    }
}

/// A line of the form the cursor can sit on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Row {
    Field(Field),
    Default,
    AutoNotify,
    Command,
    Save,
}

/// What the user pressed, independent of the terminal library.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Key {
    Up,
    Down,
    Enter,
    Char(char),
    Backspace,
    /// Empty the field (Ctrl-U).
    Clear,
    Paste(String),
    /// Test and save (Ctrl-S).
    Save,
    /// Esc: leave, asking again if anything was entered.
    Quit,
    /// Ctrl-C: leave now.
    Cancel,
}

/// What the UI loop should do after a key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Outcome {
    Continue,
    /// Fill the field under the cursor from the clipboard.
    ReadClipboard,
    /// The values are ready: build the webhook, test-ping it if `test`, and save.
    Submit {
        test: bool,
    },
    Quit,
}

/// Where the form starts: flags, the saved webhook, and what herdr and PATH look like.
#[derive(Debug)]
struct Start<'a> {
    name: &'a str,
    current: Option<&'a Webhook>,
    /// `--url` / `--authorization` / `--description`, already checked.
    url: Option<String>,
    authorization: Option<String>,
    description: Option<String>,
    /// The names of the saved webhooks, and which one is the default.
    saved: Vec<String>,
    default_webhook: Option<String>,
    /// `--default`: whether "Make it the default webhook" starts ticked.
    make_default: bool,
    plugin: Plugin,
    /// Whether auto-notify starts ticked.
    auto_notify: bool,
    /// Where `nudge` already is, as shown; `None` offers to link it.
    command: Option<String>,
    /// The shortest turn auto-notify sends, in words.
    min_turn: String,
    test: bool,
}

/// Whether saving test-pings the webhook first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ping {
    Test,
    /// The last test ping failed without blaming a field, so saving again skips it.
    SaveAnyway,
    /// `--no-test`.
    Skip,
}

/// Whether there is anything to lose on quit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Edits {
    None,
    Unsaved,
    /// Esc was pressed once with something entered; the next Esc leaves.
    LeaveOnNextQuit,
}

/// The setup form. Pure state: no terminal, no files, no network.
#[derive(Debug)]
struct Form {
    url: Entry,
    authorization: Entry,
    name: Entry,
    description: Entry,
    /// The saved webhook the form started from, if any.
    loaded: Option<String>,
    saved: Vec<String>,
    default_webhook: Option<String>,
    make_default: bool,
    plugin: Plugin,
    auto_notify: bool,
    command: Option<String>,
    link_command: bool,
    min_turn: String,
    ping: Ping,
    cursor: usize,
    status: Option<String>,
    edits: Edits,
}

impl Form {
    fn new(start: Start<'_>) -> Self {
        let current = start.current;
        Self {
            url: Entry::new(start.url.or_else(|| current.map(|webhook| webhook.url.clone()))),
            authorization: Entry::new(
                start
                    .authorization
                    .or_else(|| current.map(|webhook| webhook.authorization.clone())),
            ),
            // Not checked yet: `--webhook` is only validated when the form is saved or the field is left.
            name: Entry {
                value: start.name.to_owned(),
                ..Entry::default()
            },
            description: Entry::new(
                start
                    .description
                    .or_else(|| current.and_then(|webhook| webhook.description.clone())),
            ),
            loaded: current.map(|webhook| webhook.name.clone()),
            saved: start.saved,
            default_webhook: start.default_webhook,
            make_default: start.make_default,
            plugin: start.plugin,
            auto_notify: start.auto_notify,
            command: start.command,
            link_command: true,
            min_turn: start.min_turn,
            ping: if start.test { Ping::Test } else { Ping::Skip },
            cursor: 0,
            status: None,
            edits: Edits::None,
        }
    }

    /// Whether there's a default to choose: other webhooks are saved and this one isn't the default already.
    fn offers_default(&self) -> bool {
        let name = self.name.value.as_str();
        self.saved.iter().any(|saved| saved != name) && self.default_webhook.as_deref() != Some(name)
    }

    fn rows(&self) -> Vec<Row> {
        let mut rows: Vec<Row> = FIELDS.into_iter().map(Row::Field).collect();
        if self.offers_default() {
            rows.push(Row::Default);
        }
        if matches!(self.plugin, Plugin::Installed(_)) {
            rows.push(Row::AutoNotify);
        }
        if self.command.is_none() {
            rows.push(Row::Command);
        }
        rows.push(Row::Save);
        rows
    }

    fn current(&self) -> Option<Row> {
        self.rows().get(self.cursor).copied()
    }

    fn move_to(&mut self, row: Row) {
        if let Some(index) = self.rows().iter().position(|candidate| *candidate == row) {
            self.cursor = index;
        }
    }

    fn next(&mut self) {
        self.cursor = (self.cursor + 1).min(self.rows().len().saturating_sub(1));
    }

    const fn entry(&self, field: Field) -> &Entry {
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

    fn changed(&mut self) {
        self.edits = Edits::Unsaved;
        if self.ping == Ping::SaveAnyway {
            self.ping = Ping::Test;
        }
        self.status = None;
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
        self.changed();
    }

    /// Check `field` unless it's empty or already checked.
    fn check_field(&mut self, field: Field) {
        let entry = self.entry_mut(field);
        if entry.checked || entry.value.is_empty() {
            return;
        }
        match field.check(&entry.value) {
            Ok(value) => {
                entry.value = value;
                entry.checked = true;
            }
            Err(error) => entry.problem = Some(problem(&error.to_string()).to_owned()),
        }
    }

    /// Check the field under the cursor before the cursor leaves it.
    fn leave(&mut self) {
        if let Some(Row::Field(field)) = self.current() {
            self.check_field(field);
        }
    }

    fn toggle(&mut self, row: Row) {
        match row {
            Row::Default => self.make_default = !self.make_default,
            Row::AutoNotify => self.auto_notify = !self.auto_notify,
            Row::Command => self.link_command = !self.link_command,
            Row::Field(_) | Row::Save => return,
        }
        self.changed();
    }

    /// Check every field; ready to submit unless one needs fixing, which gets the cursor.
    fn submit(&mut self) -> Outcome {
        self.leave();
        for field in FIELDS {
            self.check_field(field);
            let entry = self.entry_mut(field);
            if entry.value.is_empty() {
                entry.problem = match field {
                    Field::Url | Field::Authorization => Some(format!("Paste or type the {}.", field.label())),
                    Field::Name => Some("Give this webhook a name.".into()),
                    Field::Description => None,
                };
            }
            if entry.problem.is_some() {
                self.status = None;
                self.move_to(Row::Field(field));
                return Outcome::Continue;
            }
        }
        Outcome::Submit {
            test: self.ping == Ping::Test,
        }
    }

    fn handle(&mut self, key: Key) -> Outcome {
        let armed = self.edits == Edits::LeaveOnNextQuit;
        if armed {
            self.edits = Edits::Unsaved;
        }
        match (key, self.current()) {
            (Key::Up, _) => {
                self.leave();
                self.cursor = self.cursor.saturating_sub(1);
            }
            (Key::Enter, Some(Row::Field(field @ (Field::Url | Field::Authorization))))
                if self.entry(field).value.is_empty() =>
            {
                return Outcome::ReadClipboard;
            }
            (Key::Down, _) => {
                self.leave();
                self.next();
            }
            // Enter moves on only from a value that passed its check.
            (Key::Enter, Some(Row::Field(field))) => {
                self.check_field(field);
                if self.entry(field).problem.is_none() {
                    self.next();
                }
            }
            (Key::Enter | Key::Char(' '), Some(row @ (Row::Default | Row::AutoNotify | Row::Command))) => {
                self.toggle(row);
            }
            (Key::Enter, Some(Row::Save)) | (Key::Save, _) => return self.submit(),
            (Key::Char(typed), Some(Row::Field(field))) => self.edit(field, |value| value.push(typed)),
            (Key::Paste(text), Some(Row::Field(field))) => self.edit(field, |value| value.push_str(text.trim())),
            (Key::Backspace, Some(Row::Field(field))) => self.edit(field, |value| {
                value.pop();
            }),
            (Key::Clear, Some(Row::Field(field))) => self.edit(field, String::clear),
            (Key::Quit, _) if self.edits != Edits::None && !armed => {
                self.edits = Edits::LeaveOnNextQuit;
                self.status = Some("Press Esc again to leave without saving.".into());
            }
            (Key::Quit | Key::Cancel, _) => return Outcome::Quit,
            (
                Key::Char(_) | Key::Paste(_) | Key::Backspace | Key::Clear,
                Some(Row::Default | Row::AutoNotify | Row::Command | Row::Save) | None,
            )
            | (Key::Enter, None) => {}
        }
        Outcome::Continue
    }

    /// Fill the field under the cursor from the clipboard (`None`: it's empty).
    fn fill_from_clipboard(&mut self, text: Option<String>) {
        let Some(Row::Field(field)) = self.current() else {
            return;
        };
        let label = field.label();
        let Some(text) = text else {
            self.status = Some(format!("No clipboard text found. Type or paste the {label} here."));
            return;
        };
        match field.check(&text) {
            Ok(value) => {
                *self.entry_mut(field) = Entry::new(Some(value));
                self.changed();
                self.next();
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

    /// The webhook to test and save.
    fn webhook(&self, signing_secret: Option<&str>) -> Result<Webhook> {
        let description = Some(self.description.value.as_str()).filter(|text| !text.is_empty());
        validate_webhook(
            &self.name.value,
            &self.url.value,
            &self.authorization.value,
            description,
            signing_secret,
        )
    }

    /// Show why the test ping failed: on the field it blames, or offer to save anyway.
    fn test_failed(&mut self, error: &WebhookError) {
        let reason = explain(error);
        self.status = None;
        let field = match error.blames() {
            Some(Blame::Url) => Field::Url,
            Some(Blame::Authorization) => Field::Authorization,
            None => {
                self.status = Some(format!("✗ {reason}. Press Enter to save it anyway."));
                self.ping = Ping::SaveAnyway;
                self.move_to(Row::Save);
                return;
            }
        };
        self.entry_mut(field).problem = Some(reason);
        self.move_to(Row::Field(field));
    }

    /// The status, or the problem with the field under the cursor.
    fn status(&self) -> Option<String> {
        self.status.clone().or_else(|| match self.current() {
            Some(Row::Field(field)) => self.entry(field).problem.as_ref().map(|problem| format!("✗ {problem}")),
            Some(Row::Default | Row::AutoNotify | Row::Command | Row::Save) | None => None,
        })
    }

    /// What the row under the cursor does.
    fn hint(&self) -> Cow<'static, str> {
        match self.current() {
            Some(Row::Field(Field::Url)) => {
                "Copy the Webhook URL from your webhook's settings and press Enter here to paste it, or type it.".into()
            }
            Some(Row::Field(Field::Authorization)) if self.authorization.checked => {
                "Never shown. To replace it, type or paste a new one; Ctrl-U empties the field.".into()
            }
            Some(Row::Field(Field::Authorization)) => {
                "Copy the Authorization header from the same page and press Enter here to paste it. It's never shown."
                    .into()
            }
            Some(Row::Field(Field::Name)) => {
                "A short name for this webhook, used by nudge list, --webhook and settings. With one webhook, the \
                 default is fine."
                    .into()
            }
            Some(Row::Field(Field::Description)) => {
                "Optional. Shown by nudge list and to agents choosing a webhook.".into()
            }
            Some(Row::Default) => "Notifications go to the default webhook unless they name another one.".into(),
            Some(Row::AutoNotify) => format!(
                "Sends an agent's last terminal lines (secrets redacted) to the webhook when it's blocked, or finishes \
                 a turn longer than {} while you're not watching it.",
                self.min_turn
            )
            .into(),
            Some(Row::Command) => "Links ~/.local/bin/nudge so agents can run nudge notify.".into(),
            Some(Row::Save) => match self.ping {
                Ping::Test => "Sends the webhook a test ping, then saves it (mode 600) and does what's ticked above.",
                Ping::SaveAnyway => "Saves the webhook although its test ping failed.",
                Ping::Skip => "Saves the webhook without a test ping (mode 600) and does what's ticked above.",
            }
            .into(),
            None => "".into(),
        }
    }

    /// What saving does to the webhook of this name: a new one, an update, or replacing another saved webhook.
    fn name_note(&self) -> Option<Span<'static>> {
        let name = self.name.value.as_str();
        if name.is_empty() || self.name.problem.is_some() {
            None
        } else if self.loaded.as_deref() == Some(name) {
            Some(Span::styled("already saved: saving updates it", DIM))
        } else if self.saved.iter().any(|saved| saved == name) {
            Some(Span::styled("replaces the saved webhook with this name", WARN))
        } else {
            Some(Span::styled("will be added", DIM))
        }
    }

    /// A text field's line, and where the text cursor goes when it's focused; values get `room` columns.
    fn field_line(&self, field: Field, focused: bool, room: usize) -> (Line<'static>, Option<usize>) {
        let entry = self.entry(field);
        let mut spans = vec![Span::raw(format!("{INDENT}{:<15}", field.label()))];
        // A header shows only its masked preview once checked; while it's typed, only dots.
        let shown = match field {
            Field::Authorization if entry.checked => format!(
                "{}  ({} characters)",
                mask_secret(&entry.value),
                entry.value.chars().count()
            ),
            Field::Authorization => "•".repeat(entry.value.chars().count().min(room)),
            Field::Url | Field::Name | Field::Description => fit(&entry.value, room, focused),
        };
        let shown = Span::raw(shown);
        // A checked header can't be edited in place (typing replaces it), so it gets no text cursor.
        let caret = (focused && !(field == Field::Authorization && entry.checked)).then(|| LABEL_WIDTH + shown.width());
        spans.push(shown);
        if entry.value.is_empty() {
            let placeholder = match (field, focused) {
                (Field::Url | Field::Authorization, true) => "press Enter to paste, or type",
                (Field::Url | Field::Authorization | Field::Name, _) => "not set",
                (Field::Description, _) => "optional",
            };
            spans.push(Span::styled(placeholder, DIM));
        }
        if entry.problem.is_some() {
            spans.push(Span::styled("  ✗", BAD));
        } else if field == Field::Name {
            spans.extend(
                self.name_note()
                    .map(|note| [Span::raw("  "), note])
                    .into_iter()
                    .flatten(),
            );
        } else if entry.checked && !entry.value.is_empty() && field != Field::Description {
            spans.push(Span::styled("  ✓", GOOD));
        }
        (Line::from(spans), caret)
    }

    /// The lines to draw, which one the cursor is on, and where the text cursor goes on it. Field values get
    /// `room` columns.
    fn lines(&self, room: usize) -> (Vec<Line<'static>>, usize, Option<usize>) {
        let cursor_row = self.current();
        let section = |number: &str, title: &str, note: &'static str| {
            Line::from(vec![
                Span::styled(format!("{number}  {title}"), HEADER),
                Span::styled(note, DIM),
            ])
        };
        // A checkbox or a status mark (padded to a checkbox's width), a label and a detail, all lined up.
        let option = |mark: Span<'static>, label: &str, detail: Option<Span<'static>>| {
            let mut spans = vec![Span::raw(INDENT), mark, Span::raw(format!(" {label:<25}"))];
            spans.extend(detail.map(|detail| [Span::raw("  "), detail]).into_iter().flatten());
            Line::from(spans)
        };
        let mut items: Vec<(Option<Row>, Line<'static>)> = vec![
            (
                None,
                Line::raw("Your webhook is where nudge sends agent events: an AI routine, a"),
            ),
            (None, Line::raw("workflow automation or your own service.")),
            (None, Line::default()),
            (
                None,
                section("1", "Where to send", "  copy both from the webhook's settings"),
            ),
        ];
        let mut caret = None;
        for field in FIELDS {
            if field == Field::Name {
                items.extend([(None, Line::default()), (None, section("2", "This webhook", ""))]);
            }
            let focused = cursor_row == Some(Row::Field(field));
            let (line, at) = self.field_line(field, focused, room);
            if focused {
                caret = at;
            }
            items.push((Some(Row::Field(field)), line));
        }
        if self.offers_default() {
            let line = option(check(self.make_default), "Make it the default webhook", None);
            items.push((Some(Row::Default), line));
        }
        items.extend([(None, Line::default()), (None, section("3", "After saving", ""))]);
        let warn = |text: &'static str, command: &'static str| {
            option(Span::styled(" ! ", WARN), text, Some(Span::styled(command, HEADER)))
        };
        match &self.plugin {
            Plugin::Installed(info) => {
                let state = if info.enabled {
                    Span::styled("herdr plugin enabled", DIM)
                } else {
                    Span::styled("herdr plugin disabled", WARN)
                };
                let line = option(check(self.auto_notify), "Turn on auto-notify", Some(state));
                items.push((Some(Row::AutoNotify), line));
            }
            Plugin::HerdrUnavailable => {
                items.push((
                    None,
                    warn("herdr isn't running", "run nudge setup in herdr for auto-notify"),
                ));
            }
            Plugin::NotInstalled => {
                items.push((
                    None,
                    warn("herdr plugin not installed", "herdr plugin install weskor/nudge"),
                ));
            }
        }
        if let Some(path) = &self.command {
            // Keep the end of a long path: that's where it says what it is.
            let shown = Span::styled(fit(path, room, true), DIM);
            items.push((
                None,
                option(Span::styled(" ✓ ", GOOD), "nudge command installed", Some(shown)),
            ));
        } else {
            let detail = Span::styled("into ~/.local/bin", DIM);
            let line = option(check(self.link_command), "Install the nudge command", Some(detail));
            items.push((Some(Row::Command), line));
        }
        let button = match self.ping {
            Ping::Test => "[ Test and save ]",
            Ping::SaveAnyway => "[ Save anyway ]",
            Ping::Skip => "[ Save ]",
        };
        items.extend([
            (None, Line::default()),
            (
                Some(Row::Save),
                Line::from(vec![Span::raw(INDENT), Span::styled(button, HEADER)]),
            ),
        ]);
        let selected = items
            .iter()
            .position(|(row, _)| row.is_some() && *row == cursor_row)
            .unwrap_or(0);
        (items.into_iter().map(|(_, line)| line).collect(), selected, caret)
    }
}

/// `text` in at most `room` columns: its end (where typing happens) when `typing`, else its start, with `…` for the
/// rest.
fn fit(text: &str, room: usize, typing: bool) -> String {
    let count = text.chars().count();
    if count <= room {
        return text.to_owned();
    }
    let keep = room.saturating_sub(1);
    if typing {
        format!("…{}", text.chars().skip(count - keep).collect::<String>())
    } else {
        format!("{}…", text.chars().take(keep).collect::<String>())
    }
}

fn form_key(event: Event) -> Option<Key> {
    let pressed = match event {
        Event::Paste(text) => return Some(Key::Paste(text)),
        Event::Key(pressed) if pressed.kind == KeyEventKind::Press => pressed,
        Event::Key(_) | Event::FocusGained | Event::FocusLost | Event::Mouse(_) | Event::Resize(..) => return None,
    };
    match (pressed.modifiers.contains(KeyModifiers::CONTROL), pressed.code) {
        (true, KeyCode::Char('c')) => Some(Key::Cancel),
        (true, KeyCode::Char('s')) => Some(Key::Save),
        (true, KeyCode::Char('u')) => Some(Key::Clear),
        (false, KeyCode::Up | KeyCode::BackTab) => Some(Key::Up),
        (false, KeyCode::Down | KeyCode::Tab) => Some(Key::Down),
        (false, KeyCode::Enter) => Some(Key::Enter),
        (false, KeyCode::Backspace) => Some(Key::Backspace),
        (false, KeyCode::Esc) => Some(Key::Quit),
        (false, KeyCode::Char(typed)) => Some(Key::Char(typed)),
        _ => None,
    }
}

fn draw(terminal: &mut DefaultTerminal, form: &Form) -> Result<()> {
    let width = terminal.size()?.width.min(FORM_WIDTH);
    // Leave room for the frame, the label, the caret and the ✓/✗ mark.
    let room = usize::from(width.saturating_sub(ui::FRAME_WIDTH))
        .saturating_sub(LABEL_WIDTH + 4)
        .max(8);
    let (lines, selected, caret) = form.lines(room);
    let status = form.status();
    ui::draw(
        terminal,
        View {
            title: "nudge setup",
            unsaved: false,
            lines,
            selected,
            caret: caret.and_then(|column| u16::try_from(column).ok()),
            note: ui::note(status.as_deref(), &form.hint()),
            help: if form.ping == Ping::Skip { &HELP_NO_TEST } else { &HELP },
            width: Some(width),
        },
    )
}

/// Run the form until the webhook is ready to save, with the test ping's HTTP status if one was sent, or the user
/// leaves (`None`).
fn run_form(
    terminal: &mut DefaultTerminal,
    form: &mut Form,
    signing_secret: Option<&str>,
    env: &Env,
) -> Result<Option<(Webhook, Option<u16>)>> {
    loop {
        draw(terminal, form)?;
        let Some(key) = form_key(event::read()?) else {
            continue;
        };
        match form.handle(key) {
            Outcome::Continue => {}
            Outcome::Quit => return Ok(None),
            Outcome::ReadClipboard => form.fill_from_clipboard(read_clipboard(&System, env, &runtime_dir(env))),
            Outcome::Submit { test } => {
                let webhook = match form.webhook(signing_secret) {
                    Ok(webhook) => webhook,
                    Err(Error::Config(message)) => {
                        form.status = Some(format!("✗ {}", problem(&message)));
                        continue;
                    }
                    Err(other) => return Err(other),
                };
                if !test {
                    return Ok(Some((webhook, None)));
                }
                form.status = Some("Testing…".into());
                draw(terminal, form)?;
                match ping(&webhook, env)? {
                    Ok(status) => return Ok(Some((webhook, Some(status)))),
                    Err(error) => form.test_failed(&error),
                }
            }
        }
    }
}

/// `path` with the home directory shown as `~`.
fn tilde(path: &Path, home: &Path) -> String {
    path.strip_prefix(home)
        .map_or_else(|_| path.display().to_string(), |rest| format!("~/{}", rest.display()))
}

/// Do what the form's ticks asked for after saving, and say so.
fn finish_extras(form: &Form, env: &Env) -> Result<()> {
    match &form.plugin {
        Plugin::Installed(_) => {
            update_state(env, |state| state.auto = form.auto_notify)?;
            let mark = if form.auto_notify { Mark::Ok } else { Mark::Warn };
            eprintln!("{mark} auto-notify {}", if form.auto_notify { "on" } else { "off" });
        }
        Plugin::HerdrUnavailable => eprintln!(
            "{} herdr isn't running, so auto-notify wasn't turned on. Run {} again inside herdr.",
            Mark::Warn,
            Style::Bold.paint("nudge setup")
        ),
        Plugin::NotInstalled => eprintln!(
            "{} The herdr plugin isn't installed: {}",
            Mark::Warn,
            Style::Bold.paint("herdr plugin install weskor/nudge")
        ),
    }
    match &form.command {
        Some(shown) => eprintln!("{} nudge command {shown}", Mark::Ok),
        None if form.link_command => match link_command(&nudge_root(env), env) {
            Ok(path) => eprintln!("{} nudge command {}", Mark::Ok, tilde(&path, &env.home())),
            Err(error) => eprintln!("{} {error}", Mark::Fail),
        },
        None => {}
    }
    Ok(())
}

/// Interactive setup on a terminal; returns the exit code.
pub(super) fn interactive(options: &SetupOptions, env: &Env) -> Result<u8> {
    let path = default_config_path(env);
    let existing = read_config(&path)?;
    let name = options.webhook.as_deref().unwrap_or("main");
    let current = existing
        .as_ref()
        .and_then(|config| config.webhook.iter().find(|webhook| webhook.name == name))
        .cloned();
    let flag = |value: &Option<String>| value.clone().filter(|value| !value.is_empty());
    let url = flag(&options.url).map(|url| check_webhook_url(&url)).transpose()?;
    let authorization = flag(&options.authorization)
        .map(|value| check_authorization(&value))
        .transpose()?;
    let link = command_link(env);
    let mut form = Form::new(Start {
        name,
        current: current.as_ref(),
        url,
        authorization,
        description: options.description.clone(),
        saved: existing
            .as_ref()
            .map(|config| config.webhook.iter().map(|webhook| webhook.name.clone()).collect())
            .unwrap_or_default(),
        default_webhook: existing.as_ref().and_then(|config| config.default_webhook.clone()),
        make_default: options.make_default,
        plugin: plugin(env, &System),
        // A first setup suggests auto-notify; a rerun keeps what's on.
        auto_notify: existing.is_none() || current_state(env).auto,
        // Plugin panes get herdr's server PATH, which usually lacks ~/.local/bin, so also accept the link itself.
        command: env
            .which("nudge")
            .or_else(|| link.exists.then_some(link.path))
            .map(|path| tilde(&path, &env.home())),
        min_turn: seconds_in_words(
            existing
                .as_ref()
                .map_or_else(Auto::default, |config| config.auto)
                .min_turn_seconds,
        ),
        test: options.test,
    });
    let signing_secret = options
        .signing_secret
        .clone()
        .or_else(|| current.as_ref().and_then(|webhook| webhook.signing_secret.clone()));

    let mut terminal = ratatui::try_init()?;
    let ran = execute!(std::io::stdout(), EnableBracketedPaste)
        .map_err(Error::from)
        .and_then(|()| run_form(&mut terminal, &mut form, signing_secret.as_deref(), env));
    let unpasted = execute!(std::io::stdout(), DisableBracketedPaste);
    ratatui::restore();
    unpasted?;
    let Some((webhook, tested)) = ran? else {
        return Err(Error::Cancelled);
    };

    match tested {
        Some(status) => eprintln!("{} Test ping: HTTP {status}, the webhook received it", Mark::Ok),
        None if options.test => eprintln!("{} Saved without a successful test ping", Mark::Warn),
        None => {}
    }
    // Without a choice on screen (no other webhooks, or it's the default already), `--default` decides.
    let make_default = if form.offers_default() {
        form.make_default
    } else {
        options.make_default
    };
    let name = webhook.name.clone();
    let config = save(&path, existing, webhook, make_default)?;
    let as_default = if config.default_webhook.as_deref() == Some(name.as_str()) {
        " as the default webhook"
    } else {
        ""
    };
    let location = Style::Dim.paint(format!("({}, mode 600)", path.display()));
    eprintln!("{} Saved \"{name}\"{as_default} {location}", Mark::Ok);
    finish_extras(&form, env)?;
    eprintln!();
    eprintln!(
        "Done. Check everything any time with {}.",
        Style::Bold.paint("nudge doctor")
    );
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::{Field, Form, Key, Outcome, Row, Start};
    use crate::config::Webhook;
    use crate::error::WebhookError;
    use crate::install::Plugin;

    /// A form with `saved` webhooks (the first is the default), starting from `current` and the given flags.
    fn form(saved: &[&str], current: Option<&Webhook>, url: Option<&str>, authorization: Option<&str>) -> Form {
        Form::new(Start {
            name: "main",
            current,
            url: url.map(str::to_owned),
            authorization: authorization.map(str::to_owned),
            description: None,
            saved: saved.iter().map(|name| (*name).to_owned()).collect(),
            default_webhook: saved.first().map(|name| (*name).to_owned()),
            make_default: false,
            plugin: Plugin::HerdrUnavailable,
            auto_notify: true,
            command: Some("/usr/bin/nudge".into()),
            min_turn: "2 minutes".into(),
            test: true,
        })
    }

    fn screen(form: &Form) -> String {
        let (lines, _, _) = form.lines(60);
        let mut text = lines.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n");
        text.push('\n');
        text.push_str(&form.status().unwrap_or_default());
        text
    }

    fn type_text(form: &mut Form, text: &str) {
        for typed in text.chars() {
            assert_eq!(form.handle(Key::Char(typed)), Outcome::Continue);
        }
    }

    fn failure(status: u16) -> WebhookError {
        WebhookError {
            message: format!("webhook \"main\" returned HTTP {status}: nope"),
            http_status: Some(status),
        }
    }

    #[test]
    fn enter_on_an_empty_field_pastes_the_clipboard_and_a_value_that_fails_its_check_is_not_shown() {
        let mut ui = form(&[], None, None, None);
        assert_eq!(ui.handle(Key::Enter), Outcome::ReadClipboard);
        ui.fill_from_clipboard(Some("Bearer wrong-place-0123456789abcdef".into()));
        let text = screen(&ui);
        assert!(text.contains("✗ The clipboard doesn't hold the Webhook URL"), "{text}");
        assert!(!text.contains("wrong-place"), "{text}");
        assert_eq!(ui.current(), Some(Row::Field(Field::Url)));
        ui.fill_from_clipboard(None);
        assert!(screen(&ui).contains("No clipboard text found"));
        ui.fill_from_clipboard(Some(" https://webhooks.example/hook \n".into()));
        assert!(screen(&ui).contains("https://webhooks.example/hook  ✓"));
        assert_eq!(
            ui.current(),
            Some(Row::Field(Field::Authorization)),
            "moves on to the next field"
        );
    }

    #[test]
    fn the_authorization_header_is_never_on_screen_and_typing_replaces_a_checked_one() {
        let mut ui = form(&[], None, Some("https://webhooks.example/hook"), None);
        ui.handle(Key::Down);
        type_text(&mut ui, "Bearer good-token-0123456789abcdef");
        let typing = screen(&ui);
        assert!(!typing.contains("good-token") && !typing.contains("cdef"), "{typing}");
        ui.handle(Key::Down);
        let checked = screen(&ui);
        assert!(checked.contains("Bearer ••••••••cdef  (34 characters)  ✓"), "{checked}");
        assert!(!checked.contains("good-token"), "{checked}");
        ui.handle(Key::Up);
        type_text(&mut ui, "Bearer new-token-0123456789wxyz");
        assert_eq!(
            ui.webhook(None).unwrap().authorization,
            "Bearer new-token-0123456789wxyz"
        );
    }

    #[test]
    fn missing_values_and_rejected_ones_get_the_cursor_and_block_saving() {
        let mut ui = form(&[], None, None, None);
        assert_eq!(ui.handle(Key::Save), Outcome::Continue);
        assert_eq!(ui.current(), Some(Row::Field(Field::Url)));
        assert!(screen(&ui).contains("✗ Paste or type the Webhook URL."));

        let mut ui = form(
            &[],
            None,
            Some("https://webhooks.example/hook"),
            Some("Bearer token-0123456789abcdef"),
        );
        assert_eq!(ui.handle(Key::Save), Outcome::Submit { test: true });
        ui.test_failed(&failure(401));
        assert_eq!(ui.current(), Some(Row::Field(Field::Authorization)));
        assert!(screen(&ui).contains("rejected the Authorization header"));
        assert_eq!(
            ui.handle(Key::Save),
            Outcome::Continue,
            "the same header isn't sent again"
        );
        type_text(&mut ui, "Bearer other-0123456789abcdef");
        assert_eq!(ui.handle(Key::Save), Outcome::Submit { test: true });
    }

    #[test]
    fn a_failed_test_that_blames_neither_value_offers_saving_anyway_until_something_changes() {
        let mut ui = form(
            &[],
            None,
            Some("https://webhooks.example/hook"),
            Some("Bearer token-0123456789abcdef"),
        );
        assert_eq!(ui.handle(Key::Save), Outcome::Submit { test: true });
        ui.test_failed(&failure(500));
        assert_eq!(ui.current(), Some(Row::Save));
        assert!(screen(&ui).contains("[ Save anyway ]"));
        assert_eq!(ui.handle(Key::Enter), Outcome::Submit { test: false });
        ui.test_failed(&failure(500));
        ui.handle(Key::Up);
        type_text(&mut ui, "Main webhook");
        assert_eq!(ui.handle(Key::Save), Outcome::Submit { test: true });
    }

    #[test]
    fn a_rerun_keeps_the_saved_values_until_they_are_replaced() {
        let saved = Webhook {
            name: "main".into(),
            description: Some("Main webhook".into()),
            url: "https://webhooks.example/hook".into(),
            authorization: "Bearer saved-0123456789abcdef".into(),
            signing_secret: None,
        };
        let mut ui = form(&["main"], Some(&saved), None, None);
        assert!(screen(&ui).contains("already saved: saving updates it"));
        assert_eq!(ui.handle(Key::Save), Outcome::Submit { test: true });
        assert_eq!(ui.webhook(None).unwrap(), saved);
    }

    #[test]
    fn renaming_the_webhook_says_what_saving_will_do_and_offers_the_default() {
        let saved = Webhook {
            name: "main".into(),
            description: None,
            url: "https://webhooks.example/hook".into(),
            authorization: "Bearer saved-0123456789abcdef".into(),
            signing_secret: None,
        };
        let mut ui = form(&["main", "research"], Some(&saved), None, None);
        assert!(!ui.rows().contains(&Row::Default), "it's the default already");
        ui.move_to(Row::Field(Field::Name));
        ui.handle(Key::Clear);
        type_text(&mut ui, "research");
        assert!(screen(&ui).contains("replaces the saved webhook with this name"));
        ui.handle(Key::Clear);
        type_text(&mut ui, "laptop");
        let text = screen(&ui);
        assert!(
            text.contains("laptop  will be added") && text.contains("[ ] Make it the default webhook"),
            "{text}"
        );
        ui.handle(Key::Clear);
        type_text(&mut ui, "my laptop");
        assert_eq!(ui.handle(Key::Save), Outcome::Continue);
        assert!(screen(&ui).contains(r#"may only contain letters, digits, "-" and "_""#));
        ui.handle(Key::Clear);
        type_text(&mut ui, "laptop");
        assert_eq!(ui.handle(Key::Save), Outcome::Submit { test: true });
        assert_eq!(ui.webhook(None).unwrap().name, "laptop");
    }

    #[test]
    fn esc_asks_again_once_something_was_entered_and_ctrl_c_leaves_at_once() {
        let mut ui = form(&[], None, None, None);
        assert_eq!(ui.handle(Key::Quit), Outcome::Quit, "nothing entered");
        let mut ui = form(&[], None, None, None);
        type_text(&mut ui, "https://webhooks.example/hook");
        assert_eq!(ui.handle(Key::Quit), Outcome::Continue);
        assert!(screen(&ui).contains("Press Esc again"));
        assert_eq!(ui.handle(Key::Quit), Outcome::Quit);
        let mut ui = form(&[], None, None, None);
        type_text(&mut ui, "h");
        assert_eq!(ui.handle(Key::Cancel), Outcome::Quit);
    }
}
