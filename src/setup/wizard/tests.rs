use super::{Choice, FailChoice, Field, Key, Outcome, SaveState, Source, Start, Step, Wizard};
use crate::error::{Blame, Error, WebhookError};
use crate::herdr::{Plugin, PluginInfo};
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

fn value(wizard: &Wizard, field: Field) -> &str {
    &wizard.entry(field).value
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
fn the_clipboard_prefills_only_what_checks_out_and_a_combined_paste_fills_both() {
    let mut wizard = Wizard::new(start(&[], None));
    wizard.prefill(Some("my password is hunter2-0123456789"));
    assert!(value(&wizard, Field::Url).is_empty() && value(&wizard, Field::Authorization).is_empty());
    wizard.prefill(Some(URL));
    assert_eq!(
        (
            value(&wizard, Field::Url),
            wizard.entry(Field::Url).source,
            wizard.focus()
        ),
        (URL, Some(Source::Clipboard), Some(Field::Authorization))
    );
    let mut wizard = Wizard::new(start(&[], None));
    wizard.handle(Key::Paste(format!("curl -H 'Authorization: {HEADER}' {URL}")));
    assert_eq!(
        (value(&wizard, Field::Url), value(&wizard, Field::Authorization)),
        (URL, HEADER)
    );
    assert_eq!(wizard.entry(Field::Authorization).source, Some(Source::Pasted));
    let mut wizard = Wizard::new(start(&[], None));
    wizard.fill_from_clipboard(Field::Url, Some("Bearer not-a-url-0123456789".into()));
    assert!(value(&wizard, Field::Url).is_empty());
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
    assert_eq!(wizard.step(), Step::Name);
    assert!(wizard.status().unwrap().contains("\"research\" is already a webhook"));
    wizard.handle(Key::Clear);
    type_text(&mut wizard, "my laptop");
    wizard.handle(Key::Enter);
    assert_eq!(wizard.step(), Step::Name);
    wizard.handle(Key::Clear);
    type_text(&mut wizard, "laptop");
    wizard.handle(Key::Enter);
    assert_eq!(wizard.step(), Step::Options);
    assert_eq!(wizard.offered(), [Choice::AutoNotify, Choice::Command, Choice::Default]);

    let mut update = Wizard::new(start(&["main", "research"], Some("research")));
    type_text(&mut update, URL);
    update.handle(Key::Tab);
    type_text(&mut update, HEADER);
    update.handle(Key::Enter);
    update.handle(Key::Enter);
    assert_eq!(update.step(), Step::Options, "the webhook being updated keeps its name");
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
        wizard.save(),
        Some(SaveState::Failed {
            blame: Some(Blame::Authorization),
            choice: FailChoice::Back
        })
    ));
    wizard.handle(Key::Enter);
    assert_eq!(
        (wizard.step(), wizard.focus()),
        (Step::Connect, Some(Field::Authorization))
    );
    assert!(wizard.status().unwrap().contains("rejected the Authorization header"));
    assert_eq!(
        wizard.handle(Key::Enter),
        Outcome::Continue,
        "the rejected header isn't sent again"
    );
    assert_eq!(wizard.step(), Step::Connect);
    type_text(&mut wizard, "Bearer other-0123456789abcdef");
    wizard.handle(Key::Enter);
    wizard.handle(Key::Enter);
    assert!(matches!(wizard.handle(Key::Enter), Outcome::Test(_)));
    wizard.tested(Err(failure(500)));
    assert!(matches!(
        wizard.save(),
        Some(SaveState::Failed {
            choice: FailChoice::Retry,
            ..
        })
    ));
    assert!(matches!(wizard.handle(Key::Enter), Outcome::Test(_)), "retry");
    wizard.tested(Err(failure(503)));
    wizard.handle(Key::Down);
    wizard.handle(Key::Down);
    assert_eq!(wizard.handle(Key::Enter), Outcome::Save { tested: None });
    assert_eq!(wizard.save(), Some(&SaveState::Saving { tested: None }));
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
fn a_pasted_header_never_lands_in_a_visible_field() {
    let mut wizard = Wizard::new(start(&[], None));
    wizard.handle(Key::Paste(format!("  {HEADER}\n")));
    assert!(
        value(&wizard, Field::Url).is_empty(),
        "the URL field never holds the header"
    );
    assert_eq!(
        (value(&wizard, Field::Authorization), wizard.focus()),
        (HEADER, Some(Field::Authorization))
    );
    let mut wizard = Wizard::new(start(&[], None));
    wizard.handle(Key::Paste("sk-live-0123456789abcdef".into()));
    assert!(
        value(&wizard, Field::Url).is_empty(),
        "text that isn't a URL goes to the hidden field"
    );

    let mut wizard = connected(&[]);
    wizard.handle(Key::Enter);
    assert_eq!(wizard.step(), Step::Name);
    wizard.handle(Key::Paste(format!("Authorization: {HEADER}")));
    assert_eq!(
        value(&wizard, Field::Name),
        "main",
        "a header pasted into the name is refused"
    );
    assert!(wizard.status().unwrap().contains("looks like an Authorization header"));
    wizard.handle(Key::Tab);
    let other = "Bearer other-0123456789abcdef";
    wizard.handle(Key::Paste(format!("https://other.example/h\n{other}")));
    assert!(
        value(&wizard, Field::Description).is_empty(),
        "a URL and header pasted on step 2 go to step 1"
    );
    assert_eq!(
        (value(&wizard, Field::Url), value(&wizard, Field::Authorization)),
        ("https://other.example/h", other)
    );
}

