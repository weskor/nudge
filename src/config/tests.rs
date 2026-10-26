use std::fs;
use std::os::unix::fs::PermissionsExt as _;

use super::{
    Auto, Config, Scope, Webhook, check_authorization, check_webhook_url, default_config_path, find_webhook,
    load_config, parse_config, read_and_migrate, read_config, serialize_config, upsert_webhook, validate_webhook,
    write_config,
};
use crate::env::Env;
use crate::error::Error;

const VALID: &str = r#"
default_webhook = "bridge"

[[webhook]]
name = "bridge"
description = "Coordinator"
url = "https://webhooks.example/hook/bridge"
authorization = "Bearer webhook-token"

[[webhook]]
name = "research"
url = "https://webhooks.example/hook/research"
authorization = "Token xyz"
"#;

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

fn webhook(name: &str, authorization: &str) -> Webhook {
    Webhook {
        name: name.to_owned(),
        description: None,
        url: format!("https://webhooks.example/hook/{name}"),
        authorization: authorization.to_owned(),
        signing_secret: None,
    }
}

fn config_of(default_webhook: Option<&str>, webhook: Vec<Webhook>) -> Config {
    Config {
        default_webhook: default_webhook.map(str::to_owned),
        webhook,
        ..Config::default()
    }
}

fn config_error(result: Result<impl std::fmt::Debug, Error>) -> String {
    match result {
        Err(Error::Config(message)) => message,
        other => panic!("expected a config error, got {other:?}"),
    }
}

#[test]
fn parses_webhooks_and_default_webhook() {
    let config = parse_config(VALID, "cfg").unwrap();
    assert_eq!(config.default_webhook.as_deref(), Some("bridge"));
    assert_eq!(
        config.webhook.iter().map(|w| w.name.as_str()).collect::<Vec<_>>(),
        ["bridge", "research"]
    );
    assert_eq!(config.webhook[1].description, None);
}

#[test]
fn the_shipped_example_is_rejected_until_placeholders_are_filled_in() {
    let example = fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/config.example.toml")).unwrap();
    assert!(config_error(parse_config(&example, "example")).contains("placeholder"));
}

#[test]
fn http_is_accepted_only_for_loopback_hosts() {
    let loopback = VALID.replace("https://webhooks.example/hook/bridge", "http://127.0.0.1:8788/x");
    assert!(parse_config(&loopback, "cfg").is_ok());
    let remote = VALID.replace("https://webhooks.example/hook/bridge", "http://webhooks.example/x");
    assert!(config_error(parse_config(&remote, "cfg")).contains("webhook.0.url: must be an https URL"));
}

