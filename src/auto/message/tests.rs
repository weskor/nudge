use super::{Trigger, auto_message, clean_tail};
use crate::auto::redact::redact;
use crate::auto::rules::Settled;

#[test]
fn clean_tail_drops_ansi_rules_and_blank_runs_and_keeps_the_last_lines() {
    let raw = [
        "\x1b[32mline 1\x1b[0m",
        "",
        "",
        "────────────",
        "line 2",
        "line 3",
        "",
        "━━━",
        "❯",
        "",
        "",
    ]
    .join("\n");
    assert_eq!(clean_tail(&raw, 40), "line 1\n\nline 2\nline 3\n\n❯");
    assert_eq!(clean_tail(&raw, 2), "\n❯");
}

#[test]
fn clean_tail_caps_very_long_output() {
    assert!(clean_tail(&"x".repeat(10_000), 40).chars().count() <= 4_001);
}

#[test]
fn auto_message_says_it_was_auto_detected_and_redacts() {
    let message = auto_message(
        Trigger::Settled(Settled::Blocked),
        "Approve rm -rf build? token=abc12345secret",
        40,
    );
    assert!(message.starts_with("Agent is waiting for approval or input (auto-detected by herdr"));
    assert!(message.contains("token=[REDACTED]"));
    assert!(auto_message(Trigger::Settled(Settled::Done), "  ", 40).ends_with("No terminal output captured."));
}

/// `cat` of a 4096-bit key: 49 base64 lines, more than the 40-line tail, so its BEGIN line is cut.
fn long_key() -> (String, String) {
    let body: Vec<String> = (0..49)
        .map(|i| format!("MIIJKAIBAAKCAgEA{i:02}x+Lk/9qZr3/Wm+Tq8vY2bN5cP0dE7fH1gJ4kS6uX8wZ/aB3+cD{i:02}eF"))
        .collect();
    let key = format!(
        "$ cat ~/.ssh/id_rsa\n-----BEGIN RSA PRIVATE KEY-----\n{}\n-----END RSA PRIVATE KEY-----\n$ ",
        body.join("\n")
    );
    (key, body.join("\n"))
}

#[test]
fn a_private_key_is_redacted_even_when_its_begin_line_is_outside_the_tail() {
    let (key, body) = long_key();
    let message = auto_message(Trigger::Manual, &key, 40);
    for line in body.lines() {
        let middle: String = line.chars().skip(10).take(30).collect();
        assert!(!message.contains(&middle), "key material sent: {message}");
    }
    assert!(message.contains("[REDACTED]"));
    let without_begin = key.lines().skip(10).collect::<Vec<_>>().join("\n");
    let redacted = redact(&without_begin);
    assert!(!redacted.contains("Lk/9qZr3"), "{redacted}");
    assert!(
        redacted.ends_with("[REDACTED]\n$ ") || redacted.ends_with("[REDACTED]\n$"),
        "{redacted}"
    );
}
