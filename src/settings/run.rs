//! The editor's terminal loop: draw, wait for a key or a test ping's result, and act on it until saved or quit.
use std::sync::mpsc;
use std::time::Duration;

use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyEventKind};

use super::keys::{Outcome, key};
use super::ping::start_test;
use super::rows::describe;
use super::save::save;
use super::state::{Edits, Settings};
use crate::config::Config;
use crate::env::Env;
use crate::error::Result;
use crate::terminal::ui::{self, View};

/// How often the editor checks for a finished test ping while waiting for a key.
const TICK: Duration = Duration::from_millis(100);

const HELP: [(&str, &str); 6] = [
    ("↑↓", "move"),
    ("space", "pick"),
    ("←→", "change"),
    ("t", "test"),
    ("s", "save"),
    ("q", "quit"),
];

fn draw(terminal: &mut DefaultTerminal, settings: &Settings) -> Result<()> {
    let (lines, selected) = settings.lines();
    ui::draw(
        terminal,
        View {
            title: "nudge settings",
            unsaved: settings.edits != Edits::None,
            steps: None,
            lines,
            selected: Some(selected),
            caret: None,
            note: ui::note(settings.status.as_deref(), settings.current().map_or("", describe)),
            help: &HELP,
            width: None,
        },
    )
}

/// Run the UI until saved (`true`) or quit (`false`). A test ping runs on a worker thread so the screen keeps
/// answering (and Ctrl-C works) while the webhook takes its time.
pub(super) fn run_ui(
    terminal: &mut DefaultTerminal,
    settings: &mut Settings,
    config: &Config,
    env: &Env,
) -> Result<bool> {
    let (sender, receiver) = mpsc::channel();
    let mut redraw = true;
    loop {
        if std::mem::take(&mut redraw) {
            draw(terminal, settings)?;
        }
        if let Ok(result) = receiver.try_recv() {
            settings.status = Some(result);
            redraw = true;
            continue;
        }
        if !event::poll(TICK)? {
            continue;
        }
        redraw = true;
        let Event::Key(pressed) = event::read()? else { continue };
        if pressed.kind != KeyEventKind::Press {
            continue;
        }
        let Some(key) = key(pressed.code, pressed.modifiers) else {
            continue;
        };
        match settings.handle(key) {
            Outcome::Continue => {}
            Outcome::Quit => return Ok(false),
            Outcome::Save => match save(settings, env) {
                Ok(()) => return Ok(true),
                Err(error) => settings.status = Some(format!("✗ not saved: {}", error.to_string().replace('\n', " "))),
            },
            Outcome::Test(webhook) => {
                settings.status = Some(format!("Testing \"{webhook}\"…"));
                start_test(config, env, webhook, &sender);
            }
        }
    }
}