#[test]
fn rejects_invalid_configs_naming_the_field() {
    for (label, text, expected) in [
        ("no webhooks", r#"default_webhook = "bridge""#.to_owned(), "webhook:"),
        (
            "duplicate webhook names",
            VALID.replace(r#"name = "research""#, r#"name = "bridge""#),
            r#"webhook.1.name: duplicate webhook name "bridge""#,
        ),
        (
            "unknown default_webhook",
            VALID.replace(r#"default_webhook = "bridge""#, r#"default_webhook = "nope""#),
            r#"default_webhook: no [[webhook]] named "nope""#,
        ),
        (
            "multi-line authorization",
            VALID.replace(r#""Token xyz""#, r#""Token x\ny""#),
            "webhook.1.authorization",
        ),
        (
            "bad webhook name",
            VALID.replace(r#"name = "research""#, r#"name = "re search""#),
            "webhook.1.name",
        ),
        ("invalid TOML", "[[webhook]".to_owned(), "invalid TOML"),
    ] {
        assert!(config_error(parse_config(&text, "cfg")).contains(expected), "{label}");
    }
}

#[test]
fn reports_every_problem_and_skips_cross_checks_over_wrong_types() {
    let message = config_error(parse_config("[[webhook]]\nname = \"\"\nurl = 5\n", "cfg"));
    assert_eq!(
        message,
        "cfg: invalid config\n  webhook.0.name: must be a non-empty string\n  \
         webhook.0.name: may only contain letters, digits, \"-\" and \"_\"\n  \
         webhook.0.url: Invalid input: expected string, received number\n  \
         webhook.0.authorization: Invalid input: expected string, received undefined"
    );
}

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

#[test]
fn find_webhook_by_name_default_or_only_webhook() {
    let config = parse_config(VALID, "cfg").unwrap();
    assert_eq!(find_webhook(&config, Some("research")).unwrap().name, "research");
    assert_eq!(find_webhook(&config, None).unwrap().name, "bridge");
    let single = config_of(None, vec![webhook("research", "Bearer b")]);
    assert_eq!(find_webhook(&single, None).unwrap().name, "research");
}

#[test]
fn find_webhook_errors_name_the_configured_webhooks() {
    let two = config_of(
        None,
        vec![webhook("bridge", "Bearer a"), webhook("research", "Bearer b")],
    );
    assert!(config_error(find_webhook(&two, None)).contains("no default_webhook; pass --webhook"));
    let config = parse_config(VALID, "cfg").unwrap();
    assert!(config_error(find_webhook(&config, Some("nope"))).contains("Configured webhooks: bridge, research"));
}

#[test]
fn validate_webhook_strips_a_pasted_header_name_and_whitespace() {
    let webhook = validate_webhook(
        "bridge",
        " https://webhooks.example/x ",
        "Authorization: Bearer abc \n",
        None,
        None,
    )
    .unwrap();
    assert_eq!(webhook.authorization, "Bearer abc");
    assert_eq!(webhook.url, "https://webhooks.example/x");
}

#[test]
fn validate_webhook_reports_every_problem() {
    let message = config_error(validate_webhook("bridge", "not a url", "", None, None));
    let url = message.find("url: must be an https URL").unwrap();
    let authorization = message.find("authorization: must be a non-empty string").unwrap();
    assert!(url < authorization);
}

#[test]
fn setup_field_checks_never_echo_the_rejected_value() {
    assert_eq!(
        check_webhook_url(" https://webhooks.example/hook\n").unwrap(),
        "https://webhooks.example/hook"
    );
    assert_eq!(check_authorization("authorization: Bearer abc").unwrap(), "Bearer abc");
    for message in [
        config_error(check_webhook_url("http://secret-host.example/x")),
        config_error(check_authorization("Bearer super-secret\nsecond-line")),
        config_error(check_authorization("Bearer s3cr\u{e9}t")),
    ] {
        assert!(!message.contains("secret") && !message.contains("s3cr"), "{message}");
    }
}

#[test]
fn upsert_first_webhook_becomes_default_and_later_ones_only_when_asked() {
    let bridge = webhook("bridge", "Bearer a");
    let research = webhook("research", "Bearer b");
    let one = upsert_webhook(None, bridge.clone(), false);
    assert_eq!(one, config_of(Some("bridge"), vec![bridge.clone()]));
    let two = upsert_webhook(Some(one.clone()), research.clone(), false);
    assert_eq!(two, config_of(Some("bridge"), vec![bridge, research.clone()]));
    assert_eq!(
        upsert_webhook(Some(one), research, true).default_webhook.as_deref(),
        Some("research")
    );
}

#[test]
fn upsert_existing_name_replaces_it_in_place_and_keeps_other_settings() {
    let mut two = config_of(Some("research"), vec![webhook("bridge", "a"), webhook("research", "b")]);
    two.auto.toast = false;
    two.scope.agents = vec!["api".into()];
    let rotated = webhook("bridge", "Bearer rotated");
    let updated = upsert_webhook(Some(two.clone()), rotated.clone(), false);
    assert_eq!(updated.webhook, [rotated, webhook("research", "b")]);
    assert_eq!(
        (updated.default_webhook, updated.auto, updated.scope),
        (two.default_webhook, two.auto, two.scope)
    );
}

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

#[test]
fn serialized_output_omits_absent_and_default_settings() {
    let mut config = config_of(None, vec![webhook("bridge", "Bearer a")]);
    let text = serialize_config(&config);
    for absent in ["default_webhook", "description", "signing_secret", "[auto]", "[scope]"] {
        assert!(!text.contains(absent), "{absent}");
    }
    config.auto.cooldown_seconds = 60;
    let text = serialize_config(&config);
    assert!(text.contains("[auto]\ncooldown_seconds = 60\n"), "{text}");
    assert!(!text.contains("min_turn_seconds"), "{text}");
}

#[test]
fn auto_and_scope_settings_parse_with_defaults_for_missing_keys() {
    let text = format!("{VALID}\n[auto]\nmin_turn_seconds = 0\ntail_lines = 200\n\n[scope]\nagents = [\"api\"]\n");
    let config = parse_config(&text, "cfg").unwrap();
    let expected = Auto {
        min_turn_seconds: 0,
        tail_lines: 200,
        ..Auto::default()
    };
    assert_eq!(config.auto, expected);
    assert_eq!(config.scope.agents, ["api"]);
    assert!(config.scope.workspaces.is_empty());
}

#[test]
fn rejects_bad_auto_and_scope_settings_naming_each() {
    let text = format!(
        "{VALID}\n[auto]\nmin_turn_seconds = -1\ncooldown_seconds = 1.5\ntail_lines = 0\ntoast = \"yes\"\n\
         min_turn = 5\n\n[scope]\nagents = [\"\"]\nworkspaces = \"base\"\nteams = []\n"
    );
    let message = config_error(parse_config(&text, "cfg"));
    for expected in [
        "auto.min_turn_seconds: must be a whole number of seconds, 0 or more",
        "auto.cooldown_seconds: must be a whole number of seconds, 0 or more",
        "auto.tail_lines: must be a whole number from 1 to 200",
        "auto.toast: Invalid input: expected boolean, received string",
        "auto.min_turn: unknown setting",
        "scope.agents.0: must be a non-empty string",
        "scope.workspaces: Invalid input: expected array, received string",
        "scope.teams: unknown setting",
    ] {
        assert!(message.contains(expected), "missing {expected:?} in\n{message}");
    }
}

#[test]
fn signing_secrets_must_be_long_and_are_never_echoed() {
    let short = VALID.replace(
        "authorization = \"Token xyz\"",
        "authorization = \"Token xyz\"\nsigning_secret = \"tiny-secret\"",
    );
    let message = config_error(parse_config(&short, "cfg"));
    assert!(message.contains("webhook.1.signing_secret: must be at least 16 printable ASCII characters"));
    assert!(!message.contains("tiny-secret"));
    let webhook = validate_webhook("d", "https://x.example/h", "Bearer a", None, Some(" 0123456789abcdef ")).unwrap();
    assert_eq!(webhook.signing_secret.as_deref(), Some("0123456789abcdef"));
}

#[test]
fn durations_in_words() {
    assert_eq!(
        [120, 60, 90, 1, 0].map(super::seconds_in_words),
        ["2 minutes", "1 minute", "90 seconds", "1 second", "0 seconds"]
    );
}
