use std::fs;

use super::parse_config;
use crate::config::Auto;
use crate::config::tests::{VALID, config_error};

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
