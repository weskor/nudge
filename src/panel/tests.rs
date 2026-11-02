use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};

use parking_lot::Mutex;
use serde_json::Value;

use super::files::{self, Removed};
use super::window::{self, Window};
use super::{app_home, layout};
use crate::env::tests::FakeRun;
use crate::env::{Env, Run};

/// Herdr's `main` layout, cut down: a root row with the work area.
pub(super) const MAIN: &str = r#"{"id":"root","type":"row","background":"chrome","children":[{"id":"work","type":"block","content":{"tab":"active"}}]}"#;

/// Herdr's app home with `layouts/<name>.json` for each (name, text).
pub(super) fn app(layouts: &[(&str, &str)]) -> tempfile::TempDir {
    let home = tempfile::tempdir().unwrap();
    fs::create_dir_all(home.path().join("layouts")).unwrap();
    for (name, text) in layouts {
        fs::write(home.path().join(format!("layouts/{name}.json")), text).unwrap();
    }
    home
}

pub(super) fn text(home: &Path, name: &str) -> String {
    fs::read_to_string(home.join(format!("layouts/{name}.json"))).unwrap()
}

pub(super) fn json(home: &Path, name: &str) -> Value {
    serde_json::from_str(&text(home, name)).unwrap()
}

pub(super) fn shows_panel(home: &Path, name: &str) -> bool {
    layout::parse(&text(home, name), name).unwrap().shows_panel()
}

pub(super) fn layout_files(home: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(home.join("layouts"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    names
}

/// Herdr's window: `layout.clone` writes the copy as Herdr does (the source plus `cloned_from`), and
/// `layout.switch` works for any layout file that exists.
pub(super) struct Herdr {
    pub(super) home: PathBuf,
    pub(super) calls: Mutex<Vec<String>>,
}

impl Herdr {
    pub(super) fn new(home: &Path) -> Self {
        Self {
            home: home.into(),
            calls: Mutex::default(),
        }
    }

    pub(super) fn calls(&self) -> Vec<String> {
        self.calls.lock().clone()
    }
}

impl Run for Herdr {
    fn run(&self, cmd: &[&str], _cwd: &Path) -> Option<String> {
        self.calls.lock().push(cmd[1..].join(" "));
        match cmd {
            ["herdr-app", "do", "layout.clone", args] => {
                let args: Value = serde_json::from_str(args).ok()?;
                let (from, to) = (args["name"].as_str()?, args["as"].as_str()?);
                let mut copy: Value = serde_json::from_str(&text(&self.home, from)).ok()?;
                copy["cloned_from"] = serde_json::json!({ "name": from, "version": "0.1.12" });
                fs::write(
                    self.home.join(format!("layouts/{to}.json")),
                    serde_json::to_string_pretty(&copy).ok()?,
                )
                .ok()?;
                Some(r#"{"ok":true}"#.into())
            }
            ["herdr-app", "do", "layout.switch", "--name", name] => self
                .home
                .join(format!("layouts/{name}.json"))
                .exists()
                .then(|| r#"{"ok":true}"#.into()),
            _ => None,
        }
    }
}

pub(super) fn open<'a>(run: &'a dyn Run, layout: &str) -> Window<'a> {
    Window::Open {
        run,
        layout: layout.into(),
    }
}

#[test]
fn the_window_is_open_closed_or_unreachable_without_herdr_app_on_path() {
    let bin = tempfile::tempdir().unwrap();
    let path = bin.path().to_str().unwrap();
    assert!(matches!(
        window::ask(&Env::from([("PATH", path)]), &FakeRun::default()).unwrap(),
        Window::NoApp
    ));
    let app = bin.path().join("herdr-app");
    fs::write(&app, "#!/bin/sh\n").unwrap();
    fs::set_permissions(&app, fs::Permissions::from_mode(0o755)).unwrap();
    let env = Env::from([("PATH", path)]);
    assert!(matches!(
        window::ask(&env, &FakeRun::default()).unwrap(),
        Window::Closed
    ));
    let run = FakeRun::new([("herdr-app status", r#"{"layout":"mine","errors":[]}"#)]);
    assert!(matches!(window::ask(&env, &run).unwrap(), Window::Open { layout, .. } if layout == "mine"));
}

#[test]
fn the_mod_files_are_written_once_and_removed_again_leaving_anything_else() {
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

    fs::write(dir.join("jsconfig.json"), "{}").unwrap();
    assert_eq!(files::remove_mod(&dir).unwrap(), Removed::Folder);
    assert!(!dir.exists());
    assert_eq!(
        files::remove_mod(&dir).unwrap(),
        Removed::Nothing,
        "removing again is fine"
    );

    files::install_mod(&dir).unwrap();
    fs::write(dir.join("notes.md"), "mine").unwrap();
    assert_eq!(files::remove_mod(&dir).unwrap(), Removed::Kept(vec!["notes.md".into()]));
    assert_eq!(fs::read_to_string(dir.join("notes.md")).unwrap(), "mine");
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
