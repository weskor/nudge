use std::fs;
use std::os::unix::fs::PermissionsExt as _;

use super::{default_config_path, load_config, read_and_migrate, read_config};
use crate::config::parse::parse_config;
use crate::config::serialize::serialize_config;
use crate::config::tests::{VALID, config_error, config_of, webhook};
use crate::env::Env;

/// A config in the old format: `default_desk` and `[[desk]]` tables.
const OLD_FORMAT: &str = r#"
# hand-written
default_desk = "research"

[[desk]]
name = "bridge"
description = "Coordinator"
url = "https://webhooks.example/hook/bridge"
authorization = "Bearer webhook-token"
signing_secret = "0123456789abcdef-secret"

[[desk]]
name = "research"
url = "https://webhooks.example/hook/research"
authorization = "Token xyz"

[auto]
cooldown_seconds = 60

[scope]
agents = ["api"]
"#;

#[test]
fn an_old_format_config_is_rewritten_once_keeping_every_setting() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(&path, OLD_FORMAT).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();

    let (config, note) = read_and_migrate(&path).unwrap().unwrap();
    assert_eq!(
        note.unwrap(),
        format!(
            "updated {} to the new format ([[webhook]], default_webhook)",
            path.display()
        )
    );
    let mut bridge = webhook("bridge", "Bearer webhook-token");
    bridge.description = Some("Coordinator".into());
    bridge.signing_secret = Some("0123456789abcdef-secret".into());
    let mut expected = config_of(Some("research"), vec![bridge, webhook("research", "Token xyz")]);
    expected.auto.cooldown_seconds = 60;
    expected.scope.agents = vec!["api".into()];
    assert_eq!(config, expected);

    let text = fs::read_to_string(&path).unwrap();
    assert_eq!(text, serialize_config(&expected));
    assert!(!text.contains("desk"), "{text}");
    assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);

    assert_eq!(read_and_migrate(&path).unwrap(), Some((expected, None)));
    assert_eq!(fs::read_to_string(&path).unwrap(), text);
}

#[test]
fn old_keys_are_an_error_outside_a_migration_naming_them() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let mixed = format!("default_desk = \"bridge\"\n{VALID}");
    fs::write(&path, &mixed).unwrap();
    let message = config_error(read_config(&path));
    assert!(
        message.contains("default_desk: old name of default_webhook; rename it"),
        "{message}"
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), mixed);
    assert!(config_error(parse_config(OLD_FORMAT, "cfg")).contains("  desk: old name of webhook"));
}

#[test]
fn default_config_path_precedence() {
    let path = |env: Env| default_config_path(&env).display().to_string();
    assert_eq!(
        path(Env::from([("NUDGE_CONFIG", "/x/c.toml"), ("XDG_CONFIG_HOME", "/xdg")])),
        "/x/c.toml"
    );
    assert_eq!(path(Env::from([("XDG_CONFIG_HOME", "/xdg")])), "/xdg/nudge/config.toml");
    assert_eq!(
        path(Env::from([("HOME", "/home/u")])),
        "/home/u/.config/nudge/config.toml"
    );
}

#[test]
fn missing_config_reads_as_none_and_load_points_at_setup() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nope.toml");
    assert_eq!(read_config(&path).unwrap(), None);
    assert!(config_error(load_config(&path)).contains("nudge setup"));
}
