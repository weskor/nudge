use std::fs;

use serde_json::Value;

use super::{Docked, bound_all, dock};
use crate::env::tests::FakeRun;
use crate::panel::clicks::Restored;
use crate::panel::record;
use crate::panel::tests::{Herdr, MAIN, app, json, layout_files, open, shows_panel, text};
use crate::panel::undock::undock;
use crate::panel::window::Window;

const BLOCK: &str =
    r#"{"id":"nudge-panel","type":"block","content":{"mod":"nudge"},"size":{"px":280},"min":280,"max":360}"#;

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
            from: "main".into(),
            restored: Restored::default()
        }
    );
    assert_eq!(text(home.path(), "main"), MAIN, "Herdr's own layout is never written");
    assert!(shows_panel(home.path(), "main-nudge"));
    assert_eq!(herdr.calls(), [r#"do layout.clone {"as":"main-nudge","name":"main"}"#]);
    let made = record::load(&record_path);
    assert_eq!(made.made["main-nudge"].from, "main");

    let before = text(home.path(), "main-nudge");
    let again = dock(home.path(), &record_path, open(&herdr, "main-nudge")).unwrap();
    assert_eq!(
        again,
        Docked::Already {
            name: "main-nudge".into(),
            restored: Restored::default()
        }
    );
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
            from: "main".into(),
            restored: Restored::default()
        }
    );
    assert_eq!(herdr.calls(), ["do layout.switch --name main-nudge"]);
    assert_eq!(text(home.path(), "main-nudge"), copy, "it already had the panel");
    assert!(!home.path().join("panel.json").exists(), "nudge didn't make that copy");
}

#[test]
fn a_width_or_hidden_flag_the_window_saved_for_the_docked_panel_is_put_back_to_the_layout_s() {
    let copy = r#"{"id":"root","type":"row","cloned_from":{"name":"main"},"children":[{"id":"nudge-panel","type":"block","content":{"mod":"nudge"},"size":{"px":320}}]}"#;
    let home = app(&[("main-nudge", copy)]);
    fs::create_dir_all(home.path().join("layouts/.state")).unwrap();
    let state = home.path().join("layouts/.state/main-nudge.json");
    let herdr = Herdr::new(home.path());

    // Saved at 559 px (how the window came to save that is unexplained), and hidden.
    let saved = r#"{"nodes":{"nudge-panel":{"size":{"px":559.0},"hidden":true},"panel":{"hidden":false}}}"#;
    fs::write(&state, saved).unwrap();
    let docked = dock(home.path(), &home.path().join("panel.json"), open(&herdr, "main-nudge")).unwrap();
    let restored = Restored {
        width: Some(559.0),
        hidden: true,
        refused: Vec::new(),
    };
    assert_eq!(
        docked,
        Docked::Already {
            name: "main-nudge".into(),
            restored
        }
    );
    assert_eq!(
        herdr.calls(),
        [
            "do layout.resize --id nudge-panel --px 320",
            "do layout.show --id nudge-panel"
        ],
        "the window saves the shown layout's state from memory, so it's asked, not the file edited"
    );
    assert_eq!(fs::read_to_string(&state).unwrap(), saved);
    assert_eq!(text(home.path(), "main-nudge"), copy);

    // Saved at the layout's own width, or for another node only: nothing to put back.
    for saved in [
        r#"{"nodes":{"nudge-panel":{"size":{"px":320.0}}}}"#,
        r#"{"nodes":{"panel":{"size":{"px":559.0}}}}"#,
    ] {
        fs::write(&state, saved).unwrap();
        let herdr = Herdr::new(home.path());
        let docked = dock(home.path(), &home.path().join("panel.json"), open(&herdr, "main-nudge")).unwrap();
        assert!(matches!(docked, Docked::Already { restored, .. } if restored == Restored::default()));
        assert!(herdr.calls().is_empty());
    }
}

