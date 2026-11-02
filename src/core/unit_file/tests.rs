use std::path::{Path, PathBuf};

use super::{ServiceKind, service_file};
use crate::env::Env;

#[test]
fn the_systemd_unit_runs_the_watcher_with_the_captured_path() {
    let unit = service_file(
        ServiceKind::SystemdUser,
        Path::new("/home/me/.local/bin/nudge"),
        "/home/me/bin:/usr/bin",
    );
    assert_eq!(
        unit,
        "# Written by `nudge core service install`; `nudge core service remove` deletes it.\n\
         [Unit]\n\
         Description=nudge: notify your webhook about herdr-core agents\n\
         \n\
         [Service]\n\
         ExecStart=\"/home/me/.local/bin/nudge\" core watch\n\
         Restart=on-failure\n\
         RestartSec=10\n\
         Environment=\"PATH=/home/me/bin:/usr/bin\"\n\
         \n\
         [Install]\n\
         WantedBy=default.target\n"
    );
    let odd = service_file(
        ServiceKind::SystemdUser,
        Path::new("/opt/my \"nudge\"/50%/nudge"),
        "/a\\b",
    );
    assert!(
        odd.contains(r#"ExecStart="/opt/my \"nudge\"/50%%/nudge" core watch"#),
        "{odd}"
    );
    assert!(odd.contains(r#"Environment="PATH=/a\\b""#), "{odd}");
}

#[test]
fn the_launchd_agent_runs_at_load_and_again_after_a_failure() {
    let plist = service_file(
        ServiceKind::LaunchdAgent,
        Path::new("/Users/me/bin/nudge & co"),
        "/opt/homebrew/bin:/usr/bin",
    );
    assert_eq!(
        plist,
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <!-- Written by `nudge core service install`; `nudge core service remove` deletes it. -->\n\
         <plist version=\"1.0\">\n\
         <dict>\n\
         \x20 <key>Label</key>\n\
         \x20 <string>dev.nudge.watch</string>\n\
         \x20 <key>ProgramArguments</key>\n\
         \x20 <array>\n\
         \x20   <string>/Users/me/bin/nudge &amp; co</string>\n\
         \x20   <string>core</string>\n\
         \x20   <string>watch</string>\n\
         \x20 </array>\n\
         \x20 <key>EnvironmentVariables</key>\n\
         \x20 <dict>\n\
         \x20   <key>PATH</key>\n\
         \x20   <string>/opt/homebrew/bin:/usr/bin</string>\n\
         \x20 </dict>\n\
         \x20 <key>RunAtLoad</key>\n\
         \x20 <true/>\n\
         \x20 <key>KeepAlive</key>\n\
         \x20 <dict>\n\
         \x20   <key>SuccessfulExit</key>\n\
         \x20   <false/>\n\
         \x20 </dict>\n\
         </dict>\n\
         </plist>\n"
    );
}

#[test]
fn where_each_file_goes() {
    let home = Env::from([("HOME", "/home/me")]);
    assert_eq!(
        ServiceKind::SystemdUser.path(&home),
        PathBuf::from("/home/me/.config/systemd/user/nudge-watch.service")
    );
    let xdg = Env::from([("HOME", "/home/me"), ("XDG_CONFIG_HOME", "/cfg")]);
    assert_eq!(
        ServiceKind::SystemdUser.path(&xdg),
        PathBuf::from("/cfg/systemd/user/nudge-watch.service")
    );
    assert_eq!(
        ServiceKind::LaunchdAgent.path(&home),
        PathBuf::from("/home/me/Library/LaunchAgents/dev.nudge.watch.plist")
    );
}
