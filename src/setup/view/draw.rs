//! Drawing the wizard: the step list, the step's content, the note and the key help for where the wizard is.
use ratatui::DefaultTerminal;

use super::content::{Content, content};
use super::field::LABEL;
use crate::error::Result;
use crate::setup::wizard::{SaveState, Step, Wizard};
use crate::terminal::ui::{self, StepMark, View};

/// The wizard's width (or the screen's, if narrower): room for the step list and a typical webhook URL.
const WIDTH: u16 = 96;

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

/// The keys that do something where the wizard is.
fn help(wizard: &Wizard) -> &'static [(&'static str, &'static str)] {
    match (wizard.step(), wizard.save()) {
        (Step::Connect, _) => &HELP_CONNECT,
        (Step::Name, _) => &HELP_NAME,
        (Step::Options, _) if wizard.editing().is_some() => &HELP_EDITING,
        (Step::Options, _) => &HELP_OPTIONS,
        (Step::Save, Some(SaveState::Failed { .. })) => &HELP_FAILED,
        (Step::Save, Some(SaveState::NotSaved { .. })) => &HELP_NOT_SAVED,
        (Step::Save, Some(SaveState::Testing { .. } | SaveState::Saving { .. }) | None) => &HELP_TESTING,
    }
}

pub(super) fn draw(terminal: &mut DefaultTerminal, wizard: &Wizard) -> Result<()> {
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
            help: help(wizard),
            width: Some(width),
        },
    )
}
