use serde_json::Value;

use super::{Docked, dock};
use crate::env::tests::FakeRun;
use crate::panel::record;
use crate::panel::tests::{Herdr, MAIN, app, json, layout_files, open, shows_panel, text};
use crate::panel::window::Window;

const BLOCK: &str = r#"{"id":"nudge-panel","type":"block","content":{"mod":"nudge"},"size":{"px":320}}"#;

#[test]
fn herdr_s_own_layout_is_copied_with_layout_clone_recorded_and_the_copy_gets_the_panel() {
    let home = app(&[("main", MAIN)]);
    let record_path = home.path().join("panel.json");
    let herdr = Herdr::new(home.path());
    let docked = dock(home.path(), &record_path, open(&herdr, "main")).unwrap();
    assert_eq!(
        docked,
        Docked::Copied {
            name: "main-nudge".into(),
            from: "main".into()
        }
    );
    assert_eq!(text(home.path(), "main"), MAIN, "Herdr's own layout is never written");
    assert!(shows_panel(home.path(), "main-nudge"));
    assert_eq!(herdr.calls(), [r#"do layout.clone {"as":"main-nudge","name":"main"}"#]);
    let made = record::load(&record_path);
    assert_eq!(made.made["main-nudge"].from, "main");

    let before = text(home.path(), "main-nudge");
    let again = dock(home.path(), &record_path, open(&herdr, "main-nudge")).unwrap();
    assert_eq!(again, Docked::Already("main-nudge".into()));
    assert_eq!(text(home.path(), "main-nudge"), before);
}

#[test]
fn back_on_herdr_s_layout_the_existing_copy_is_shown_again_not_copied_twice() {
    let copy = r#"{"id":"root","type":"row","cloned_from":{"name":"main"},"children":[{"id":"nudge-panel","type":"block","content":{"mod":"nudge"}}]}"#;
    let home = app(&[("main", MAIN), ("main-nudge", copy)]);
    let herdr = Herdr::new(home.path());
    let docked = dock(home.path(), &home.path().join("panel.json"), open(&herdr, "main")).unwrap();
    assert_eq!(
        docked,
        Docked::Copied {
            name: "main-nudge".into(),
            from: "main".into()
        }
    );
    assert_eq!(herdr.calls(), ["do layout.switch --name main-nudge"]);
    assert_eq!(text(home.path(), "main-nudge"), copy, "it already had the panel");
    assert!(!home.path().join("panel.json").exists(), "nudge didn't make that copy");
}

#[test]
fn the_shown_layout_of_your_own_gets_the_panel_in_place_and_install_says_what_it_added() {
    // A custom layout of your own (stacked sidebar on the left), shown in the window.
    let own = r#"{"id":"root","type":"row","cloned_from":{"name":"main"},"theme":"dusk","children":[
        {"id":"side","type":"stack","active":"agents","children":[{"id":"agents","type":"block","content":{"mod":"agents"}}]},
        {"id":"work","type":"block","content":{"tab":"active"},"size":{"weight":3}}]}"#;
    let home = app(&[("desk", own)]);
    let run = FakeRun::default();
    let docked = dock(home.path(), &home.path().join("panel.json"), open(&run, "desk")).unwrap();
    assert_eq!(
        docked,
        Docked::Added {
            name: "desk".into(),
            block: BLOCK.into()
        }
    );
    assert!(run.calls().is_empty(), "no Herdr command for a layout of your own");
    let mut expected: Value = serde_json::from_str(own).unwrap();
    expected["children"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::from_str(BLOCK).unwrap());
    assert_eq!(json(home.path(), "desk"), expected, "only the block was added");
}

#[test]
fn a_closed_window_or_no_herdr_app_docks_nothing_and_a_failed_copy_writes_nothing() {
    let home = app(&[("main", MAIN)]);
    let record_path = home.path().join("panel.json");
    assert_eq!(dock(home.path(), &record_path, Window::Closed).unwrap(), Docked::Closed);
    assert_eq!(dock(home.path(), &record_path, Window::NoApp).unwrap(), Docked::NoApp);
    let refused = FakeRun::default();
    let error = dock(home.path(), &record_path, open(&refused, "main"))
        .unwrap_err()
        .to_string();
    assert_eq!(error, "Herdr couldn't copy the layout \"main\"");
    assert_eq!(layout_files(home.path()), ["main.json"]);
    assert!(!record_path.exists());
}

#[test]
fn a_layout_of_your_own_whose_top_isnt_a_row_is_left_as_it_was() {
    let column = r#"{"id":"root","type":"column","cloned_from":{"name":"main"},"children":[]}"#;
    let home = app(&[("tall", column)]);
    let error = dock(
        home.path(),
        &home.path().join("panel.json"),
        open(&FakeRun::default(), "tall"),
    )
    .unwrap_err();
    assert!(error.to_string().contains("isn't a row"), "{error}");
    assert_eq!(text(home.path(), "tall"), column);
}
