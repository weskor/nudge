use std::fs;
use std::os::unix::fs::PermissionsExt as _;

use super::write_config;
use crate::config::tests::{VALID, config_of, webhook};
use crate::config::{Auto, Scope, load_config, upsert_webhook};

#[test]
fn write_config_uses_mode_600_in_a_mode_700_dir_and_round_trips() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("nudge");
    let path = dir.join("config.toml");
    let mut bridge = webhook("bridge", "Bearer a");
    bridge.description = Some("Says \"hi\"\\ and\tmore".into());
    bridge.signing_secret = Some("0123456789abcdef-secret".into());
    let mut config = config_of(Some("bridge"), vec![bridge, webhook("research", "Bearer b")]);
    config.auto = Auto {
        min_turn_seconds: 30,
        cooldown_seconds: 0,
        tail_lines: 80,
        toast: false,
    };
    config.scope = Scope {
        workspaces: vec!["team \"bridge\"".into()],
        agents: vec!["api-refactor".into(), "w1:p2".into()],
    };
    write_config(&path, &config).unwrap();
    assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
    assert_eq!(fs::metadata(&dir).unwrap().permissions().mode() & 0o777, 0o700);
    assert_eq!(load_config(&path).unwrap(), config);
}

#[test]
fn write_config_tightens_an_existing_world_readable_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(&path, VALID).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    let config = upsert_webhook(
        Some(load_config(&path).unwrap()),
        webhook("research", "Bearer b"),
        false,
    );
    write_config(&path, &config).unwrap();
    assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
    let names: Vec<String> = load_config(&path)
        .unwrap()
        .webhook
        .into_iter()
        .map(|w| w.name)
        .collect();
    assert_eq!(names, ["bridge", "research"]);
}

#[test]
fn write_config_keeps_a_symlinked_config_a_symlink() {
    let dir = tempfile::tempdir().unwrap();
    let real = dir.path().join("dotfiles").join("nudge.toml");
    fs::create_dir_all(real.parent().unwrap()).unwrap();
    fs::write(&real, VALID).unwrap();
    let link = dir.path().join("config.toml");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let config = upsert_webhook(
        Some(load_config(&link).unwrap()),
        webhook("research", "Bearer b"),
        false,
    );
    write_config(&link, &config).unwrap();
    assert_eq!(fs::read_link(&link).unwrap(), real, "still a link to the same file");
    assert_eq!(load_config(&real).unwrap(), config);
}

#[test]
fn a_failed_write_leaves_the_old_config_and_no_temp_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(&path, VALID).unwrap();
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o500)).unwrap();
    let config = upsert_webhook(
        Some(load_config(&path).unwrap()),
        webhook("research", "Bearer b"),
        false,
    );
    let result = write_config(&path, &config);
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).unwrap();
    assert!(result.is_err());
    assert_eq!(fs::read_to_string(&path).unwrap(), VALID);
    let names: Vec<_> = fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(names, ["config.toml"]);
}
