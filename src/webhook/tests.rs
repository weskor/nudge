use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::net::TcpListener;
use std::sync::Arc;
use std::thread;

use parking_lot::Mutex;

use super::{NotifyStatus, Origin, Status, notify_webhook, ping_webhook, reply_hint};
use crate::config::{Config, Webhook};
use crate::env::Env;
use crate::env::tests::FakeRun;
use crate::error::Error;
use crate::state::update_state;

/// A request the fake webhook received.
#[derive(Clone, Debug)]
pub(crate) struct Request {
    pub(crate) path: String,
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) body: String,
}

impl Request {
    pub(crate) fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

/// A local HTTP server answering every request with `respond(request)`; records what it received.
pub(crate) struct FakeWebhook {
    pub(crate) base: String,
    pub(crate) requests: Arc<Mutex<Vec<Request>>>,
}

impl FakeWebhook {
    pub(crate) fn start(respond: impl Fn(&Request) -> (u16, String) + Send + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&requests);
        let _server = thread::spawn(move || {
            for stream in listener.incoming() {
                let mut stream = stream.unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let path = line.split_whitespace().nth(1).unwrap_or_default().to_owned();
                let mut headers = Vec::new();
                loop {
                    line.clear();
                    reader.read_line(&mut line).unwrap();
                    let Some((key, value)) = line.trim_end().split_once(": ") else {
                        break;
                    };
                    headers.push((key.to_owned(), value.to_owned()));
                }
                let length = headers
                    .iter()
                    .find(|(key, _)| key.eq_ignore_ascii_case("content-length"))
                    .map_or(0, |(_, value)| value.parse().unwrap());
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                let request = Request {
                    path,
                    headers,
                    body: String::from_utf8(body).unwrap(),
                };
                let (status, text) = respond(&request);
                seen.lock().push(request);
                write!(
                    stream,
                    "HTTP/1.1 {status} X\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{text}",
                    text.len()
                )
                .unwrap();
            }
        });
        Self { base, requests }
    }

    pub(crate) fn requests(&self) -> Vec<Request> {
        self.requests.lock().clone()
    }
}

fn config(base: &str) -> Config {
    Config {
        default_webhook: Some("bridge".into()),
        webhook: vec![
            Webhook {
                name: "bridge".into(),
                description: None,
                url: format!("{base}/bridge"),
                authorization: "Bearer bridge-secret".into(),
                signing_secret: None,
            },
            Webhook {
                name: "research".into(),
                description: None,
                url: format!("{base}/research"),
                authorization: "Token research-secret".into(),
                signing_secret: Some("research-signing-secret-0123".into()),
            },
        ],
        ..Config::default()
    }
}

fn herdr_and_git() -> FakeRun {
    FakeRun::new([
        (
            "herdr agent get w1:p1",
            r#"{"result":{"agent":{"name":"api-refactor","pane_id":"w1:p1"}}}"#,
        ),
        ("git rev-parse --show-toplevel", "/home/user/src/api"),
        ("git remote get-url origin", "git@github.com:acme/api.git"),
        ("git branch --show-current", "main"),
    ])
}

/// An environment in a herdr pane (unless `in_pane` is false) with its own state dir.
fn env(state: &tempfile::TempDir, in_pane: bool) -> Env {
    let dir = state.path().to_str().unwrap();
    if in_pane {
        Env::from([("HERDR_PANE_ID", "w1:p1"), ("NUDGE_STATE_DIR", dir)])
    } else {
        Env::from([("NUDGE_STATE_DIR", dir)])
    }
}

fn webhook_error(result: crate::error::Result<impl std::fmt::Debug>) -> String {
    match result {
        Err(Error::Webhook(error)) => error.message,
        other => panic!("expected a webhook error, got {other:?}"),
    }
}

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
fn every_notification_gets_its_own_id() {
    let hook = FakeWebhook::start(|_| (200, String::new()));
    let state = tempfile::tempdir().unwrap();
    let (env, run) = (env(&state, true), herdr_and_git());
    let origin = Origin {
        env: &env,
        cwd: "/home/user/src/api",
        run: &run,
    };
    let config = config(&hook.base);
    let ids: std::collections::BTreeSet<String> = (0..20)
        .map(|_| {
            notify_webhook(&config, None, NotifyStatus::Done, "x", None, origin)
                .unwrap()
                .notification
                .id
        })
        .collect();
    assert_eq!(ids.len(), 20);
}

#[test]
fn utc_timestamps_match_the_calendar() {
    for (seconds, expected) in [
        (0, "1970-01-01T00:00:00Z"),
        (951_782_400, "2000-02-29T00:00:00Z"),
        (1_800_000_000, "2027-01-15T08:00:00Z"),
        (4_102_444_799, "2099-12-31T23:59:59Z"),
    ] {
        assert_eq!(super::utc_timestamp(seconds), expected);
    }
}

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

#[test]
fn reply_hint_shell_quotes_targets_that_need_it() {
    assert_eq!(
        reply_hint(Some("w1:p1")),
        "reply with: herdr agent prompt w1:p1 \"...\""
    );
    assert_eq!(
        reply_hint(Some("my agent")),
        "reply with: herdr agent prompt 'my agent' \"...\""
    );
    assert_eq!(
        reply_hint(Some("it's")),
        r#"reply with: herdr agent prompt 'it'\''s' "...""#
    );
}
