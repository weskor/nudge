use super::{check_authorization, check_webhook_url, validate_webhook};
use crate::config::parse::parse_config;
use crate::config::tests::{VALID, config_error};

#[test]
fn http_is_accepted_only_for_loopback_hosts() {
    let loopback = VALID.replace("https://webhooks.example/hook/bridge", "http://127.0.0.1:8788/x");
    assert!(parse_config(&loopback, "cfg").is_ok());
    let remote = VALID.replace("https://webhooks.example/hook/bridge", "http://webhooks.example/x");
    assert!(config_error(parse_config(&remote, "cfg")).contains("webhook.0.url: must be an https URL"));
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