#[test]
fn a_failed_save_keeps_every_answer_and_enter_tries_again() {
    let mut wizard = connected(&[]);
    for _ in 0..2 {
        wizard.handle(Key::Enter);
    }
    let Outcome::Test(tested) = wizard.handle(Key::Enter) else {
        panic!("expected a test ping");
    };
    assert_eq!(wizard.tested(Ok(200)), Outcome::Save { tested: Some(200) });
    wizard.not_saved(&Error::Other("config.toml: Permission denied (os error 13)".into()));
    assert_eq!(wizard.step(), Step::Save);
    assert_eq!(wizard.save(), Some(&SaveState::NotSaved { tested: Some(200) }));
    assert!(
        wizard
            .status()
            .unwrap()
            .starts_with("✗ Not saved: config.toml: Permission denied")
    );
    assert_eq!(
        wizard.handle(Key::Enter),
        Outcome::Save { tested: Some(200) },
        "the retry keeps the passed test ping"
    );
    wizard.not_saved(&Error::Other("still read-only".into()));
    wizard.handle(Key::BackTab);
    assert_eq!(wizard.step(), Step::Options);
    assert_eq!(wizard.webhook().unwrap(), tested, "every answer is still there");
}

#[test]
fn a_slow_clipboard_read_never_overwrites_what_was_typed() {
    let mut wizard = Wizard::new(start(&[], None));
    type_text(&mut wizard, "https://typed.example/h");
    wizard.prefill(Some(&format!("https://clipboard.example/h\n{HEADER}")));
    assert_eq!(value(&wizard, Field::Url), "https://typed.example/h");
    assert_eq!(
        value(&wizard, Field::Authorization),
        HEADER,
        "the empty header is filled"
    );
    assert_eq!(wizard.focus(), Some(Field::Url), "the focus stays where the user is");
    assert_eq!(
        wizard.handle(Key::Esc),
        Outcome::Continue,
        "what was typed still asks before leaving"
    );

    let mut wizard = connected(&[]);
    wizard.handle(Key::Clear);
    assert_eq!(wizard.handle(Key::Enter), Outcome::ReadClipboard(Field::Authorization));
    assert_eq!(wizard.status().as_deref(), Some("Reading the clipboard…"));
    type_text(&mut wizard, "Bearer typed-0123456789abcdef");
    wizard.fill_from_clipboard(Field::Authorization, Some(format!("https://late.example/h\n{HEADER}")));
    assert_eq!(
        (value(&wizard, Field::Url), value(&wizard, Field::Authorization)),
        (URL, "Bearer typed-0123456789abcdef"),
        "a read that finishes after typing overwrites nothing"
    );
    wizard.handle(Key::Enter);
    assert_eq!(wizard.step(), Step::Name);
    wizard.fill_from_clipboard(Field::Url, Some("https://late.example/h".into()));
    assert_eq!(
        value(&wizard, Field::Url),
        URL,
        "a read that finishes after leaving step 1 fills nothing"
    );
}

#[test]
fn a_name_with_spaces_around_it_cant_take_over_another_webhook() {
    let mut wizard = connected(&["research"]);
    wizard.handle(Key::Enter);
    wizard.handle(Key::Clear);
    type_text(&mut wizard, "research ");
    wizard.handle(Key::Enter);
    assert_eq!(wizard.step(), Step::Name);
    assert!(wizard.status().unwrap().contains("\"research\" is already a webhook"));
}

#[test]
fn a_pasted_command_line_stores_only_the_header_in_it() {
    for pasted in [
        format!("-H \"Authorization: {HEADER}\""),
        format!("'Authorization: {HEADER}'"),
    ] {
        let mut wizard = Wizard::new(start(&[], None));
        wizard.handle(Key::Tab);
        wizard.handle(Key::Paste(pasted.clone()));
        assert_eq!(value(&wizard, Field::Authorization), HEADER, "{pasted}");
        let mut wizard = Wizard::new(start(&[], None));
        wizard.handle(Key::Paste(pasted.clone()));
        assert_eq!(
            value(&wizard, Field::Authorization),
            HEADER,
            "into the URL field: {pasted}"
        );
        assert!(value(&wizard, Field::Url).is_empty());
        let mut wizard = Wizard::new(start(&[], None));
        wizard.handle(Key::Tab);
        wizard.fill_from_clipboard(Field::Authorization, Some(pasted.clone()));
        assert_eq!(
            value(&wizard, Field::Authorization),
            HEADER,
            "from the clipboard: {pasted}"
        );
    }
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
