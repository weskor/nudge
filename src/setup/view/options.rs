//! Step 3 on screen: what nudge will do after saving (or checkboxes to change it), and what isn't a choice.
use ratatui::text::{Line, Span};

use super::content::Content;
use super::field::fit;
use crate::herdr::Plugin;
use crate::setup::choices::Choices;
use crate::setup::wizard::{Choice, Field, Wizard};
use crate::terminal::ui::{DIM, GOOD, HEADER, WARN, check};

/// Step 3's lines: what nudge will do (or checkboxes to change it), and what isn't a choice.
pub(super) fn options(wizard: &Wizard, content: &mut Content, room: usize) {
    let choices = wizard.choices();
    let plugin_state = match &choices.plugin {
        Plugin::Installed(info) if info.enabled => Span::styled("  herdr plugin enabled", DIM),
        Plugin::Installed(_) => Span::styled("  herdr plugin disabled", WARN),
        Plugin::HerdrUnavailable | Plugin::NotInstalled => Span::raw(""),
    };
    if let Some(cursor) = wizard.editing() {
        checkboxes(wizard, content, cursor, &plugin_state);
    } else {
        summary(wizard, content, &plugin_state);
    }
    fixed(choices, content, room);
}

/// The offered choices as checkboxes, the one at `cursor` selected.
fn checkboxes(wizard: &Wizard, content: &mut Content, cursor: usize, plugin_state: &Span<'static>) {
    let name = &wizard.entry(Field::Name).value;
    let choices = wizard.choices();
    content.text(&["Change what nudge will do after saving:"]);
    for (index, choice) in wizard.offered().iter().enumerate() {
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
}

/// What nudge will do for each offered choice, as chosen.
fn summary(wizard: &Wizard, content: &mut Content, plugin_state: &Span<'static>) {
    let name = &wizard.entry(Field::Name).value;
    let choices = wizard.choices();
    content.text(&["After saving, nudge will:"]);
    for choice in &wizard.offered() {
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

/// What isn't a choice: herdr's plugin missing or unreachable, and the `nudge` command already installed.
fn fixed(choices: &Choices, content: &mut Content, room: usize) {
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
