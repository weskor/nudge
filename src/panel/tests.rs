use std::fs;
use std::path::{Path, PathBuf};

use parking_lot::Mutex;

use super::{Docked, app_home, dock, files, layout};
use crate::env::tests::FakeRun;
use crate::env::{Env, Run};

const MAIN: &str = r#"{"id":"root","type":"row","children":[{"id":"work","type":"block","content":{"tab":"active"}}]}"#;
const CLONE: &str = r#"herdr-app do layout.clone {"as":"main-nudge","name":"main"}"#;

/// Herdr's app home with its `layouts/`, holding `files` (name, text).
fn app(layouts: &[(&str, &str)]) -> tempfile::TempDir {
    let home = tempfile::tempdir().unwrap();
    fs::create_dir_all(home.path().join("layouts")).unwrap();
    for (name, text) in layouts {
        fs::write(home.path().join("layouts").join(format!("{name}.json")), text).unwrap();
    }
    home
}

fn layout_text(home: &Path, name: &str) -> String {
    fs::read_to_string(home.join("layouts").join(format!("{name}.json"))).unwrap()
}

fn shows_panel(home: &Path, name: &str) -> bool {
    layout::parse(&layout_text(home, name), name).unwrap().shows_panel()
}

/// A window showing `layout`, whose `layout.clone` writes the copy as Herdr does (adding `cloned_from`).
struct Window {
    home: PathBuf,
    layout: String,
    calls: Mutex<Vec<String>>,
}

impl Run for Window {
    fn run(&self, cmd: &[&str], _cwd: &Path) -> Option<String> {
        self.calls.lock().push(cmd.join(" "));
        match cmd {
            ["herdr-app", "status"] => Some(format!(r#"{{"layout":"{}","errors":[]}}"#, self.layout)),
            ["herdr-app", "do", "layout.clone", _] => {
                let copy = MAIN.replacen('{', r#"{"cloned_from":{"name":"main","version":"0.1.12"},"#, 1);
                fs::write(self.home.join("layouts/main-nudge.json"), copy).ok()?;
                Some(r#"{"ok":true}"#.into())
            }
            _ => None,
        }
    }
}

#[test]
fn herdr_s_own_layout_is_copied_with_layout_clone_and_the_copy_gets_the_panel() {
    let home = app(&[("main", MAIN)]);
    let window = Window {
        home: home.path().into(),
        layout: "main".into(),
        calls: Mutex::default(),
    };
    let docked = dock(home.path(), &window).unwrap();
    assert_eq!(
        docked,
        Docked::Copied {
            name: "main-nudge".into(),
            from: "main".into()
        }
    );
    assert_eq!(
        layout_text(home.path(), "main"),
        MAIN,
        "Herdr's own layout is never written"
    );
    assert!(shows_panel(home.path(), "main-nudge"));
    assert_eq!(window.calls.lock().as_slice(), ["herdr-app status", CLONE]);

    // Run again while the copy is shown: nothing more to do.
    let again = Window {
        home: home.path().into(),
        layout: "main-nudge".into(),
        calls: Mutex::default(),
    };
    let before = layout_text(home.path(), "main-nudge");
    assert_eq!(dock(home.path(), &again).unwrap(), Docked::Already("main-nudge".into()));
    assert_eq!(layout_text(home.path(), "main-nudge"), before);
}

#[test]
fn back_on_herdr_s_layout_the_existing_copy_is_shown_again_not_copied_twice() {
    let copy = r#"{"id":"root","type":"row","cloned_from":{"name":"main"},"children":[{"id":"nudge-panel","type":"block","content":{"mod":"nudge"}}]}"#;
    let home = app(&[("main", MAIN), ("main-nudge", copy)]);
    let switch = "herdr-app do layout.switch --name main-nudge";
    let run = FakeRun::new([("herdr-app status", r#"{"layout":"main"}"#), (switch, r#"{"ok":true}"#)]);
    let docked = dock(home.path(), &run).unwrap();
    assert_eq!(
        docked,
        Docked::Copied {
            name: "main-nudge".into(),
            from: "main".into()
        }
    );
    assert_eq!(run.calls(), ["herdr-app status", switch]);
    assert_eq!(layout_text(home.path(), "main-nudge"), copy, "it already had the panel");
}

#[test]
fn a_layout_of_your_own_gets_the_panel_in_place_without_any_herdr_command() {
    let own = MAIN.replacen('{', r#"{"cloned_from":{"name":"main"},"#, 1);
    let home = app(&[("mine", &own)]);
    let run = FakeRun::new([("herdr-app status", r#"{"layout":"mine"}"#)]);
    assert_eq!(dock(home.path(), &run).unwrap(), Docked::Added("mine".into()));
    assert!(shows_panel(home.path(), "mine"));
    assert_eq!(run.calls(), ["herdr-app status"]);
}

#[test]
fn a_closed_window_docks_nothing_and_a_failed_copy_writes_nothing() {
    let home = app(&[("main", MAIN)]);
    assert_eq!(dock(home.path(), &FakeRun::default()).unwrap(), Docked::Closed);
    let refused = FakeRun::new([("herdr-app status", r#"{"layout":"main"}"#)]);
    let error = dock(home.path(), &refused).unwrap_err().to_string();
    assert_eq!(error, "Herdr couldn't copy the layout \"main\"");
    let names: Vec<_> = fs::read_dir(home.path().join("layouts"))
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(names, ["main.json"]);
}

#[test]
fn the_mod_files_are_written_once_and_left_alone_when_they_match() {
    let home = tempfile::tempdir().unwrap();
    let dir = home.path().join("mods/nudge");
    assert!(files::install_mod(&dir).unwrap());
    let mut names: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    assert_eq!(
        names,
        ["main.js", "mod.json", "popup-view.js", "popup.js", "rows.js"],
        "no temp files left"
    );
    assert!(!files::install_mod(&dir).unwrap(), "nothing changed the second time");
    fs::write(dir.join("rows.js"), "stale").unwrap();
    assert!(files::install_mod(&dir).unwrap());
    assert_ne!(fs::read_to_string(dir.join("rows.js")).unwrap(), "stale");
}

#[test]
fn herdr_s_home_is_herdr_app_home_else_under_the_config_folder() {
    let set = Env::from([("HERDR_APP_HOME", "/srv/herdr"), ("HOME", "/home/me")]);
    assert_eq!(app_home(&set), PathBuf::from("/srv/herdr"));
    assert_eq!(
        app_home(&Env::from([("HOME", "/home/me")])),
        PathBuf::from("/home/me/.config/herdr-app")
    );
}
