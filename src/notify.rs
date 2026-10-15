//! Build a notification and POST it to a desk webhook.
use std::thread;
use std::time::Duration;

use serde::Serialize;

use crate::VERSION;
use crate::config::{Config, Desk, find_desk};
use crate::context::{Env, Run, detect_agent, git_info};
use crate::error::{DeskError, Error, Result};
use crate::state::{now_ms, update_state};

const WEBHOOK_TIMEOUT: Duration = Duration::from_secs(15);
const ERROR_BODY_MAX_CHARS: usize = 300;

/// What the agent tells the desk with `nudge notify --status`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum NotifyStatus {
    Done,
    Blocked,
    Question,
}

impl NotifyStatus {
    pub(crate) const ALL: [Self; 3] = [Self::Done, Self::Blocked, Self::Question];

    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Done => "done",
            Self::Blocked => "blocked",
            Self::Question => "question",
        }
    }

    pub(crate) fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|status| status.as_str() == text)
    }
}

/// The `status` field of a notification.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Status {
    Done,
    Blocked,
    Question,
    /// Only sent by `nudge test` pings.
    Test,
}

impl From<NotifyStatus> for Status {
    fn from(status: NotifyStatus) -> Self {
        match status {
            NotifyStatus::Done => Self::Done,
            NotifyStatus::Blocked => Self::Blocked,
            NotifyStatus::Question => Self::Question,
        }
    }
}

/// The JSON body sent to a desk webhook; the desk sees it inside `<webhook_event>`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct Notification {
    /// herdr agent target the desk can reply to; `None` if it could not be detected.
    pub(crate) from_agent: Option<String>,
    pub(crate) cwd: String,
    pub(crate) repo: Option<String>,
    pub(crate) branch: Option<String>,
    pub(crate) status: Status,
    pub(crate) message: String,
    pub(crate) reply_hint: String,
}

/// Where a notification is sent from: environment, working directory and command runner.
#[derive(Clone, Copy)]
pub(crate) struct Origin<'a> {
    pub(crate) env: &'a Env,
    pub(crate) cwd: &'a str,
    pub(crate) run: &'a dyn Run,
}

impl std::fmt::Debug for Origin<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Origin").field("cwd", &self.cwd).finish_non_exhaustive()
    }
}

pub(crate) fn reply_hint(agent: Option<&str>) -> String {
    let Some(agent) = agent else {
        return "sender's herdr agent could not be detected, so it cannot be prompted directly".to_owned();
    };
    let shell_safe = !agent.is_empty()
        && agent
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._:@%+=,-".contains(c));
    let target = if shell_safe {
        agent.to_owned()
    } else {
        format!("'{}'", agent.replace('\'', r"'\''"))
    };
    format!("reply with: herdr agent prompt {target} \"...\"")
}

/// Build the notification: trimmed message, the agent to reply to (`agent`, else detected), git repo and branch.
pub(crate) fn build_notification(
    status: Status,
    message: &str,
    agent: Option<String>,
    origin: Origin<'_>,
) -> Result<Notification> {
    let message = message.trim();
    if message.is_empty() {
        return Err(DeskError {
            message: "message is empty".into(),
            http_status: None,
        }
        .into());
    }
    let (from_agent, git) = thread::scope(|scope| {
        let git = scope.spawn(|| git_info(origin.cwd, origin.run));
        let agent = agent.or_else(|| detect_agent(origin.env, origin.cwd, origin.run));
        (agent, git.join())
    });
    let git = git.map_err(|_| Error::Other("git lookup failed".into()))?;
    Ok(Notification {
        reply_hint: reply_hint(from_agent.as_deref()),
        from_agent,
        cwd: origin.cwd.to_owned(),
        repo: git.repo,
        branch: git.branch,
        status,
        message: message.to_owned(),
    })
}

/// POST `payload` to the desk webhook with its Authorization header; returns the 2xx status.
pub(crate) fn post_to_desk(desk: &Desk, payload: &Notification) -> Result<u16, DeskError> {
    let unreachable = |detail: &dyn std::fmt::Display| DeskError {
        message: format!("could not reach desk \"{}\" webhook: {detail}", desk.name),
        http_status: None,
    };
    let body = serde_json::to_string(payload).map_err(|error| unreachable(&error))?;
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(WEBHOOK_TIMEOUT))
        .http_status_as_error(false)
        .build()
        .into();
    let response = agent
        .post(&desk.url)
        .header("authorization", &desk.authorization)
        .header("content-type", "application/json")
        .header("user-agent", format!("nudge/{VERSION}"))
        .send(body)
        .map_err(|error| unreachable(&error))?;
    let status = response.status().as_u16();
    if response.status().is_success() {
        return Ok(status);
    }
    let text = response.into_body().read_to_string().unwrap_or_default();
    let text = text.trim();
    let detail = if text.chars().count() > ERROR_BODY_MAX_CHARS {
        format!(": {}…", text.chars().take(ERROR_BODY_MAX_CHARS).collect::<String>())
    } else if text.is_empty() {
        String::new()
    } else {
        format!(": {text}")
    };
    Err(DeskError {
        message: format!("desk \"{}\" webhook returned HTTP {status}{detail}", desk.name),
        http_status: Some(status),
    })
}

/// A notification the desk accepted.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Sent {
    pub(crate) desk: String,
    /// HTTP status the webhook answered with.
    pub(crate) http_status: u16,
    pub(crate) notification: Notification,
}

