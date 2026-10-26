use crate::webhook::status::NotifyStatus;
use crate::webhook::tests::{FakeWebhook, config, env, herdr_and_git, webhook_error};
use crate::webhook::{Origin, notify_webhook};

#[test]
fn webhook_rejection_surfaces_status_and_body_without_the_secret() {
    let hook = FakeWebhook::start(|_| (403, "bad auth".into()));
    let state = tempfile::tempdir().unwrap();
    let (env, run) = (env(&state, true), herdr_and_git());
    let origin = Origin {
        env: &env,
        cwd: "/home/user/src/api",
        run: &run,
    };
    let message = webhook_error(notify_webhook(
        &config(&hook.base),
        None,
        NotifyStatus::Done,
        "x",
        None,
        origin,
    ));
    assert_eq!(message, "webhook \"bridge\" returned HTTP 403: bad auth");
}

#[test]
fn an_echoed_header_and_terminal_escapes_never_reach_the_error() {
    let hook = FakeWebhook::start(|request| {
        let sent = request.headers.iter().find(|(key, _)| key == "authorization").unwrap();
        (400, format!("\x1b[31mbad\x1b[0m you sent {}", sent.1))
    });
    let state = tempfile::tempdir().unwrap();
    let (env, run) = (env(&state, true), herdr_and_git());
    let origin = Origin {
        env: &env,
        cwd: "/home/user/src/api",
        run: &run,
    };
    let message = webhook_error(notify_webhook(
        &config(&hook.base),
        None,
        NotifyStatus::Done,
        "x",
        None,
        origin,
    ));
    assert_eq!(
        message,
        "webhook \"bridge\" returned HTTP 400: [31mbad [0m you sent [REDACTED]"
    );
}

#[test]
fn a_redirect_is_reported_not_followed() {
    let hook = FakeWebhook::start(|_| (302, String::new()));
    let state = tempfile::tempdir().unwrap();
    let (env, run) = (env(&state, true), herdr_and_git());
    let origin = Origin {
        env: &env,
        cwd: "/home/user/src/api",
        run: &run,
    };
    let message = webhook_error(notify_webhook(
        &config(&hook.base),
        None,
        NotifyStatus::Done,
        "x",
        None,
        origin,
    ));
    assert!(
        message.starts_with("webhook \"bridge\" returned HTTP 302 (a redirect: nudge doesn't follow redirects"),
        "{message}"
    );
    assert_eq!(hook.requests().len(), 1, "one POST, no follow-up request");
}
