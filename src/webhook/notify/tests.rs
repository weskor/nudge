use crate::error::Error;
use crate::state::update_state;
use crate::webhook::status::{NotifyStatus, Status};
use crate::webhook::tests::{FakeWebhook, config, env, herdr_and_git, webhook_error};
use crate::webhook::{Origin, notify_webhook, ping_webhook};

#[test]
fn posts_the_notification_to_the_default_webhook_with_its_authorization_header() {
    let hook = FakeWebhook::start(|_| (202, "queued".into()));
    let state = tempfile::tempdir().unwrap();
    let (env, run) = (env(&state, true), herdr_and_git());
    let origin = Origin {
        env: &env,
        cwd: "/home/user/src/api",
        run: &run,
    };
    let config = config(&hook.base);
    let sent = notify_webhook(
        &config,
        None,
        NotifyStatus::Blocked,
        "  Need the staging DB password.\n",
        None,
        origin,
    )
    .unwrap();

    let expected = serde_json::json!({
        "from_agent": "api-refactor",
        "cwd": "/home/user/src/api",
        "repo": "acme/api",
        "branch": "main",
        "status": "blocked",
        "message": "Need the staging DB password.",
        "reply_hint": "reply with: herdr agent prompt api-refactor \"...\"",
    });
    assert_eq!((sent.webhook.as_str(), sent.http_status), ("bridge", 202));
    let requests = hook.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].path, "/bridge");
    assert_eq!(requests[0].header("authorization"), Some("Bearer bridge-secret"));
    assert_eq!(requests[0].header("content-type"), Some("application/json"));
    assert_eq!(requests[0].header("x-nudge-signature"), None, "unsigned webhook");
    let mut body: serde_json::Value = serde_json::from_str(&requests[0].body).unwrap();
    let id = body["id"].as_str().unwrap().to_owned();
    let sent_at = body["sent_at"].as_str().unwrap().to_owned();
    assert_eq!(
        (id.as_str(), sent_at.as_str()),
        (sent.notification.id.as_str(), sent.notification.sent_at.as_str())
    );
    assert!(is_uuid_v4(&id), "{id}");
    assert!(is_utc_timestamp(&sent_at), "{sent_at}");
    body.as_object_mut()
        .unwrap()
        .retain(|key, _| key != "id" && key != "sent_at");
    assert_eq!(body, expected);
}

fn is_uuid_v4(id: &str) -> bool {
    let groups: Vec<&str> = id.split('-').collect();
    groups.iter().map(|group| group.len()).eq([8, 4, 4, 4, 12])
        && id
            .chars()
            .all(|c| c == '-' || c.is_ascii_digit() || ('a'..='f').contains(&c))
        && groups[2].starts_with('4')
        && groups[3].starts_with(['8', '9', 'a', 'b'])
}

fn is_utc_timestamp(text: &str) -> bool {
    text.len() == 20
        && text.ends_with('Z')
        && text.chars().enumerate().all(|(index, c)| match index {
            4 | 7 => c == '-',
            10 => c == 'T',
            13 | 16 => c == ':',
            19 => c == 'Z',
            _ => c.is_ascii_digit(),
        })
}

#[test]
fn agents_outside_the_scope_are_refused_before_sending() {
    let hook = FakeWebhook::start(|_| (200, String::new()));
    let state = tempfile::tempdir().unwrap();
    let (env, run) = (env(&state, true), herdr_and_git());
    let origin = Origin {
        env: &env,
        cwd: "/home/user/src/api",
        run: &run,
    };
    let mut config = config(&hook.base);
    config.scope.agents = vec!["reviewer".into()];
    let refused = notify_webhook(&config, None, NotifyStatus::Done, "x", None, origin);
    assert!(
        matches!(&refused, Err(Error::Config(message)) if message.contains("outside nudge's scope")),
        "{refused:?}"
    );
    assert_eq!(hook.requests().len(), 0);
    config.scope.agents = vec!["api-refactor".into()];
    let _sent = notify_webhook(&config, None, NotifyStatus::Done, "x", None, origin).unwrap();
    let _ping = ping_webhook(&config, None, Some("someone-else".into()), origin).unwrap();
    assert_eq!(hook.requests().len(), 2, "in-scope notify and any ping go through");
}

