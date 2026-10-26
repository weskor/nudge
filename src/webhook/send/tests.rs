use std::net::TcpListener;

use crate::webhook::status::NotifyStatus;
use crate::webhook::tests::{FakeWebhook, config, env, herdr_and_git, webhook_error};
use crate::webhook::{Origin, notify_webhook};

#[test]
fn a_webhook_with_a_signing_secret_gets_a_verifiable_signature() {
    let hook = FakeWebhook::start(|_| (200, String::new()));
    let state = tempfile::tempdir().unwrap();
    let (env, run) = (env(&state, true), herdr_and_git());
    let origin = Origin {
        env: &env,
        cwd: "/home/user/src/api",
        run: &run,
    };
    let _sent = notify_webhook(
        &config(&hook.base),
        Some("research"),
        NotifyStatus::Done,
        "x",
        None,
        origin,
    )
    .unwrap();
    let request = &hook.requests()[0];
    let timestamp = request.header("x-nudge-timestamp").unwrap();
    let signature = request.header("x-nudge-signature").unwrap();
    let age = crate::state::now_ms() / 1000 - timestamp.parse::<i64>().unwrap();
    assert!((0..60).contains(&age), "timestamp is now");
    // What a webhook would do: recompute the HMAC over "<timestamp>.<body>" with the shared secret.
    let key = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, b"research-signing-secret-0123");
    let hex = signature.strip_prefix("sha256=").unwrap();
    let tag: Vec<u8> = hex
        .as_bytes()
        .chunks(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect();
    let signed = format!("{timestamp}.{}", request.body);
    assert!(ring::hmac::verify(&key, signed.as_bytes(), &tag).is_ok());
    assert!(
        ring::hmac::verify(&key, format!("{timestamp}.{{}}").as_bytes(), &tag).is_err(),
        "body is covered"
    );
}

#[test]
fn network_failure_is_a_webhook_error() {
    let closed = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap();
    let state = tempfile::tempdir().unwrap();
    let (env, run) = (env(&state, true), herdr_and_git());
    let origin = Origin {
        env: &env,
        cwd: "/home/user/src/api",
        run: &run,
    };
    let message = webhook_error(notify_webhook(
        &config(&format!("http://{closed}")),
        None,
        NotifyStatus::Done,
        "x",
        None,
        origin,
    ));
    assert!(message.starts_with("could not reach webhook \"bridge\": "), "{message}");
}
