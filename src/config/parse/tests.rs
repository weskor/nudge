use std::fs;

use super::parse_config;
use crate::config::Auto;
use crate::config::serialize::serialize_config;
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
fn the_shipped_example_is_rejected_only_until_placeholders_are_filled_in() {
    let example = fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/config.example.toml")).unwrap();
    let message = config_error(parse_config(&example, "example"));
    let problems: Vec<&str> = message.lines().skip(1).collect();
    assert!(!problems.is_empty());
    assert!(problems.iter().all(|line| line.contains("placeholder")), "{message}");
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

const ROUTES: &str = r#"
[[route]]
workspace = "client-a"
webhook = "research"

[[route]]
pane = "api-refactor"
webhook = "bridge"
backend = "core"
"#;

#[test]
fn routes_parse_in_file_order_and_survive_a_save() {
    let config = parse_config(&format!("{VALID}{ROUTES}"), "cfg").unwrap();
    let shown: Vec<String> = config.route.iter().map(ToString::to_string).collect();
    assert_eq!(
        shown,
        [
            r#"workspace "client-a" → research"#,
            r#"pane "api-refactor" → bridge (core only)"#
        ]
    );
    let saved = serialize_config(&config);
    assert_eq!(parse_config(&saved, "saved").unwrap(), config);
    assert_eq!(serialize_config(&parse_config(&saved, "saved").unwrap()), saved);
}

#[test]
fn a_config_without_routes_saves_byte_for_byte_as_before() {
    let config = parse_config(VALID, "cfg").unwrap();
    let saved = serialize_config(&config);
    assert!(!saved.contains("route"), "{saved}");
    assert_eq!(serialize_config(&parse_config(&saved, "saved").unwrap()), saved);
}

#[test]
fn rejects_bad_routes_naming_each() {
    let text = format!(
        "{VALID}\n[[route]]\npane = \"a\"\nwebhook = \"nope\"\n\n[[route]]\npane = \"a\"\nworkspace = \"w\"\nwebhook = \"bridge\"\n\n\
         [[route]]\nwebhook = \"bridge\"\n\n[[route]]\nworkspace = \"w\"\nwebhook = \"bridge\"\nbackend = \"desktop\"\n\n\
         [[route]]\nworkspace = \"w\"\nwebhook = \"bridge\"\nmachine = \"box\"\n\n\
         [[route]]\nworkspace = \"w\"\nwebhook = \"research\"\n\n[[route]]\npane = \" \"\nwebhook = \"bridge\"\n"
    );
    let message = config_error(parse_config(&text, "cfg"));
    for expected in [
        r#"route.0.webhook: no [[webhook]] named "nope""#,
        "route.1: set exactly one of pane or workspace",
        "route.2: set exactly one of pane or workspace",
        r#"route.3.backend: must be "tui" or "core""#,
        "route.4.machine: unknown setting",
        "route.6.pane: must be a non-empty string",
    ] {
        assert!(message.contains(expected), "missing {expected:?} in\n{message}");
    }
    let duplicate = format!(
        "{VALID}\n[[route]]\npane = \"a\"\nwebhook = \"bridge\"\n\n[[route]]\npane = \"a\"\nwebhook = \"research\"\n"
    );
    assert!(
        config_error(parse_config(&duplicate, "cfg"))
            .contains(r#"route.1: duplicate route for pane "a"; an earlier route already matches it"#)
    );
    let other_backend = format!("{duplicate}backend = \"core\"\n");
    assert!(
        parse_config(&other_backend, "cfg").is_ok(),
        "a backend makes it a different route"
    );
}

#[test]
fn core_watch_is_own_unless_connected_and_saved_only_when_connected() {
    let own = parse_config(&format!("{VALID}\n[core]\nwatch = \"own\"\n"), "cfg").unwrap();
    assert!(!own.core.watch_connected);
    assert!(!serialize_config(&own).contains("[core]"), "the default isn't written");
    let connected = parse_config(&format!("{VALID}{ROUTES}\n[core]\nwatch = \"connected\"\n"), "cfg").unwrap();
    assert!(connected.core.watch_connected);
    let saved = serialize_config(&connected);
    assert!(saved.contains("[core]\nwatch = \"connected\"\n"), "{saved}");
    assert_eq!(parse_config(&saved, "saved").unwrap(), connected);
    let message = config_error(parse_config(
        &format!("{VALID}\n[core]\nwatch = \"all\"\nlinger = true\n"),
        "cfg",
    ));
    for expected in [
        r#"core.watch: must be "own" or "connected""#,
        "core.linger: unknown setting",
    ] {
        assert!(message.contains(expected), "missing {expected:?} in\n{message}");
    }
}
