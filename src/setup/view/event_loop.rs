//! The terminal loop: reads keys, runs the clipboard reads and the test ping on worker threads (so the screen never
//! freezes), redraws after each change, and saves while the screen is still up.
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use ratatui::DefaultTerminal;
use ratatui::crossterm::event;

use super::draw::draw;
use super::keys::key;
use crate::clipboard::{Desktop, read_clipboard, runtime_dir};
use crate::config::Config;
use crate::env::{Env, System};
use crate::error::{Error, Result, WebhookError};
use crate::setup::ping::ping;
use crate::setup::wizard::{Field, Outcome, SaveState, Wizard};

/// How often the screen redraws while waiting, so the spinner turns.
const TICK: Duration = Duration::from_millis(80);

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

/// Hand what a worker thread finished to the wizard.
fn received(wizard: &mut Wizard, done: Done) -> Outcome {
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
            return wizard.tested(result);
        }
    }
    Outcome::Continue
}

/// Wait up to a tick for a worker's result or a key and hand it to the wizard, else turn the spinner while the test
/// ping runs. `None`: nothing changed, so there's nothing to redraw.
fn next(wizard: &mut Wizard, receiver: &mpsc::Receiver<Done>) -> Result<Option<Outcome>> {
    if let Ok(done) = receiver.try_recv() {
        return Ok(Some(received(wizard, done)));
    }
    if event::poll(TICK)? {
        return Ok(Some(
            key(event::read()?).map_or(Outcome::Continue, |key| wizard.handle(key)),
        ));
    }
    if matches!(wizard.save(), Some(SaveState::Testing { .. })) {
        // Only the spinner moves on its own.
        wizard.tick();
        return Ok(Some(Outcome::Continue));
    }
    Ok(None)
}

/// Run the wizard until `save` succeeds (returning the test ping's HTTP status, if one passed, and the saved config)
/// or the user leaves ([`Error::Cancelled`]). Saving runs with the screen up: when it fails, the wizard stays on step 4
/// with every answer and offers to try again.
pub(in crate::setup) fn run(
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
        let Some(outcome) = next(wizard, &receiver)? else {
            continue;
        };
        redraw = true;
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