#[test]
fn putting_the_panel_back_is_best_effort_and_only_for_the_shown_layout() {
    let docked = r#"{"id":"root","type":"row","cloned_from":{"name":"main"},"children":[{"id":"nudge-panel","type":"block","content":{"mod":"nudge"},"size":{"px":320}}]}"#;
    let plain = r#"{"id":"root","type":"row","cloned_from":{"name":"main"},"children":[]}"#;
    let home = app(&[("mine", docked), ("other", docked), ("plain", plain)]);
    fs::create_dir_all(home.path().join("layouts/.state")).unwrap();
    let saved = r#"{"nodes":{"nudge-panel":{"size":{"px":559.0}}}}"#;
    for name in ["mine", "other"] {
        fs::write(home.path().join(format!("layouts/.state/{name}.json")), saved).unwrap();
    }
    let record_path = home.path().join("panel.json");

    // The window refuses (an older Herdr, or it closed in between): install still docks, and says so.
    let refusing = FakeRun::default();
    let docked_in = dock(home.path(), &record_path, open(&refusing, "mine")).unwrap();
    let Docked::Already { restored, .. } = docked_in else {
        panic!("{docked_in:?}")
    };
    assert_eq!(restored.width, None);
    let refusal = "Herdr couldn't set the panel's width in \"mine\" back to 320 px (it had saved 559 px)";
    assert_eq!(restored.refused, [refusal]);

    // Another layout is shown, or the window is closed: no layout's saved state is put back, none is written.
    let herdr = Herdr::new(home.path());
    let other_shown = dock(home.path(), &record_path, open(&herdr, "plain")).unwrap();
    assert!(matches!(other_shown, Docked::Added { .. }), "{other_shown:?}");
    assert_eq!(dock(home.path(), &record_path, Window::Closed).unwrap(), Docked::Closed);
    assert!(herdr.calls().is_empty());
    for name in ["mine", "other"] {
        assert_eq!(text(home.path(), name), docked);
        let state = fs::read_to_string(home.path().join(format!("layouts/.state/{name}.json"))).unwrap();
        assert_eq!(state, saved);
    }
}

#[test]
fn a_panel_docked_by_an_older_nudge_gets_its_limits_and_is_still_removed_with_its_copy() {
    let home = app(&[("main", MAIN)]);
    let record_path = home.path().join("panel.json");
    let herdr = Herdr::new(home.path());
    dock(home.path(), &record_path, open(&herdr, "main")).unwrap();
    // The copy as the previous release (4d0ac1a) left it: the block without `min` and `max`.
    let path = home.path().join("layouts/main-nudge.json");
    let current = fs::read_to_string(&path).unwrap();
    let older = current.replace(",\n      \"min\": 280,\n      \"max\": 360", "");
    assert_ne!(older, current);
    assert!(older.contains("\"id\": \"nudge-panel\"") && !older.contains("\"min\""));
    fs::write(&path, &older).unwrap();

    assert_eq!(bound_all(home.path()).unwrap(), ["main-nudge"]);
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        current,
        "the same block a new install writes"
    );
    assert!(bound_all(home.path()).unwrap().is_empty(), "again: nothing to do");
    assert_eq!(text(home.path(), "main"), MAIN, "Herdr's own layout is never written");

    let undocked = undock(home.path(), &record_path, open(&herdr, "main-nudge")).unwrap();
    assert_eq!(
        undocked.deleted,
        ["main-nudge"],
        "still recognized as the copy install made"
    );
    assert_eq!(layout_files(home.path()), ["main.json"]);
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
fn a_layout_install_cant_dock_in_is_left_as_it_was_and_the_error_says_how_to_dock_by_hand() {
    let refusals = [
        r#"{"id":"root","type":"column","cloned_from":{"name":"main"},"children":[]}"#,
        r#"{"id":"root","type":"row","cloned_from":{"name":"main"}}"#,
        "// mine\n{\"id\":\"root\",\"type\":\"row\",\"cloned_from\":{\"name\":\"main\"},\"children\":[]}",
        "{\"id\":\"root\",\"type\":\"row\",",
    ];
    for layout in refusals {
        let home = app(&[("mine", layout)]);
        let record_path = home.path().join("panel.json");
        let run = FakeRun::default();
        let error = dock(home.path(), &record_path, open(&run, "mine"))
            .unwrap_err()
            .to_string();
        let path = home.path().join("layouts/mine.json");
        for hint in [
            format!("add this block as the last child of the top row in {}", path.display()),
            format!("\n  {BLOCK}\n"),
            "Or open it over the window: herdr-app do mod.open --name nudge".to_owned(),
        ] {
            assert!(error.contains(&hint), "{hint:?} in {error}");
        }
        assert_eq!(text(home.path(), "mine"), layout, "nothing written");
        assert_eq!(layout_files(home.path()), ["mine.json"]);
        assert!(run.calls().is_empty() && !record_path.exists());
    }
}
