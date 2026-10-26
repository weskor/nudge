use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use super::{link_command, refresh_command_link};
use crate::env::Env;
use crate::error::Error;

/// A temporary home with `~/.local/bin`; returns it, its env and the command link path.
fn home() -> (tempfile::TempDir, Env, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join(".local/bin")).unwrap();
    let env = Env::from([("HOME", dir.path().to_str().unwrap())]);
    let link = dir.path().join(".local/bin/nudge");
    (dir, env, link)
}

#[test]
fn link_command_points_at_root_bin_nudge_replacing_an_old_symlink() {
    let (_home, env, link) = home();
    symlink("/somewhere/else", &link).unwrap();
    let _path = link_command(Path::new("/plugins/nudge-v2"), &env).unwrap();
    assert_eq!(fs::read_link(&link).unwrap(), Path::new("/plugins/nudge-v2/bin/nudge"));
}

#[test]
fn link_command_refuses_to_replace_a_regular_file() {
    let (_home, env, link) = home();
    fs::write(&link, "#!/bin/sh\n").unwrap();
    let result = link_command(Path::new("/plugins/nudge"), &env);
    assert!(matches!(result, Err(Error::Other(message)) if message.contains("not a symlink")));
}

#[test]
fn refresh_repoints_a_link_into_an_older_install() {
    let (_home, env, link) = home();
    symlink("/plugins/nudge-v1/bin/nudge", &link).unwrap();
    assert!(refresh_command_link(Path::new("/plugins/nudge-v2"), &env).unwrap());
    assert_eq!(fs::read_link(&link).unwrap(), Path::new("/plugins/nudge-v2/bin/nudge"));
}

#[test]
fn refresh_leaves_current_foreign_and_missing_links_alone() {
    let root = Path::new("/plugins/nudge-v2");
    let (_current_home, current, current_link) = home();
    symlink("/plugins/nudge-v2/bin/nudge", &current_link).unwrap();
    assert!(!refresh_command_link(root, &current).unwrap());

    let (_foreign_home, foreign, foreign_link) = home();
    for elsewhere in [
        "/usr/local/bin/nudge",
        "/home/user/.cargo/bin/nudge",
        "/home/user/src/nudge/bin/nudge",
    ] {
        fs::remove_file(&foreign_link).ok();
        symlink(elsewhere, &foreign_link).unwrap();
        assert!(!refresh_command_link(root, &foreign).unwrap(), "{elsewhere}");
        assert_eq!(fs::read_link(&foreign_link).unwrap(), Path::new(elsewhere));
    }

    let (_empty_home, empty, _) = home();
    assert!(!refresh_command_link(root, &empty).unwrap());
}
