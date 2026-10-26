use crate::setup::wizard::tests::{HEADER, URL, connected, start, type_text};
use crate::setup::wizard::{Choice, Key, Step, Wizard};

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
fn a_name_with_spaces_around_it_cant_take_over_another_webhook() {
    let mut wizard = connected(&["research"]);
    wizard.handle(Key::Enter);
    wizard.handle(Key::Clear);
    type_text(&mut wizard, "research ");
    wizard.handle(Key::Enter);
    assert_eq!(wizard.step(), Step::Name);
    assert!(wizard.status().unwrap().contains("\"research\" is already a webhook"));
}