/// Tell a desk (the default desk unless `desk` is given) that this agent is done, blocked or has a question.
/// Inside a herdr pane, records that the agent notified, so the herdr plugin doesn't send a duplicate.
pub(crate) fn notify_desk(
    config: &Config,
    desk: Option<&str>,
    status: NotifyStatus,
    message: &str,
    agent: Option<String>,
    origin: Origin<'_>,
) -> Result<Sent> {
    let desk = find_desk(config, desk)?;
    let notification = build_notification(status.into(), message, agent, origin)?;
    let http_status = post_to_desk(desk, &notification)?;
    if let Some(pane_id) = origin.env.get("HERDR_PANE_ID") {
        let now = now_ms();
        // Best effort: a missing record only risks one duplicate automatic notification.
        update_state(origin.env, |state| {
            state.panes.entry(pane_id.to_owned()).or_default().self_notified_at = Some(now);
        })
        .ok();
    }
    Ok(Sent {
        desk: desk.name.clone(),
        http_status,
        notification,
    })
}

/// Send a harmless `status: "test"` notification to check the webhook URL and Authorization header.
pub(crate) fn ping_desk(
    config: &Config,
    desk: Option<&str>,
    agent: Option<String>,
    origin: Origin<'_>,
) -> Result<Sent> {
    let desk = find_desk(config, desk)?;
    let notification = build_notification(Status::Test, "nudge test ping. No action needed.", agent, origin)?;
    let http_status = post_to_desk(desk, &notification)?;
    Ok(Sent {
        desk: desk.name.clone(),
        http_status,
        notification,
    })
}

/// The process's own working directory as a string, for notifications sent from the CLI.
pub(crate) fn current_dir() -> Result<String> {
    Ok(std::env::current_dir()?.to_string_lossy().into_owned())
}

#[cfg(test)]
pub(crate) mod tests {
    use std::io::{BufRead as _, BufReader, Read as _, Write as _};
    use std::net::TcpListener;
    use std::sync::Arc;
    use std::thread;

    use parking_lot::Mutex;

    use super::{NotifyStatus, Origin, Status, notify_desk, ping_desk, reply_hint};
    use crate::config::{Config, Desk};
    use crate::context::Env;
    use crate::context::tests::FakeRun;
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
            default_desk: Some("bridge".into()),
            desk: vec![
                Desk {
                    name: "bridge".into(),
                    description: None,
                    url: format!("{base}/bridge"),
                    authorization: "Bearer bridge-secret".into(),
                },
                Desk {
                    name: "research".into(),
                    description: None,
                    url: format!("{base}/research"),
                    authorization: "Token research-secret".into(),
                },
            ],
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

    fn desk_error(result: crate::error::Result<impl std::fmt::Debug>) -> String {
        match result {
            Err(Error::Desk(error)) => error.message,
            other => panic!("expected a desk error, got {other:?}"),
        }
    }

    #[test]
    fn posts_the_notification_to_the_default_desk_with_its_authorization_header() {
        let hook = FakeWebhook::start(|_| (202, "queued".into()));
        let state = tempfile::tempdir().unwrap();
        let (env, run) = (env(&state, true), herdr_and_git());
        let origin = Origin {
            env: &env,
            cwd: "/home/user/src/api",
            run: &run,
        };
        let config = config(&hook.base);
        let sent = notify_desk(
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
        assert_eq!((sent.desk.as_str(), sent.http_status), ("bridge", 202));
        assert_eq!(serde_json::to_value(&sent.notification).unwrap(), expected);
        let requests = hook.requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].path, "/bridge");
        assert_eq!(requests[0].header("authorization"), Some("Bearer bridge-secret"));
        assert_eq!(requests[0].header("content-type"), Some("application/json"));
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&requests[0].body).unwrap(),
            expected
        );
    }

    #[test]
    fn desk_and_agent_override_the_default_desk_and_detected_agent() {
        let hook = FakeWebhook::start(|_| (200, String::new()));
        let state = tempfile::tempdir().unwrap();
        let (env, run) = (env(&state, true), herdr_and_git());
        let origin = Origin {
            env: &env,
            cwd: "/home/user/src/api",
            run: &run,
        };
        let config = config(&hook.base);
        let sent = notify_desk(
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
        let sent = notify_desk(&config(&hook.base), None, NotifyStatus::Done, "finished", None, origin).unwrap();
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
        let message = desk_error(notify_desk(
            &config(&hook.base),
            None,
            NotifyStatus::Done,
            "x",
            None,
            origin,
        ));
        assert_eq!(message, "desk \"bridge\" webhook returned HTTP 403: bad auth");
    }

    #[test]
    fn network_failure_is_a_desk_error() {
        let closed = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap();
        let state = tempfile::tempdir().unwrap();
        let (env, run) = (env(&state, true), herdr_and_git());
        let origin = Origin {
            env: &env,
            cwd: "/home/user/src/api",
            run: &run,
        };
        let message = desk_error(notify_desk(
            &config(&format!("http://{closed}")),
            None,
            NotifyStatus::Done,
            "x",
            None,
            origin,
        ));
        assert!(
            message.starts_with("could not reach desk \"bridge\" webhook: "),
            "{message}"
        );
    }

    #[test]
    fn empty_message_and_unknown_desk_fail_before_any_request() {
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
            desk_error(notify_desk(&config, None, NotifyStatus::Done, " \n ", None, origin)),
            "message is empty"
        );
        let unknown = notify_desk(&config, Some("nope"), NotifyStatus::Done, "x", None, origin);
        assert!(matches!(unknown, Err(Error::Config(message)) if message.contains("unknown desk")));
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
        let _sent = notify_desk(&config(&ok.base), None, NotifyStatus::Done, "x", None, origin).unwrap();
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
        let _failed = notify_desk(&config(&failing.base), None, NotifyStatus::Done, "x", None, origin).unwrap_err();
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
        let sent = ping_desk(&config(&hook.base), None, None, origin).unwrap();
        assert_eq!(sent.desk, "bridge");
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
}
