use crate::error::{Blame, Error};
use crate::setup::wizard::tests::{connected, failure, type_text};
use crate::setup::wizard::{FailChoice, Field, Key, Outcome, SaveState, Step};

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
