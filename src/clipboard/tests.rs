use std::fs;

use super::{Desktop, clipboard_commands, read_clipboard};
use crate::env::Env;
use crate::env::tests::FakeRun;

const WL: &str = "wl-paste --no-newline --type text";
const XC: &str = "xclip -selection clipboard -o";

fn graphical_session() -> Env {
    Env::from([
        ("WAYLAND_DISPLAY", "wayland-1"),
        ("XDG_RUNTIME_DIR", "/run/user/1000"),
        ("DISPLAY", ":0"),
    ])
}

#[test]
fn prefers_wl_paste_in_the_current_session() {
    let runtime = tempfile::tempdir().unwrap();
    let run = FakeRun::new([(WL, "https://webhook.example/hook"), (XC, "other")]);
    assert_eq!(
        read_clipboard(Desktop::Unix, &run, &graphical_session(), runtime.path()).as_deref(),
        Some("https://webhook.example/hook")
    );
    assert_eq!(run.calls(), [WL]);
}

#[test]
fn falls_back_to_xclip_when_wl_paste_fails_or_is_empty() {
    let runtime = tempfile::tempdir().unwrap();
    for run in [
        FakeRun::new([(XC, "Bearer abc")]),
        FakeRun::new([(WL, ""), (XC, "Bearer abc")]),
    ] {
        assert_eq!(
            read_clipboard(Desktop::Unix, &run, &graphical_session(), runtime.path()).as_deref(),
            Some("Bearer abc")
        );
    }
}

#[test]
fn from_a_plugin_pane_without_session_env_it_finds_the_wayland_socket() {
    let runtime = tempfile::tempdir().unwrap();
    fs::write(runtime.path().join("wayland-1"), "").unwrap();
    fs::write(runtime.path().join("wayland-1.lock"), "").unwrap();
    let cmd = format!(
        "env XDG_RUNTIME_DIR={} WAYLAND_DISPLAY=wayland-1 {WL}",
        runtime.path().display()
    );
    let run = FakeRun::new([(cmd.as_str(), "https://webhook.example/hook")]);
    assert_eq!(
        read_clipboard(Desktop::Unix, &run, &Env::default(), runtime.path()).as_deref(),
        Some("https://webhook.example/hook")
    );
}

#[test]
fn none_when_nothing_yields_text() {
    let runtime = tempfile::tempdir().unwrap();
    let run = FakeRun::default();
    assert_eq!(
        read_clipboard(Desktop::Unix, &run, &graphical_session(), runtime.path()),
        None
    );
    assert_eq!(run.calls(), [WL, XC]);
}

#[test]
fn session_fallbacks_only_when_the_env_lacks_them() {
    let joined = |commands: Vec<Vec<String>>| commands.into_iter().map(|c| c.join(" ")).collect::<Vec<_>>();
    assert_eq!(
        joined(clipboard_commands(
            Desktop::Unix,
            &graphical_session(),
            "/run/user/1000",
            &["wayland-1".into()]
        )),
        [WL, XC]
    );
    assert_eq!(
        joined(clipboard_commands(
            Desktop::Unix,
            &Env::default(),
            "/run/user/1000",
            &["wayland-0".into(), "wayland-1".into()]
        )),
        [
            WL.to_owned(),
            format!("env XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-0 {WL}"),
            format!("env XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-1 {WL}"),
            XC.to_owned(),
            format!("env DISPLAY=:0 {XC}"),
        ]
    );
}

#[test]
fn macos_reads_pbpaste_only() {
    let commands = clipboard_commands(Desktop::MacOs, &Env::default(), "", &["wayland-0".into()]);
    assert_eq!(commands, [["pbpaste"]]);
}
