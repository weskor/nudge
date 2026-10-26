//! The wizard on screen: its lines for each step, and the terminal loop that reads keys, runs the clipboard reads
//! and the test ping on worker threads (so the screen never freezes) and saves while the screen is still up.
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::text::{Line, Span};

use super::ping;
use super::wizard::{Choice, FailChoice, Field, Key, Outcome, SaveState, Step, Wizard};
use crate::clipboard::{Desktop, read_clipboard, runtime_dir};
use crate::config::Config;
use crate::env::{Env, System};
use crate::error::{Blame, Error, Result, WebhookError};
use crate::herdr::Plugin;
use crate::terminal::tty::mask_secret;
use crate::terminal::ui::{self, BAD, DIM, GOOD, HEADER, StepMark, View, WARN, check, radio};

/// How often the screen redraws while waiting, so the spinner turns.
const TICK: Duration = Duration::from_millis(80);
/// The wizard's width (or the screen's, if narrower): room for the step list and a typical webhook URL.
const WIDTH: u16 = 96;
/// Columns a field's label takes before its value.
const LABEL: usize = 15;
const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

const HELP_CONNECT: [(&str, &str); 4] = [
    ("enter", "next"),
    ("tab", "field"),
    ("esc", "leave"),
    ("ctrl-c", "quit"),
];
const HELP_NAME: [(&str, &str); 4] = [
    ("enter", "next"),
    ("tab", "field"),
    ("shift+tab/esc", "back"),
    ("ctrl-c", "quit"),
];
const HELP_OPTIONS: [(&str, &str); 4] = [
    ("enter", "accept"),
    ("e", "change"),
    ("shift+tab/esc", "back"),
    ("q", "quit"),
];
const HELP_EDITING: [(&str, &str); 4] = [("↑↓", "move"), ("space", "tick"), ("enter", "done"), ("q", "quit")];
const HELP_FAILED: [(&str, &str); 4] = [
    ("↑↓", "choose"),
    ("enter", "go"),
    ("shift+tab/esc", "back"),
    ("q", "quit"),
];
const HELP_TESTING: [(&str, &str); 1] = [("ctrl-c", "stop")];
const HELP_NOT_SAVED: [(&str, &str); 3] = [("enter", "try again"), ("shift+tab/esc", "back"), ("q", "quit")];