#[test]
fn webhook_and_agent_override_the_default_webhook_and_detected_agent() {
    let hook = FakeWebhook::start(|_| (200, String::new()));
    let state = tempfile::tempdir().unwrap();
    let (env, run) = (env(&state, true), herdr_and_git());
    let origin = Origin {
        env: &env,
        cwd: "/home/user/src/api",
        run: &run,
    };
    let config = config(&hook.base);
    let sent = notify_webhook(
        &config,
        Some("research"),
        NotifyStatus::Question,
        "q",
        Some("reviewer".into()),
        origin,
    )
    .unwrap();
    assert_eq!(hook.requests()[0].path, "/research");
    assert_eq!(
        hook.requests()[0].header("authorization"),
        Some("Token research-secret")
    );
    assert_eq!(sent.notification.from_agent.as_deref(), Some("reviewer"));
    assert_eq!(
        sent.notification.reply_hint,
        "reply with: herdr agent prompt reviewer \"...\""
    );
}

#[test]
fn still_sends_when_the_agent_cannot_be_detected() {
    let hook = FakeWebhook::start(|_| (200, String::new()));
    let state = tempfile::tempdir().unwrap();
    let (env, run) = (env(&state, false), herdr_and_git());
    let origin = Origin {
        env: &env,
        cwd: "/home/user/src/api",
        run: &run,
    };
    let sent = notify_webhook(&config(&hook.base), None, NotifyStatus::Done, "finished", None, origin).unwrap();
    assert_eq!(sent.notification.from_agent, None);
    assert!(sent.notification.reply_hint.contains("could not be detected"));
}

#[test]
fn empty_message_and_unknown_webhook_fail_before_any_request() {
    let hook = FakeWebhook::start(|_| (200, String::new()));
    let state = tempfile::tempdir().unwrap();
    let (env, run) = (env(&state, true), herdr_and_git());
    let origin = Origin {
        env: &env,
        cwd: "/home/user/src/api",
        run: &run,
    };
    let config = config(&hook.base);
    assert_eq!(
        webhook_error(notify_webhook(&config, None, NotifyStatus::Done, " \n ", None, origin)),
        "message is empty"
    );
    let unknown = notify_webhook(&config, Some("nope"), NotifyStatus::Done, "x", None, origin);
    assert!(matches!(unknown, Err(Error::Config(message)) if message.contains("unknown webhook")));
    assert!(hook.requests().is_empty());
}

#[test]
fn records_the_notifying_pane_and_failures_record_nothing() {
    let ok = FakeWebhook::start(|_| (200, String::new()));
    let state = tempfile::tempdir().unwrap();
    let (env, run) = (env(&state, true), herdr_and_git());
    let origin = Origin {
        env: &env,
        cwd: "/home/user/src/api",
        run: &run,
    };
    let before = crate::state::now_ms();
    let _sent = notify_webhook(&config(&ok.base), None, NotifyStatus::Done, "x", None, origin).unwrap();
    let recorded = update_state(&env, |state| state.panes["w1:p1"].self_notified_at).unwrap();
    assert!(recorded.unwrap() >= before);

    let failing = FakeWebhook::start(|_| (500, "nope".into()));
    let failed_state = tempfile::tempdir().unwrap();
    let failed_env = self::env(&failed_state, true);
    let origin = Origin {
        env: &failed_env,
        cwd: "/home/user/src/api",
        run: &run,
    };
    let _failed = notify_webhook(&config(&failing.base), None, NotifyStatus::Done, "x", None, origin).unwrap_err();
    assert!(update_state(&failed_env, |state| state.panes.is_empty()).unwrap());
}

#[test]
fn ping_sends_a_test_status_and_does_not_count_as_the_agent_notifying() {
    let hook = FakeWebhook::start(|_| (204, String::new()));
    let state = tempfile::tempdir().unwrap();
    let (env, run) = (env(&state, true), herdr_and_git());
    let origin = Origin {
        env: &env,
        cwd: "/home/user/src/api",
        run: &run,
    };
    let sent = ping_webhook(&config(&hook.base), None, None, origin).unwrap();
    assert_eq!(sent.webhook, "bridge");
    assert!(update_state(&env, |state| state.panes.is_empty()).unwrap());
    let body: serde_json::Value = serde_json::from_str(&hook.requests()[0].body).unwrap();
    assert_eq!(
        (&body["status"], &body["from_agent"], &body["repo"]),
        (&"test".into(), &"api-refactor".into(), &"acme/api".into())
    );
    assert_eq!(sent.notification.status, Status::Test);
}
