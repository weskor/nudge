//! Step 4 on screen: the test ping's spinner, the choices after a failed one, saving, and a failed save.
use ratatui::text::{Line, Span};

use super::content::Content;
use crate::error::Blame;
use crate::setup::wizard::{FailChoice, Field, SaveState, Wizard};
use crate::terminal::ui::{HEADER, radio};

const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Step 4's lines for where it stands.
pub(super) fn save(wizard: &Wizard, content: &mut Content) {
    let name = &wizard.entry(Field::Name).value;
    match wizard.save() {
        Some(SaveState::Testing { frame }) => {
            let spinner = SPINNER.get(frame % SPINNER.len()).copied().unwrap_or_default();
            content.lines.push(Line::from(vec![
                Span::styled(format!("{spinner} "), HEADER),
                Span::raw(format!("Sending a test ping to \"{name}\"…")),
            ]));
        }
        Some(SaveState::Failed { blame, choice }) => failed(content, *blame, *choice),
        Some(SaveState::Saving { .. }) => content.lines.push(Line::raw(format!("Saving \"{name}\"…"))),
        Some(SaveState::NotSaved { .. }) => content.text(&[
            "Your answers are kept. Fix what's in the way, then press Enter to",
            "try saving again.",
        ]),
        None => {}
    }
}

/// What to do after a failed test ping, with `choice` selected.
fn failed(content: &mut Content, blame: Option<Blame>, choice: FailChoice) {
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
            radio(option == choice),
            Span::raw(format!(" {label}")),
        ]);
        content.stop(line, option == choice);
    }
}