fn key(event: Event) -> Option<Key> {
    let pressed = match event {
        Event::Paste(text) => return Some(Key::Paste(text)),
        Event::Key(pressed) if pressed.kind == KeyEventKind::Press => pressed,
        Event::Key(_) | Event::FocusGained | Event::FocusLost | Event::Mouse(_) | Event::Resize(..) => return None,
    };
    match (pressed.modifiers.contains(KeyModifiers::CONTROL), pressed.code) {
        (true, KeyCode::Char('c')) => Some(Key::Cancel),
        (true, KeyCode::Char('u')) => Some(Key::Clear),
        (false, KeyCode::Up) => Some(Key::Up),
        (false, KeyCode::Down) => Some(Key::Down),
        (false, KeyCode::Tab) => Some(Key::Tab),
        (false, KeyCode::BackTab) => Some(Key::BackTab),
        (false, KeyCode::Enter) => Some(Key::Enter),
        (false, KeyCode::Esc) => Some(Key::Esc),
        (false, KeyCode::Backspace) => Some(Key::Backspace),
        (false, KeyCode::Char(typed)) => Some(Key::Char(typed)),
        _ => None,
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

/// A text field's lines (its value, then where it came from, if known) and where the text cursor goes when it's
/// focused; values get `room` columns.
fn field_lines(wizard: &Wizard, field: Field, room: usize) -> (Vec<Line<'static>>, Option<usize>) {
    let entry = wizard.entry(field);
    let focused = wizard.focus() == Some(field);
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
    let caret = (focused && !(field == Field::Authorization && entry.checked)).then(|| LABEL + shown.width());
    let mut spans = vec![Span::raw(format!("{:<LABEL$}", field.label())), shown];
    if entry.value.is_empty() {
        let placeholder = match (field, focused) {
            (Field::Url | Field::Authorization, true) => "press Enter to paste, or type",
            (Field::Url | Field::Authorization | Field::Name, false) => "not set",
            (Field::Name, true) => "",
            (Field::Description, _) => "optional",
        };
        spans.push(Span::styled(placeholder, DIM));
    }
    if entry.problem.is_some() {
        spans.push(Span::styled("  ✗", BAD));
    } else if entry.checked && !entry.value.is_empty() && matches!(field, Field::Url | Field::Authorization) {
        spans.push(Span::styled("  ✓", GOOD));
    } else if field == Field::Name && wizard.loaded.as_ref() == Some(&entry.value) {
        spans.push(Span::styled("  saved: saving updates it", DIM));
    }
    let mut lines = vec![Line::from(spans)];
    if let Some(source) = entry
        .source
        .filter(|_| matches!(field, Field::Url | Field::Authorization))
    {
        lines.push(Line::styled(format!("{:LABEL$}{}", "", source.label()), DIM));
    }
    (lines, caret)
}

/// Lines with a cursor stop each; `selected` is where the cursor is, `caret` where typing goes on it.
#[derive(Default)]
struct Content {
    lines: Vec<Line<'static>>,
    selected: Option<usize>,
    caret: Option<usize>,
}

impl Content {
    fn text(&mut self, lines: &[&'static str]) {
        self.lines.extend(lines.iter().map(|line| Line::raw(*line)));
        self.lines.push(Line::default());
    }

    fn fields(&mut self, wizard: &Wizard, fields: [Field; 2], room: usize) {
        for field in fields {
            let (lines, caret) = field_lines(wizard, field, room);
            if wizard.focus() == Some(field) {
                self.selected = Some(self.lines.len());
                self.caret = caret;
            }
            self.lines.extend(lines);
        }
    }

    /// A line the cursor can sit on, selected when `here`.
    fn stop(&mut self, line: Line<'static>, here: bool) {
        if here {
            self.selected = Some(self.lines.len());
        }
        self.lines.push(line);
    }
}

/// Step 3's lines: what nudge will do (or checkboxes to change it), and what isn't a choice.
fn options(wizard: &Wizard, content: &mut Content, room: usize) {
    let name = &wizard.entry(Field::Name).value;
    let choices = wizard.choices();
    let offered = wizard.offered();
    let plugin_state = match &choices.plugin {
        Plugin::Installed(info) if info.enabled => Span::styled("  herdr plugin enabled", DIM),
        Plugin::Installed(_) => Span::styled("  herdr plugin disabled", WARN),
        Plugin::HerdrUnavailable | Plugin::NotInstalled => Span::raw(""),
    };
    if let Some(cursor) = wizard.editing() {
        content.text(&["Change what nudge will do after saving:"]);
        for (index, choice) in offered.iter().enumerate() {
            let (on, label, detail) = match choice {
                Choice::AutoNotify => (
                    choices.auto_notify == Some(true),
                    "Turn on auto-notify".to_owned(),
                    plugin_state.clone(),
                ),
                Choice::Command => (
                    choices.link_command,
                    "Install the nudge command".to_owned(),
                    Span::styled("  into ~/.local/bin", DIM),
                ),
                Choice::Default => (
                    choices.make_default,
                    format!("Make \"{name}\" the default webhook"),
                    Span::raw(""),
                ),
            };
            let line = Line::from(vec![
                Span::raw("  "),
                check(on),
                Span::raw(format!(" {label:<32}")),
                detail,
            ]);
            content.stop(line, index == cursor);
        }
    } else {
        content.text(&["After saving, nudge will:"]);
        for choice in &offered {
            let (on, yes, no) = match choice {
                Choice::AutoNotify => (
                    choices.auto_notify == Some(true),
                    "Turn on auto-notify".to_owned(),
                    "Leave auto-notify off".to_owned(),
                ),
                Choice::Command => (
                    choices.link_command,
                    "Install the nudge command into ~/.local/bin".to_owned(),
                    "Not install the nudge command".to_owned(),
                ),
                Choice::Default => (
                    choices.make_default,
                    format!("Make \"{name}\" the default webhook"),
                    format!(
                        "Keep \"{}\" as the default webhook",
                        wizard.default_webhook.as_deref().unwrap_or_default()
                    ),
                ),
            };
            let mut spans = if on {
                vec![Span::styled("  ✓ ", GOOD), Span::raw(yes)]
            } else {
                vec![Span::styled("  ○ ", DIM), Span::styled(no, DIM)]
            };
            if *choice == Choice::AutoNotify {
                spans.push(plugin_state.clone());
            }
            content.lines.push(Line::from(spans));
        }
    }
    let warn = |text: &'static str, command: &'static str| {
        Line::from(vec![
            Span::styled("  ! ", WARN),
            Span::raw(text),
            Span::styled(command, HEADER),
        ])
    };
    match &choices.plugin {
        Plugin::HerdrUnavailable => {
            content.lines.push(warn(
                "herdr isn't running: for auto-notify, run ",
                "nudge setup in herdr",
            ));
        }
        Plugin::NotInstalled => {
            content.lines.push(warn(
                "herdr plugin not installed: ",
                "herdr plugin install weskor/nudge",
            ));
        }
        Plugin::Installed(_) => {}
    }
    if let Some(path) = &choices.command {
        content.lines.push(Line::from(vec![
            Span::styled("  ✓ ", GOOD),
            Span::raw("nudge command installed  "),
            // Keep the end of a long path: that's where it says what it is.
            Span::styled(fit(path, room, true), DIM),
        ]));
    }
}

/// The content for the step on screen.
fn content(wizard: &Wizard, room: usize) -> Content {
    let mut content = Content::default();
    let name = &wizard.entry(Field::Name).value;
    match wizard.step() {
        Step::Connect => {
            content.text(&[
                "Where should nudge send agent events? Copy the URL and the",
                "Authorization header from your webhook's settings: an AI routine,",
                "a workflow automation or your own service.",
            ]);
            content.fields(wizard, [Field::Url, Field::Authorization], room);
        }
        Step::Name => {
            content.text(&[
                "What should nudge call this webhook? You'll use the name with",
                "--webhook and in nudge list. With one webhook, main is fine.",
            ]);
            content.fields(wizard, [Field::Name, Field::Description], room);
        }
        Step::Options => options(wizard, &mut content, room),
        Step::Save => match wizard.save() {
            Some(SaveState::Testing { frame }) => {
                let spinner = SPINNER.get(frame % SPINNER.len()).copied().unwrap_or_default();
                content.lines.push(Line::from(vec![
                    Span::styled(format!("{spinner} "), HEADER),
                    Span::raw(format!("Sending a test ping to \"{name}\"…")),
                ]));
            }
            Some(SaveState::Failed { blame, choice }) => {
                content.text(&["The test ping failed. What now?"]);
                let fix = match blame {
                    Some(Blame::Authorization) => "Back to step 1 (fix the header)",
                    Some(Blame::Url) => "Back to step 1 (fix the URL)",
                    None => "Back to step 1",
                };
                for option in FailChoice::ALL {
                    let label = match option {
                        FailChoice::Retry => "Try again",
                        FailChoice::Back => fix,
                        FailChoice::SaveAnyway => "Save anyway",
                    };
                    let line = Line::from(vec![
                        Span::raw("  "),
                        radio(option == *choice),
                        Span::raw(format!(" {label}")),
                    ]);
                    content.stop(line, option == *choice);
                }
            }
            Some(SaveState::Saving { .. }) => content.lines.push(Line::raw(format!("Saving \"{name}\"…"))),
            Some(SaveState::NotSaved { .. }) => content.text(&[
                "Your answers are kept. Fix what's in the way, then press Enter to",
                "try saving again.",
            ]),
            None => {}
        },
    }
    content
}

fn draw(terminal: &mut DefaultTerminal, wizard: &Wizard) -> Result<()> {
    let width = terminal.size()?.width.min(WIDTH);
    // Leave room for the label, the ✓/✗ mark and the caret.
    let room = usize::from(ui::content_width(width)).saturating_sub(LABEL + 4).max(8);
    let Content { lines, selected, caret } = content(wizard, room);
    let steps: Vec<(&str, StepMark)> = Step::ALL
        .iter()
        .map(|step| {
            let mark = if *step == wizard.step() {
                StepMark::Current
            } else if wizard.done(*step) {
                StepMark::Done
            } else {
                StepMark::Pending
            };
            (step.title(), mark)
        })
        .collect();
    let help: &[(&str, &str)] = match (wizard.step(), wizard.save()) {
        (Step::Connect, _) => &HELP_CONNECT,
        (Step::Name, _) => &HELP_NAME,
        (Step::Options, _) if wizard.editing().is_some() => &HELP_EDITING,
        (Step::Options, _) => &HELP_OPTIONS,
        (Step::Save, Some(SaveState::Failed { .. })) => &HELP_FAILED,
        (Step::Save, Some(SaveState::NotSaved { .. })) => &HELP_NOT_SAVED,
        (Step::Save, Some(SaveState::Testing { .. } | SaveState::Saving { .. }) | None) => &HELP_TESTING,
    };
    let title = format!("Setup · Step {} of {}", wizard.step().number(), Step::ALL.len());
    let status = wizard.status();
    ui::draw(
        terminal,
        View {
            title: &title,
            unsaved: false,
            steps: Some(&steps),
            lines,
            selected,
            caret: caret.and_then(|column| u16::try_from(column).ok()),
            note: ui::note(status.as_deref(), &wizard.hint()),
            help,
            width: Some(width),
        },
    )
}

/// What a worker thread finished.
enum Done {
    /// The clipboard read when the wizard opened.
    Prefill(Option<String>),
    /// The clipboard read Enter asked for on this field.
    Clipboard(Field, Option<String>),
    Pinged(Result<Result<u16, WebhookError>>),
}

/// Read the clipboard on a worker thread (each tool may take seconds), sending the text back as `done`.
fn read_clipboard_later(
    env: &Env,
    sender: &mpsc::Sender<Done>,
    done: impl FnOnce(Option<String>) -> Done + Send + 'static,
) {
    let (env, sender, dir) = (env.clone(), sender.clone(), runtime_dir(env));
    // Not joined: it writes nothing, and a result that arrives after the user left is never read.
    thread::spawn(move || {
        sender
            .send(done(read_clipboard(Desktop::CURRENT, &System, &env, &dir)))
            .ok();
    });
}

/// Run the wizard until `save` succeeds (returning the test ping's HTTP status, if one passed, and the saved config)
/// or the user leaves ([`Error::Cancelled`]). Saving runs with the screen up: when it fails, the wizard stays on step 4
/// with every answer and offers to try again.
pub(super) fn run(
    terminal: &mut DefaultTerminal,
    wizard: &mut Wizard,
    env: &Env,
    mut save: impl FnMut(&Wizard) -> Result<Config>,
) -> Result<(Option<u16>, Config)> {
    let (sender, receiver) = mpsc::channel();
    if wizard.entry(Field::Url).value.is_empty() || wizard.entry(Field::Authorization).value.is_empty() {
        read_clipboard_later(env, &sender, Done::Prefill);
    }
    let mut redraw = true;
    loop {
        // Draw only after a change: a screen that keeps redrawing never looks settled.
        if std::mem::take(&mut redraw) {
            draw(terminal, wizard)?;
        }
        let mut outcome = Outcome::Continue;
        if let Ok(done) = receiver.try_recv() {
            match done {
                Done::Prefill(text) => wizard.prefill(text.as_deref()),
                Done::Clipboard(field, text) => wizard.fill_from_clipboard(field, text),
                // A ping that couldn't start (no working directory) is a failed test, not a reason to leave.
                Done::Pinged(result) => {
                    let result = result.unwrap_or_else(|error| {
                        Err(WebhookError {
                            message: error.to_string(),
                            http_status: None,
                        })
                    });
                    outcome = wizard.tested(result);
                }
            }
            redraw = true;
        } else if event::poll(TICK)? {
            redraw = true;
            if let Some(key) = key(event::read()?) {
                outcome = wizard.handle(key);
            }
        } else if matches!(wizard.save(), Some(SaveState::Testing { .. })) {
            // Only the spinner moves on its own.
            wizard.tick();
            redraw = true;
        }
        match outcome {
            Outcome::Continue => {}
            Outcome::Quit => return Err(Error::Cancelled),
            Outcome::ReadClipboard(field) => {
                read_clipboard_later(env, &sender, move |text| Done::Clipboard(field, text));
            }
            Outcome::Test(webhook) => {
                let (env, sender) = (env.clone(), sender.clone());
                // Not joined: on Ctrl-C the loop leaves and the ping ends with the process (nothing is saved yet).
                thread::spawn(move || {
                    sender.send(Done::Pinged(ping(&webhook, &env))).ok();
                });
            }
            Outcome::Save { tested } => {
                draw(terminal, wizard)?;
                match save(wizard) {
                    Ok(config) => return Ok((tested, config)),
                    Err(error) => wizard.not_saved(&error),
                }
            }
        }
    }
}
