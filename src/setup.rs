//! `nudge setup`: connect a desk, test it before saving, then turn on the herdr plugin's auto-notify and put the
//! `nudge` command on PATH. In a terminal it is a full-screen form (clipboard-first); otherwise it reads flags and
//! stdin.
use std::borrow::Cow;
use std::io::{IsTerminal as _, Read as _};
use std::path::{Path, PathBuf};

use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{
    self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEventKind, KeyModifiers,
};
use ratatui::crossterm::execute;
use ratatui::text::{Line, Span};

use crate::clipboard::{read_clipboard, runtime_dir};
use crate::config::{
    Auto, Config, Desk, check_authorization, check_webhook_url, default_config_path, read_config, seconds_in_words,
    upsert_desk, validate_desk, write_config,
};
use crate::context::{Env, System};
use crate::error::{Blame, DeskError, Error, Result};
use crate::install::{Plugin, command_link, link_command, nudge_root, plugin};
use crate::notify::{Origin, current_dir, ping_desk};
use crate::state::{current_state, update_state};
use crate::tty::{Mark, Style, mask_secret, wait_for_key};
use crate::ui::{self, BAD, DIM, GOOD, HEADER, View, WARN, check, toggle};

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
/// Columns before a field's value: indent and label.
const LABEL_WIDTH: usize = 17;

#[derive(Debug, Default)]
pub(crate) struct SetupOptions {
    pub(crate) desk: Option<String>,
    pub(crate) url: Option<String>,
    pub(crate) authorization: Option<String>,
    pub(crate) description: Option<String>,
    /// HMAC signing secret shared with the desk; kept from the existing desk when not given.
    pub(crate) signing_secret: Option<String>,
    pub(crate) make_default: bool,
    /// Test-ping the webhook before saving (off with `--no-test`).
    pub(crate) test: bool,
    /// Running inside a herdr plugin pane: wait for a key before exiting.
    pub(crate) pane: bool,
}

/// A text field of the form.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Field {
    Url,
    Authorization,
    Description,
}

impl Field {
    const fn label(self) -> &'static str {
        match self {
            Self::Url => "Webhook URL",
            Self::Authorization => "Authorization",
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
                        "that's a URL, not the Authorization header. Copy the header value from your desk's webhook settings."
                            .into(),
                    ));
                }
                Ok(checked)
            }
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
    /// The values are ready: build the desk, test-ping it if `test`, and save.
    Submit {
        test: bool,
    },
    Quit,
}

/// Where the form starts: flags, the saved desk, and what herdr and PATH look like.
#[derive(Debug)]
struct Start<'a> {
    name: &'a str,
    current: Option<&'a Desk>,
    /// `--url` / `--authorization` / `--description`, already checked.
    url: Option<String>,
    authorization: Option<String>,
    description: Option<String>,
    /// `Some` when other desks exist and this one isn't the default yet: whether to make it the default.
    make_default: Option<bool>,
    plugin: Plugin,
    /// Whether auto-notify starts ticked.
    auto_notify: bool,
    /// Where `nudge` already is; `None` offers to link it.
    command: Option<PathBuf>,
    /// The shortest turn auto-notify sends, in words.
    min_turn: String,
    test: bool,
}

