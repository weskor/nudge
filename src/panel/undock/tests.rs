use std::fs;

use serde_json::Value;

use super::{Kept, Undocked, undock};
use crate::env::tests::FakeRun;
use crate::panel::dock::dock;
use crate::panel::layout;
use crate::panel::record;
use crate::panel::tests::{Herdr, MAIN, app, json, layout_files, open, shows_panel, text};
use crate::panel::window::Window;

#[test]
fn install_then_remove_on_herdr_s_layout_shows_it_again_and_deletes_the_copy() {
    let home = app(&[("main", MAIN)]);
    let record_path = home.path().join("panel.json");
    let herdr = Herdr::new(home.path());
    dock(home.path(), &record_path, open(&herdr, "main")).unwrap();

    let undocked = undock(home.path(), &record_path, open(&herdr, "main-nudge")).unwrap();
    assert_eq!(undocked.switched, Some(("main-nudge".into(), "main".into())));
    assert!(
        undocked.taken_out.is_empty(),
        "a deleted copy isn't reported as edited too"
    );
    assert_eq!(undocked.deleted, ["main-nudge"]);
    assert_eq!(herdr.calls().last().unwrap(), "do layout.switch --name main");
    assert_eq!(layout_files(home.path()), ["main.json"]);
    assert_eq!(text(home.path(), "main"), MAIN);
    assert!(!record_path.exists(), "nothing left to remember");

    let again = undock(home.path(), &record_path, open(&herdr, "main")).unwrap();
    assert_eq!(again, Undocked::default(), "removing again changes nothing");
}

#[test]
fn install_then_remove_on_a_layout_of_your_own_gives_back_the_same_layout() {
    let own = r#"{"id":"root","type":"row","cloned_from":{"name":"main"},"theme":"dusk","children":[
        {"id":"side","type":"stack","active":"agents","children":[{"id":"agents","type":"block","content":{"mod":"agents"}}]},
        {"id":"work","type":"block","content":{"tab":"active"},"floating":[]}]}"#;
    let home = app(&[("desk", own)]);
    let record_path = home.path().join("panel.json");
    let run = FakeRun::default();
    dock(home.path(), &record_path, open(&run, "desk")).unwrap();
    assert!(shows_panel(home.path(), "desk"));

    let undocked = undock(home.path(), &record_path, open(&run, "desk")).unwrap();
    assert_eq!(
        undocked,
        Undocked {
            taken_out: vec!["desk".into()],
            ..Undocked::default()
        }
    );
    assert_eq!(
        json(home.path(), "desk"),
        serde_json::from_str::<Value>(own).unwrap(),
        "the same layout, field for field"
    );
    assert!(run.calls().is_empty());

    // A file already in nudge's format comes back byte for byte.
    let written = layout::to_text(&layout::parse(own, "desk").unwrap()).unwrap();
    fs::write(home.path().join("layouts/desk.json"), &written).unwrap();
    dock(home.path(), &record_path, open(&run, "desk")).unwrap();
    undock(home.path(), &record_path, open(&run, "desk")).unwrap();
    assert_eq!(text(home.path(), "desk"), written);
}

#[test]
fn a_copy_changed_since_install_is_kept_without_its_panel_and_forgotten() {
    let home = app(&[("main", MAIN)]);
    let record_path = home.path().join("panel.json");
    let herdr = Herdr::new(home.path());
    dock(home.path(), &record_path, open(&herdr, "main")).unwrap();
    let mut copy = json(home.path(), "main-nudge");
    copy["theme"] = "dusk".into();
    fs::write(home.path().join("layouts/main-nudge.json"), copy.to_string()).unwrap();

    let undocked = undock(home.path(), &record_path, open(&herdr, "main-nudge")).unwrap();
    assert_eq!(undocked.kept, [("main-nudge".to_owned(), Kept::Changed)]);
    assert!(undocked.deleted.is_empty());
    assert!(!shows_panel(home.path(), "main-nudge"));
    assert_eq!(json(home.path(), "main-nudge")["theme"], "dusk");
    assert!(
        record::load(&record_path).made.is_empty(),
        "it's yours now: a later remove won't delete it"
    );
}

#[test]
fn with_herdr_closed_the_copy_it_opens_with_is_kept_until_the_window_can_switch_away() {
    let home = app(&[("main", MAIN)]);
    let record_path = home.path().join("panel.json");
    let herdr = Herdr::new(home.path());
    dock(home.path(), &record_path, open(&herdr, "main")).unwrap();
    fs::write(home.path().join("shared.json"), r#"{"layout":"main-nudge"}"#).unwrap();

    let closed = undock(home.path(), &record_path, Window::Closed).unwrap();
    assert_eq!(closed.kept, [("main-nudge".to_owned(), Kept::Shown)]);
    assert_eq!(closed.taken_out, ["main-nudge"]);
    assert!(!shows_panel(home.path(), "main-nudge"));

    let open_again = undock(home.path(), &record_path, open(&herdr, "main-nudge")).unwrap();
    assert_eq!(open_again.deleted, ["main-nudge"]);
    assert_eq!(layout_files(home.path()), ["main.json"]);
}

#[test]
fn a_nudge_copy_nudge_didn_t_make_is_switched_away_from_and_kept() {
    let copy = r#"{"id":"root","type":"row","cloned_from":{"name":"main"},"children":[{"id":"nudge-panel","type":"block","content":{"mod":"nudge"}}]}"#;
    let home = app(&[("main", MAIN), ("main-nudge", copy)]);
    let herdr = Herdr::new(home.path());
    let undocked = undock(home.path(), &home.path().join("panel.json"), open(&herdr, "main-nudge")).unwrap();
    assert_eq!(undocked.switched, Some(("main-nudge".into(), "main".into())));
    assert_eq!(undocked.kept, [("main-nudge".to_owned(), Kept::NotMade)]);
    assert!(undocked.deleted.is_empty());
    assert_eq!(layout_files(home.path()), ["main-nudge.json", "main.json"]);
    assert!(!shows_panel(home.path(), "main-nudge"));
}

#[test]
fn herdr_s_layouts_and_panels_nudge_didn_t_add_are_never_touched() {
    let shipped =
        r#"{"id":"root","type":"row","children":[{"id":"nudge-panel","type":"block","content":{"mod":"nudge"}}]}"#;
    let mine = r#"{"id":"root","type":"row","cloned_from":{"name":"main"},"children":[{"id":"left","type":"block","content":{"mod":"nudge"}}]}"#;
    let home = app(&[("main", shipped), ("mine", mine)]);
    let undocked = undock(home.path(), &home.path().join("panel.json"), Window::NoApp).unwrap();
    assert_eq!(undocked, Undocked::default());
    assert_eq!(text(home.path(), "main"), shipped);
    assert_eq!(text(home.path(), "mine"), mine);
}
