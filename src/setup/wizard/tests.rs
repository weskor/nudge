use super::{Choices, Field, Key, Outcome, Start, Step, Wizard};
use crate::error::WebhookError;
use crate::herdr::{Plugin, PluginInfo};

pub(super) const URL: &str = "https://hooks.example.com/h/7f3c2a";
pub(super) const HEADER: &str = "Bearer token-0123456789abcdef";

pub(super) fn start(saved: &[&str], loaded: Option<&str>) -> Start {
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

pub(super) fn type_text(wizard: &mut Wizard, text: &str) {
    for typed in text.chars() {
        assert_eq!(wizard.handle(Key::Char(typed)), Outcome::Continue);
    }
}

/// A wizard on step 1 with both values typed in.
pub(super) fn connected(saved: &[&str]) -> Wizard {
    let mut wizard = Wizard::new(start(saved, None));
    type_text(&mut wizard, URL);
    wizard.handle(Key::Tab);
    type_text(&mut wizard, HEADER);
    wizard
}

pub(super) fn value(wizard: &Wizard, field: Field) -> &str {
    &wizard.entry(field).value
}

pub(super) fn failure(status: u16) -> WebhookError {
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
    assert_eq!((wizard.step(), wizard.focus()), (Step::Connect, Some(Field::Url)));
    assert!(wizard.status().unwrap().contains("https URL"));
    wizard.handle(Key::Clear);
    type_text(&mut wizard, URL);
    wizard.handle(Key::Enter);
    assert_eq!(
        (wizard.step(), wizard.focus()),
        (Step::Connect, Some(Field::Authorization)),
        "a valid URL moves on to the empty header, not the next step"
    );
    assert_eq!(
        wizard.handle(Key::Enter),
        Outcome::ReadClipboard(Field::Authorization),
        "Enter on an empty field pastes"
    );
    type_text(&mut wizard, HEADER);
    wizard.handle(Key::Enter);
    assert_eq!(wizard.step(), Step::Name);
}

#[test]
fn going_back_keeps_every_answer_and_a_new_url_needs_a_new_test() {
    let mut wizard = connected(&[]);
    wizard.handle(Key::Enter);
    wizard.handle(Key::Tab);
    type_text(&mut wizard, "Main webhook");
    wizard.handle(Key::Enter);
    assert_eq!(wizard.step(), Step::Options);
    wizard.handle(Key::Char('e'));
    wizard.handle(Key::Char(' '));
    wizard.handle(Key::Enter);
    assert_eq!(wizard.choices().auto_notify, Some(false));
    let Outcome::Test(first) = wizard.handle(Key::Enter) else {
        panic!("expected a test ping");
    };
    wizard.tested(Err(failure(500)));
    for _ in 0..3 {
        wizard.handle(Key::BackTab);
    }
    assert_eq!(wizard.step(), Step::Connect);
    assert_eq!(wizard.save(), None, "step 4's state went with it");
    for _ in 0..2 {
        wizard.handle(Key::Enter);
    }
    let Outcome::Test(again) = wizard.handle(Key::Enter) else {
        panic!("expected a test ping");
    };
    assert_eq!(again, first, "nothing was lost on the way back");
    assert_eq!(wizard.choices().auto_notify, Some(false));
    assert!(wizard.done(Step::Connect) && wizard.done(Step::Options) && !wizard.done(Step::Save));
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
    assert_eq!(value(&wizard, Field::Name), "mainq", "q is text on step 2");
    wizard.handle(Key::Backspace);
    wizard.handle(Key::Enter);
    assert_eq!(wizard.handle(Key::Char('q')), Outcome::Continue);
    assert_eq!(wizard.handle(Key::Char('q')), Outcome::Quit);
    let mut wizard = connected(&[]);
    assert_eq!(wizard.handle(Key::Cancel), Outcome::Quit);
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