/// Whether saving test-pings the desk first.
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
    name: String,
    updating: bool,
    url: Entry,
    authorization: Entry,
    description: Entry,
    make_default: Option<bool>,
    plugin: Plugin,
    auto_notify: bool,
    command: Option<PathBuf>,
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
            name: start.name.to_owned(),
            updating: current.is_some(),
            url: Entry::new(start.url.or_else(|| current.map(|desk| desk.url.clone()))),
            authorization: Entry::new(
                start
                    .authorization
                    .or_else(|| current.map(|desk| desk.authorization.clone())),
            ),
            description: Entry::new(
                start
                    .description
                    .or_else(|| current.and_then(|desk| desk.description.clone())),
            ),
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

    fn rows(&self) -> Vec<Row> {
        let mut rows = vec![
            Row::Field(Field::Url),
            Row::Field(Field::Authorization),
            Row::Field(Field::Description),
        ];
        if self.make_default.is_some() {
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
            Field::Description => &self.description,
        }
    }

    const fn entry_mut(&mut self, field: Field) -> &mut Entry {
        match field {
            Field::Url => &mut self.url,
            Field::Authorization => &mut self.authorization,
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
            Row::Default => self.make_default = self.make_default.map(|on| !on),
            Row::AutoNotify => self.auto_notify = !self.auto_notify,
            Row::Command => self.link_command = !self.link_command,
            Row::Field(_) | Row::Save => return,
        }
        self.changed();
    }

    /// Check every field; ready to submit unless one needs fixing, which gets the cursor.
    fn submit(&mut self) -> Outcome {
        self.leave();
        for field in [Field::Url, Field::Authorization, Field::Description] {
            self.check_field(field);
            let entry = self.entry_mut(field);
            if field != Field::Description && entry.value.is_empty() {
                entry.problem = Some(format!("Paste or type the {}.", field.label()));
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

    /// The desk to test and save.
    fn desk(&self, signing_secret: Option<&str>) -> Result<Desk> {
        let description = Some(self.description.value.as_str()).filter(|text| !text.is_empty());
        validate_desk(
            &self.name,
            &self.url.value,
            &self.authorization.value,
            description,
            signing_secret,
        )
    }

    /// Show why the test ping failed: on the field it blames, or offer to save anyway.
    fn test_failed(&mut self, error: &DeskError) {
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
                "Copy the Webhook URL from your desk's webhook settings and press Enter here to paste it, or type it."
                    .into()
            }
            Some(Row::Field(Field::Authorization)) if self.authorization.checked => {
                "Never shown. To replace it, type or paste a new one; Ctrl-U empties the field.".into()
            }
            Some(Row::Field(Field::Authorization)) => {
                "Copy the Authorization header from the same page and press Enter here to paste it. It's never shown."
                    .into()
            }
            Some(Row::Field(Field::Description)) => {
                "Optional. Shown by nudge list and to agents choosing a desk.".into()
            }
            Some(Row::Default) => "Notifications go to the default desk unless they name another one.".into(),
            Some(Row::AutoNotify) => format!(
                "Sends an agent's last terminal lines (secrets redacted) to the desk when it's blocked, or finishes \
                 a turn longer than {} while you're not watching it.",
                self.min_turn
            )
            .into(),
            Some(Row::Command) => "Links ~/.local/bin/nudge so agents can run nudge notify.".into(),
            Some(Row::Save) => match self.ping {
                Ping::Test => "Sends the desk a test ping, then saves it (mode 600) and does what's ticked above.",
                Ping::SaveAnyway => "Saves the desk although its test ping failed.",
                Ping::Skip => "Saves the desk without a test ping (mode 600) and does what's ticked above.",
            }
            .into(),
            None => "".into(),
        }
    }

    /// A text field's line; values get `room` columns.
    fn field_line(&self, field: Field, focused: bool, room: usize) -> Line<'static> {
        let entry = self.entry(field);
        let mut spans = vec![Span::raw(format!("  {:<15}", field.label()))];
        let hidden_checked = field == Field::Authorization && entry.checked;
        // A header shows only its masked preview once checked; while it's typed, only dots.
        let shown = match field {
            Field::Authorization if entry.checked => format!(
                "{}  ({} characters)",
                mask_secret(&entry.value),
                entry.value.chars().count()
            ),
            Field::Authorization => "•".repeat(entry.value.chars().count().min(room)),
            Field::Url | Field::Description => fit(&entry.value, room, focused),
        };
        spans.push(Span::raw(shown));
        if focused && !hidden_checked {
            spans.push(Span::raw("▏"));
        }
        if entry.value.is_empty() {
            let placeholder = match field {
                Field::Url | Field::Authorization => "Enter pastes from the clipboard",
                Field::Description => "optional",
            };
            spans.push(Span::styled(placeholder, DIM));
        }
        if entry.problem.is_some() {
            spans.push(Span::styled("  ✗", BAD));
        } else if entry.checked && !entry.value.is_empty() {
            spans.push(Span::styled("  ✓", GOOD));
        }
        Line::from(spans)
    }

    /// The lines to draw, and which one the cursor is on. Field values get `room` columns.
    fn lines(&self, room: usize) -> (Vec<Line<'static>>, usize) {
        let cursor_row = self.current();
        let state = if self.updating { "  saved desk" } else { "  new desk" };
        let mut items: Vec<(Option<Row>, Line<'static>)> = vec![(
            None,
            Line::from(vec![
                Span::styled(format!("Desk \"{}\"", self.name), HEADER),
                Span::styled(state, DIM),
            ]),
        )];
        for field in [Field::Url, Field::Authorization, Field::Description] {
            let line = self.field_line(field, cursor_row == Some(Row::Field(field)), room);
            items.push((Some(Row::Field(field)), line));
        }
        if let Some(on) = self.make_default {
            let label = format!(" Make \"{}\" the default desk", self.name);
            items.push((Some(Row::Default), toggle(check(on), label)));
        }
        items.extend([(None, Line::default()), (None, Line::styled("After saving", HEADER))]);
        let warn = |text: &'static str, command: &'static str| {
            Line::from(vec![
                Span::raw("  "),
                Span::styled("!", WARN),
                Span::raw(text),
                Span::styled(command, HEADER),
            ])
        };
        match &self.plugin {
            Plugin::Installed(info) => {
                let mut line = toggle(check(self.auto_notify), " Turn on auto-notify      ");
                line.push_span(if info.enabled {
                    Span::styled("  herdr plugin enabled", DIM)
                } else {
                    Span::styled("  herdr plugin disabled", WARN)
                });
                items.push((Some(Row::AutoNotify), line));
            }
            Plugin::HerdrUnavailable => {
                items.push((
                    None,
                    warn(" herdr isn't running: for auto-notify, run ", "nudge setup in herdr"),
                ));
            }
            Plugin::NotInstalled => {
                items.push((
                    None,
                    warn(" herdr plugin not installed: ", "herdr plugin install weskor/nudge"),
                ));
            }
        }
        if let Some(path) = &self.command {
            items.push((
                None,
                Line::from(vec![
                    Span::raw("  "),
                    Span::styled("✓", GOOD),
                    Span::raw(" nudge command  "),
                    // Keep the end of a long path: that's where it says what it is.
                    Span::styled(fit(&path.display().to_string(), room, true), DIM),
                ]),
            ));
        } else {
            let mut line = toggle(check(self.link_command), " Install the nudge command");
            line.push_span(Span::styled("  into ~/.local/bin", DIM));
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
                Line::from(vec![Span::raw("  "), Span::styled(button, HEADER)]),
            ),
        ]);
        let selected = items
            .iter()
            .position(|(row, _)| row.is_some() && *row == cursor_row)
            .unwrap_or(0);
        (items.into_iter().map(|(_, line)| line).collect(), selected)
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

