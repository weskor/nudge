use crate::setup::wizard::tests::{HEADER, URL, connected, start, type_text, value};
use crate::setup::wizard::{Field, Key, Outcome, Source, Step, Wizard};

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
