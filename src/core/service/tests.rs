use std::fs;
use std::path::Path;

use super::{Service, ServiceAction};
use crate::core::unit_file::ServiceKind;
use crate::env::Env;
use crate::env::tests::FakeRun;

const NUDGE: &str = "/home/me/.local/bin/nudge";

fn env(home: &Path) -> Env {
    Env::from([
        ("HOME", home.to_str().unwrap()),
        ("USER", "me"),
        ("PATH", "/home/me/bin::.:bin:/usr/bin"),
    ])
}

fn service<'a>(kind: ServiceKind, env: &'a Env, run: &'a FakeRun) -> Service<'a> {
    Service {
        kind,
        env,
        run,
        uid: 501,
    }
}

#[test]
fn systemd_install_writes_the_unit_reloads_enables_and_restarts_it() {
    let home = tempfile::tempdir().unwrap();
    let env = env(home.path());
    let run = FakeRun::new([
        ("systemctl --user daemon-reload", ""),
        ("systemctl --user enable nudge-watch.service", ""),
        ("systemctl --user restart nudge-watch.service", ""),
        ("loginctl show-user me --property=Linger --value", "no"),
    ]);
    let service = service(ServiceKind::SystemdUser, &env, &run);
    assert_eq!(service.act(ServiceAction::Install, Path::new(NUDGE)).unwrap(), 0);
    let unit = fs::read_to_string(home.path().join(".config/systemd/user/nudge-watch.service")).unwrap();
    assert!(
        unit.contains("ExecStart=\"/home/me/.local/bin/nudge\" core watch\n"),
        "{unit}"
    );
    assert!(
        unit.contains("Environment=\"PATH=/home/me/bin:/usr/bin\"\n"),
        "empty and relative PATH entries are dropped: {unit}"
    );
    assert_eq!(
        run.calls(),
        [
            "systemctl --user daemon-reload",
            "systemctl --user enable nudge-watch.service",
            "systemctl --user restart nudge-watch.service",
            "loginctl show-user me --property=Linger --value"
        ],
        "a reinstall restarts a running watcher, so it runs the new unit"
    );
    let note = service.linger_note().unwrap();
    assert!(note.contains("loginctl enable-linger me"), "{note}");
}

#[test]
fn a_failed_enable_is_an_error_naming_the_command_and_lingering_on_needs_no_note() {
    let home = tempfile::tempdir().unwrap();
    let env = env(home.path());
    let run = FakeRun::new([
        ("systemctl --user daemon-reload", ""),
        ("loginctl show-user me --property=Linger --value", "yes"),
    ]);
    let service = service(ServiceKind::SystemdUser, &env, &run);
    let error = service.act(ServiceAction::Install, Path::new(NUDGE)).unwrap_err();
    assert_eq!(
        error.to_string(),
        "`systemctl --user enable nudge-watch.service` failed"
    );
    assert_eq!(service.linger_note(), None);
}

#[test]
fn systemd_status_and_remove() {
    let home = tempfile::tempdir().unwrap();
    let env = env(home.path());
    let run = FakeRun::new([
        ("systemctl --user is-active nudge-watch.service", "active"),
        ("systemctl --user daemon-reload", ""),
    ]);
    let service = service(ServiceKind::SystemdUser, &env, &run);
    assert_eq!(service.act(ServiceAction::Remove, Path::new(NUDGE)).unwrap(), 0);
    assert!(run.calls().is_empty(), "nothing installed, nothing to remove");
    let unit = home.path().join(".config/systemd/user/nudge-watch.service");
    fs::create_dir_all(unit.parent().unwrap()).unwrap();
    fs::write(&unit, "[Unit]\n").unwrap();
    assert!(service.running());
    assert_eq!(service.act(ServiceAction::Remove, Path::new(NUDGE)).unwrap(), 0);
    assert!(!unit.exists());
    assert_eq!(
        run.calls(),
        [
            "systemctl --user is-active nudge-watch.service",
            "systemctl --user disable --now nudge-watch.service",
            "systemctl --user daemon-reload"
        ]
    );
}

#[test]
fn launchd_install_bootstraps_the_agent_in_the_users_domain_and_remove_boots_it_out() {
    let home = tempfile::tempdir().unwrap();
    let env = env(home.path());
    let plist = home.path().join("Library/LaunchAgents/dev.nudge.watch.plist");
    let bootstrap = format!("launchctl bootstrap gui/501 {}", plist.display());
    let run = FakeRun::new([
        (bootstrap.as_str(), ""),
        ("launchctl print gui/501/dev.nudge.watch", "state = running\npid = 42"),
    ]);
    let service = service(ServiceKind::LaunchdAgent, &env, &run);
    assert_eq!(service.act(ServiceAction::Install, Path::new(NUDGE)).unwrap(), 0);
    assert!(
        fs::read_to_string(&plist)
            .unwrap()
            .contains("<string>dev.nudge.watch</string>")
    );
    assert!(service.running());
    assert_eq!(service.act(ServiceAction::Remove, Path::new(NUDGE)).unwrap(), 0);
    assert!(!plist.exists());
    assert_eq!(
        run.calls(),
        [
            "launchctl bootout gui/501/dev.nudge.watch".to_owned(),
            bootstrap,
            "launchctl print gui/501/dev.nudge.watch".to_owned(),
            "launchctl bootout gui/501/dev.nudge.watch".to_owned()
        ]
    );
    assert_eq!(service.linger_note(), None, "lingering is a systemd matter");
}

#[test]
fn a_path_with_no_absolute_entries_falls_back_to_the_system_defaults() {
    let home = tempfile::tempdir().unwrap();
    let env = Env::from([("HOME", home.path().to_str().unwrap()), ("PATH", "bin:.:")]);
    let plist = home.path().join("Library/LaunchAgents/dev.nudge.watch.plist");
    let bootstrap = format!("launchctl bootstrap gui/501 {}", plist.display());
    let run = FakeRun::new([(bootstrap.as_str(), "")]);
    let service = service(ServiceKind::LaunchdAgent, &env, &run);
    assert_eq!(service.act(ServiceAction::Install, Path::new(NUDGE)).unwrap(), 0);
    let text = fs::read_to_string(&plist).unwrap();
    assert!(text.contains("<string>/usr/local/bin:/usr/bin:/bin</string>"), "{text}");
}