/// Plain-language reason for a failed test ping.
fn explain(error: &DeskError) -> String {
    let detail = error
        .message
        .strip_prefix("desk \"")
        .and_then(|rest| rest.split_once("\" webhook "))
        .map_or(error.message.as_str(), |(_, detail)| detail);
    match error.blames() {
        Some(Blame::Authorization) => format!("the webhook rejected the Authorization header ({detail})"),
        Some(Blame::Url) => format!("there's no webhook at that URL ({detail})"),
        None => detail.to_owned(),
    }
}

/// Test-ping `desk` alone. A webhook failure is returned, not raised.
fn ping(desk: &Desk, env: &Env) -> Result<Result<u16, DeskError>> {
    let config = Config {
        desk: vec![desk.clone()],
        ..Config::default()
    };
    let cwd = current_dir()?;
    match ping_desk(
        &config,
        Some(&desk.name),
        None,
        Origin {
            env,
            cwd: &cwd,
            run: &System,
        },
    ) {
        Ok(sent) => Ok(Ok(sent.http_status)),
        Err(Error::Desk(error)) => Ok(Err(error)),
        Err(other) => Err(other),
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
    let (lines, selected) = form.lines(room);
    let status = form.status();
    ui::draw(
        terminal,
        View {
            title: "nudge setup",
            unsaved: false,
            lines,
            selected,
            note: ui::note(status.as_deref(), &form.hint()),
            help: if form.ping == Ping::Skip { &HELP_NO_TEST } else { &HELP },
            width: Some(width),
        },
    )
}

/// Run the form until the desk is ready to save, with the test ping's HTTP status if one was sent, or the user
/// leaves (`None`).
fn run_form(
    terminal: &mut DefaultTerminal,
    form: &mut Form,
    signing_secret: Option<&str>,
    env: &Env,
) -> Result<Option<(Desk, Option<u16>)>> {
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
                let desk = match form.desk(signing_secret) {
                    Ok(desk) => desk,
                    Err(Error::Config(message)) => {
                        form.status = Some(format!("✗ {}", problem(&message)));
                        continue;
                    }
                    Err(other) => return Err(other),
                };
                if !test {
                    return Ok(Some((desk, None)));
                }
                form.status = Some("Testing…".into());
                draw(terminal, form)?;
                match ping(&desk, env)? {
                    Ok(status) => return Ok(Some((desk, Some(status)))),
                    Err(error) => form.test_failed(&error),
                }
            }
        }
    }
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
        Some(path) => eprintln!("{} nudge command {}", Mark::Ok, path.display()),
        None if form.link_command => match link_command(&nudge_root(env), env) {
            Ok(path) => eprintln!("{} nudge command {}", Mark::Ok, path.display()),
            Err(error) => eprintln!("{} {error}", Mark::Fail),
        },
        None => {}
    }
    Ok(())
}

/// Save `desk` into the config at `path`; returns the new config.
fn save(path: &Path, existing: Option<Config>, desk: Desk, make_default: bool) -> Result<Config> {
    let config = upsert_desk(existing, desk, make_default);
    write_config(path, &config)?;
    Ok(config)
}

/// Interactive setup on a terminal; returns the exit code.
fn interactive(options: &SetupOptions, env: &Env) -> Result<u8> {
    let path = default_config_path(env);
    let existing = read_config(&path)?;
    let name = options.desk.as_deref().unwrap_or("desk");
    let current = existing
        .as_ref()
        .and_then(|config| config.desk.iter().find(|desk| desk.name == name))
        .cloned();
    let flag = |value: &Option<String>| value.clone().filter(|value| !value.is_empty());
    let url = flag(&options.url).map(|url| check_webhook_url(&url)).transpose()?;
    let authorization = flag(&options.authorization)
        .map(|value| check_authorization(&value))
        .transpose()?;
    // Offer the default only when there's a choice: other desks, and this one isn't the default already.
    let make_default = existing
        .as_ref()
        .is_some_and(|config| {
            config.desk.iter().any(|desk| desk.name != name) && config.default_desk.as_deref() != Some(name)
        })
        .then_some(options.make_default);
    let link = command_link(env);
    let mut form = Form::new(Start {
        name,
        current: current.as_ref(),
        url,
        authorization,
        description: options.description.clone(),
        make_default,
        plugin: plugin(env, &System),
        // A first setup suggests auto-notify; a rerun keeps what's on.
        auto_notify: existing.is_none() || current_state(env).auto,
        // Plugin panes get herdr's server PATH, which usually lacks ~/.local/bin, so also accept the link itself.
        command: env.which("nudge").or_else(|| link.exists.then_some(link.path)),
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
        .or_else(|| current.as_ref().and_then(|desk| desk.signing_secret.clone()));

    let mut terminal = ratatui::try_init()?;
    let ran = execute!(std::io::stdout(), EnableBracketedPaste)
        .map_err(Error::from)
        .and_then(|()| run_form(&mut terminal, &mut form, signing_secret.as_deref(), env));
    let unpasted = execute!(std::io::stdout(), DisableBracketedPaste);
    ratatui::restore();
    unpasted?;
    let Some((desk, tested)) = ran? else {
        return Err(Error::Cancelled);
    };

    match tested {
        Some(status) => eprintln!("{} Test ping: HTTP {status}, the desk received it", Mark::Ok),
        None if options.test => eprintln!("{} Saved without a successful test ping", Mark::Warn),
        None => {}
    }
    let config = save(&path, existing, desk, form.make_default.unwrap_or(options.make_default))?;
    let as_default = if config.default_desk.as_deref() == Some(name) {
        " as the default desk"
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

/// Non-interactive setup from flags and stdin (URL, then Authorization, one per line).
fn scripted(options: &SetupOptions, env: &Env) -> Result<u8> {
    let path = default_config_path(env);
    let existing = read_config(&path)?;
    let name = options.desk.as_deref().unwrap_or("desk");
    let current = existing
        .as_ref()
        .and_then(|config| config.desk.iter().find(|desk| desk.name == name))
        .cloned();
    let mut url = options.url.clone();
    let mut authorization = options.authorization.clone();
    if url.is_none() || authorization.is_none() {
        let mut input = String::new();
        std::io::stdin().read_to_string(&mut input)?;
        let mut lines = input
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_owned);
        url = url.or_else(|| lines.next());
        authorization = authorization.or_else(|| lines.next());
    }
    let url = url.or_else(|| current.as_ref().map(|desk| desk.url.clone()));
    let authorization = authorization.or_else(|| current.as_ref().map(|desk| desk.authorization.clone()));
    let (Some(url), Some(authorization)) = (url, authorization) else {
        return Err(Error::Setup(
            "setup needs the webhook URL and Authorization value: run it in a terminal, pass --url/--authorization, \
             or pipe them on stdin (one per line)"
                .into(),
        ));
    };
    let signing_secret = options
        .signing_secret
        .clone()
        .or_else(|| current.as_ref().and_then(|desk| desk.signing_secret.clone()));
    let description = options
        .description
        .clone()
        .or_else(|| current.and_then(|desk| desk.description));
    let desk = validate_desk(
        name,
        &url,
        &authorization,
        description.as_deref(),
        signing_secret.as_deref(),
    )?;
    if options.test {
        match ping(&desk, env)? {
            Ok(status) => println!("Test ping to \"{}\": HTTP {status}.", desk.name),
            Err(error) => {
                return Err(Error::Setup(format!(
                    "test ping failed, nothing saved: {}. Use --no-test to save without testing.",
                    explain(&error)
                )));
            }
        }
    }
    let config = save(&path, existing, desk, options.make_default)?;
    let as_default = if config.default_desk.as_deref() == Some(name) {
        " (default)"
    } else {
        ""
    };
    println!("Saved desk \"{name}\"{as_default} to {} (mode 600).", path.display());
    Ok(0)
}

pub(crate) fn setup(options: &SetupOptions, env: &Env) -> Result<u8> {
    if !std::io::stdin().is_terminal() {
        return scripted(options, env);
    }
    let result = match interactive(options, env) {
        Err(Error::Cancelled) => {
            eprintln!("{}", Style::Dim.paint("Cancelled; nothing saved."));
            Ok(1)
        }
        other => other,
    };
    if options.pane {
        wait_for_key();
    }
    result
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt as _;
    use std::path::PathBuf;

    use super::{Field, Form, Key, Outcome, Row, SetupOptions, Start, scripted};
    use crate::config::{Desk, read_config};
    use crate::context::Env;
    use crate::error::{DeskError, Error};
    use crate::install::Plugin;
    use crate::notify::tests::FakeWebhook;

    const GOOD: &str = "Bearer good-token-0123456789";

    /// A webhook at `/hook` that accepts only GOOD, plus a fresh config path.
    fn fixture() -> (FakeWebhook, tempfile::TempDir, Env) {
        let hook = FakeWebhook::start(
            |request| match (request.path.as_str(), request.header("authorization")) {
                ("/hook", Some(GOOD)) => (200, "ok".into()),
                ("/hook", _) => (401, r#"{"message":"Invalid API key"}"#.into()),
                _ => (404, "no such hook".into()),
            },
        );
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config.toml");
        let env = Env::from([
            ("NUDGE_CONFIG", config.to_str().unwrap()),
            ("NUDGE_STATE_DIR", dir.path().to_str().unwrap()),
            ("NUDGE_AGENT", "setup-test"),
        ]);
        (hook, dir, env)
    }

    fn options(url: String, authorization: &str) -> SetupOptions {
        SetupOptions {
            url: Some(url),
            authorization: Some(authorization.into()),
            test: true,
            ..SetupOptions::default()
        }
    }

    fn setup_error(result: crate::error::Result<u8>) -> String {
        match result {
            Err(Error::Setup(message)) => message,
            other => panic!("expected a setup error, got {other:?}"),
        }
    }

    #[test]
    fn tests_the_webhook_before_saving_and_saves_mode_600() {
        let (hook, dir, env) = fixture();
        let url = format!("{}/hook", hook.base);
        assert_eq!(scripted(&options(url.clone(), GOOD), &env).unwrap(), 0);
        assert_eq!(
            hook.requests()
                .iter()
                .map(|r| r.header("authorization").unwrap().to_owned())
                .collect::<Vec<_>>(),
            [GOOD]
        );
        let path = dir.path().join("config.toml");
        let desk = &read_config(&path).unwrap().unwrap().desk[0];
        assert_eq!(
            (desk.name.as_str(), desk.url.as_str(), desk.authorization.as_str()),
            ("desk", url.as_str(), GOOD)
        );
        assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
    }

    #[test]
    fn a_rejected_authorization_header_saves_nothing_and_says_why() {
        let (hook, dir, env) = fixture();
        let message = setup_error(scripted(
            &options(format!("{}/hook", hook.base), "Bearer wrong-token-0123456789"),
            &env,
        ));
        assert!(
            message.contains("nothing saved") && message.contains("rejected the Authorization header"),
            "{message}"
        );
        assert_eq!(read_config(&dir.path().join("config.toml")).unwrap(), None);
    }

    #[test]
    fn a_wrong_url_is_reported_as_such() {
        let (hook, _dir, env) = fixture();
        let message = setup_error(scripted(&options(format!("{}/nope", hook.base), GOOD), &env));
        assert!(message.contains("no webhook at that URL"), "{message}");
    }

    #[test]
    fn no_test_saves_without_contacting_the_webhook() {
        let (hook, dir, env) = fixture();
        let options = SetupOptions {
            test: false,
            ..options(format!("{}/hook", hook.base), "Bearer offline-token-0123456789")
        };
        assert_eq!(scripted(&options, &env).unwrap(), 0);
        assert!(hook.requests().is_empty());
        assert_eq!(
            read_config(&dir.path().join("config.toml"))
                .unwrap()
                .unwrap()
                .desk
                .len(),
            1
        );
    }

    #[test]
    fn errors_never_contain_the_authorization_value() {
        let (hook, _dir, env) = fixture();
        let message = setup_error(scripted(
            &options(format!("{}/hook", hook.base), "Bearer do-not-print-0123456789"),
            &env,
        ));
        assert!(!message.contains("do-not-print"));
    }

    fn form(current: Option<&Desk>, url: Option<&str>, authorization: Option<&str>) -> Form {
        Form::new(Start {
            name: "desk",
            current,
            url: url.map(str::to_owned),
            authorization: authorization.map(str::to_owned),
            description: None,
            make_default: None,
            plugin: Plugin::HerdrUnavailable,
            auto_notify: true,
            command: Some(PathBuf::from("/usr/bin/nudge")),
            min_turn: "2 minutes".into(),
            test: true,
        })
    }

    fn screen(form: &Form) -> String {
        let (lines, _) = form.lines(60);
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

    fn failure(status: u16) -> DeskError {
        DeskError {
            message: format!("desk \"desk\" webhook returned HTTP {status}: nope"),
            http_status: Some(status),
        }
    }

    #[test]
    fn enter_on_an_empty_field_pastes_the_clipboard_and_a_value_that_fails_its_check_is_not_shown() {
        let mut ui = form(None, None, None);
        assert_eq!(ui.handle(Key::Enter), Outcome::ReadClipboard);
        ui.fill_from_clipboard(Some("Bearer wrong-place-0123456789abcdef".into()));
        let text = screen(&ui);
        assert!(text.contains("✗ The clipboard doesn't hold the Webhook URL"), "{text}");
        assert!(!text.contains("wrong-place"), "{text}");
        assert_eq!(ui.current(), Some(Row::Field(Field::Url)));
        ui.fill_from_clipboard(None);
        assert!(screen(&ui).contains("No clipboard text found"));
        ui.fill_from_clipboard(Some(" https://desks.example/hook \n".into()));
        assert!(screen(&ui).contains("https://desks.example/hook  ✓"));
        assert_eq!(
            ui.current(),
            Some(Row::Field(Field::Authorization)),
            "moves on to the next field"
        );
    }

    #[test]
    fn the_authorization_header_is_never_on_screen_and_typing_replaces_a_checked_one() {
        let mut ui = form(None, Some("https://desks.example/hook"), None);
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
        assert_eq!(ui.desk(None).unwrap().authorization, "Bearer new-token-0123456789wxyz");
    }

    #[test]
    fn missing_values_and_rejected_ones_get_the_cursor_and_block_saving() {
        let mut ui = form(None, None, None);
        assert_eq!(ui.handle(Key::Save), Outcome::Continue);
        assert_eq!(ui.current(), Some(Row::Field(Field::Url)));
        assert!(screen(&ui).contains("✗ Paste or type the Webhook URL."));

        let mut ui = form(
            None,
            Some("https://desks.example/hook"),
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
            None,
            Some("https://desks.example/hook"),
            Some("Bearer token-0123456789abcdef"),
        );
        assert_eq!(ui.handle(Key::Save), Outcome::Submit { test: true });
        ui.test_failed(&failure(500));
        assert_eq!(ui.current(), Some(Row::Save));
        assert!(screen(&ui).contains("[ Save anyway ]"));
        assert_eq!(ui.handle(Key::Enter), Outcome::Submit { test: false });
        ui.test_failed(&failure(500));
        ui.handle(Key::Up);
        ui.handle(Key::Up);
        type_text(&mut ui, "Main desk");
        assert_eq!(ui.handle(Key::Save), Outcome::Submit { test: true });
    }

    #[test]
    fn a_rerun_keeps_the_saved_values_until_they_are_replaced() {
        let saved = Desk {
            name: "desk".into(),
            description: Some("Main desk".into()),
            url: "https://desks.example/hook".into(),
            authorization: "Bearer saved-0123456789abcdef".into(),
            signing_secret: None,
        };
        let mut ui = form(Some(&saved), None, None);
        assert!(screen(&ui).contains("saved desk"));
        assert_eq!(ui.handle(Key::Save), Outcome::Submit { test: true });
        assert_eq!(ui.desk(None).unwrap(), saved);
    }

    #[test]
    fn esc_asks_again_once_something_was_entered_and_ctrl_c_leaves_at_once() {
        let mut ui = form(None, None, None);
        assert_eq!(ui.handle(Key::Quit), Outcome::Quit, "nothing entered");
        let mut ui = form(None, None, None);
        type_text(&mut ui, "https://desks.example/hook");
        assert_eq!(ui.handle(Key::Quit), Outcome::Continue);
        assert!(screen(&ui).contains("Press Esc again"));
        assert_eq!(ui.handle(Key::Quit), Outcome::Quit);
        let mut ui = form(None, None, None);
        type_text(&mut ui, "h");
        assert_eq!(ui.handle(Key::Cancel), Outcome::Quit);
    }
}
