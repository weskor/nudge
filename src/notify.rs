//! Build a notification and POST it to a webhook.
use std::time::Duration;

use ring::hmac;
use ring::rand::{SecureRandom as _, SystemRandom};
use serde::Serialize;

use crate::VERSION;
use crate::config::{Config, Webhook, find_webhook};
use crate::context::{Env, Run, Sender, detect_sender, git_info, in_scope};
use crate::error::{Error, Result, WebhookError};
use crate::state::{now_ms, update_state};

const WEBHOOK_TIMEOUT: Duration = Duration::from_secs(15);
const ERROR_BODY_MAX_CHARS: usize = 300;

/// What the agent tells the webhook with `nudge notify --status`.
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

/// The JSON body sent to a webhook; the webhook's automation sees it inside `<webhook_event>`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct Notification {
    /// Unique per notification (UUID v4), so the webhook can drop duplicates.
    pub(crate) id: String,
    /// When nudge built the notification, UTC RFC 3339.
    pub(crate) sent_at: String,
    /// herdr agent target the webhook can reply to; `None` if it could not be detected.
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

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .flat_map(|byte| [byte >> 4, byte & 0x0f])
        .filter_map(|digit| char::from_digit(u32::from(digit), 16))
        .collect()
}

/// A random UUID v4.
fn event_id() -> Result<String> {
    let mut bytes = [0_u8; 16];
    SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| Error::Other("no system randomness for the notification id".into()))?;
    if let Some(version) = bytes.get_mut(6) {
        *version = (*version & 0x0f) | 0x40;
    }
    if let Some(variant) = bytes.get_mut(8) {
        *variant = (*variant & 0x3f) | 0x80;
    }
    let mut rest = bytes.iter().copied();
    let groups: Vec<String> = [4, 2, 2, 2, 6]
        .into_iter()
        .map(|len| hex(&rest.by_ref().take(len).collect::<Vec<u8>>()))
        .collect();
    Ok(groups.join("-"))
}

/// Year, month and day of the `days`-th day after 1970-01-01 (proleptic Gregorian).
const fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + if month <= 2 { 1 } else { 0 };
    (year, month, day)
}

/// `unix_seconds` as UTC RFC 3339, e.g. `2026-10-16T06:12:03Z`.
fn utc_timestamp(unix_seconds: i64) -> String {
    let (year, month, day) = civil_from_days(unix_seconds.div_euclid(86_400));
    let second_of_day = unix_seconds.rem_euclid(86_400);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        second_of_day / 3600,
        second_of_day % 3600 / 60,
        second_of_day % 60
    )
}

/// The message as sent: trimmed and non-empty.
fn message_text(message: &str) -> Result<&str> {
    let message = message.trim();
    if message.is_empty() {
        return Err(WebhookError {
            message: "message is empty".into(),
            http_status: None,
        }
        .into());
    }
    Ok(message)
}

/// Build the notification from `from_agent` (the reply target), with a fresh id and the git repo and branch.
pub(crate) fn build_notification(
    status: Status,
    message: &str,
    from_agent: Option<String>,
    origin: Origin<'_>,
) -> Result<Notification> {
    let message = message_text(message)?;
    let git = git_info(origin.cwd, origin.run);
    Ok(Notification {
        id: event_id()?,
        sent_at: utc_timestamp(now_ms().div_euclid(1000)),
        reply_hint: reply_hint(from_agent.as_deref()),
        from_agent,
        cwd: origin.cwd.to_owned(),
        repo: git.repo,
        branch: git.branch,
        status,
        message: message.to_owned(),
    })
}

/// `sha256=<hex HMAC-SHA256(secret, "<timestamp>.<body>")>`, the `x-nudge-signature` header value.
fn signature(secret: &str, timestamp: i64, body: &str) -> String {
    let key = hmac::Key::new(hmac::HMAC_SHA256, secret.as_bytes());
    let tag = hmac::sign(&key, format!("{timestamp}.{body}").as_bytes());
    format!("sha256={}", hex(tag.as_ref()))
}

/// POST `payload` to the webhook with its Authorization header (and signature, if the webhook has a signing
/// secret); returns the 2xx status.
pub(crate) fn post_to_webhook(webhook: &Webhook, payload: &Notification) -> Result<u16, WebhookError> {
    let unreachable = |detail: &dyn std::fmt::Display| WebhookError {
        message: format!("could not reach webhook \"{}\": {detail}", webhook.name),
        http_status: None,
    };
    let body = serde_json::to_string(payload).map_err(|error| unreachable(&error))?;
    // A redirect would turn the POST into a body-less GET (and drop the headers), so it's reported, not followed.
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(WEBHOOK_TIMEOUT))
        .http_status_as_error(false)
        .max_redirects(0)
        .build()
        .into();
    let mut request = agent
        .post(&webhook.url)
        .header("authorization", &webhook.authorization)
        .header("content-type", "application/json")
        .header("user-agent", format!("nudge/{VERSION}"));
    if let Some(secret) = &webhook.signing_secret {
        let timestamp = now_ms().div_euclid(1000);
        request = request
            .header("x-nudge-timestamp", timestamp.to_string())
            .header("x-nudge-signature", signature(secret, timestamp, &body));
    }
    let response = request.send(body).map_err(|error| unreachable(&error))?;
    let status = response.status().as_u16();
    if response.status().is_success() {
        return Ok(status);
    }
    let moved = response.status().is_redirection().then(|| {
        let location = response.headers().get("location").and_then(|value| value.to_str().ok());
        let location = shown(location.unwrap_or_default(), webhook);
        format!(" (a redirect: nudge doesn't follow redirects; set the webhook URL to where it points: {location})")
    });
    let text = shown(&response.into_body().read_to_string().unwrap_or_default(), webhook);
    let text = text.trim();
    let detail = if text.chars().count() > ERROR_BODY_MAX_CHARS {
        format!(": {}…", text.chars().take(ERROR_BODY_MAX_CHARS).collect::<String>())
    } else if text.is_empty() {
        String::new()
    } else {
        format!(": {text}")
    };
    Err(WebhookError {
        message: format!(
            "webhook \"{}\" returned HTTP {status}{}{detail}",
            webhook.name,
            moved.unwrap_or_default()
        ),
        http_status: Some(status),
    })
}

/// Text from the webhook's response, safe to print and log: control characters (terminal escapes) become spaces,
/// and the webhook's own secrets (an endpoint may echo the request back) become `[REDACTED]`.
fn shown(text: &str, webhook: &Webhook) -> String {
    let mut text: String = text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    let token = webhook.authorization.split_once(' ').map(|(_, token)| token.trim());
    let secrets = [
        Some(webhook.authorization.as_str()),
        token,
        webhook.signing_secret.as_deref(),
    ];
    for secret in secrets.into_iter().flatten().filter(|secret| secret.len() >= 4) {
        text = text.replace(secret, "[REDACTED]");
    }
    text
}

/// A notification the webhook accepted.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Sent {
    pub(crate) webhook: String,
    /// HTTP status the webhook answered with.
    pub(crate) http_status: u16,
    pub(crate) notification: Notification,
}

/// Who is sending: `agent` if named explicitly, else detected from herdr.
fn sender(agent: Option<String>, origin: Origin<'_>) -> Sender {
    agent.map_or_else(
        || detect_sender(origin.env, origin.cwd, origin.run),
        |agent| Sender::named(agent, origin.env),
    )
}

/// Tell a webhook (the default webhook unless `webhook` is given) that this agent is done, blocked or has a
/// question. Refuses agents outside the config's `[scope]`. Inside a herdr pane, records that the agent notified,
/// so the herdr plugin doesn't send a duplicate.
pub(crate) fn notify_webhook(
    config: &Config,
    webhook: Option<&str>,
    status: NotifyStatus,
    message: &str,
    agent: Option<String>,
    origin: Origin<'_>,
) -> Result<Sent> {
    let webhook = find_webhook(config, webhook)?;
    let message = message_text(message)?;
    let sender = sender(agent, origin);
    if !in_scope(&config.scope, &sender, origin.env, origin.run) {
        let who = sender
            .agent
            .as_deref()
            .map_or_else(|| "this agent".to_owned(), |agent| format!("agent \"{agent}\""));
        let place = sender
            .workspace_id
            .as_deref()
            .map(|id| format!(" in workspace \"{id}\""))
            .unwrap_or_default();
        return Err(Error::Config(format!(
            "{who}{place} is outside nudge's scope, so nothing was sent. Change [scope] in the nudge config, \
             or run the herdr action \"Nudge: notify from all agents\"."
        )));
    }
    let notification = build_notification(status.into(), message, sender.agent, origin)?;
    let http_status = post_to_webhook(webhook, &notification)?;
    if let Some(pane_id) = origin.env.get("HERDR_PANE_ID") {
        let now = now_ms();
        // Best effort: a missing record only risks one duplicate automatic notification.
        update_state(origin.env, |state| {
            state.panes.entry(pane_id.to_owned()).or_default().self_notified_at = Some(now);
        })
        .ok();
    }
    Ok(Sent {
        webhook: webhook.name.clone(),
        http_status,
        notification,
    })
}

/// Send a harmless `status: "test"` notification to check the webhook URL and Authorization header. Ignores
/// `[scope]`: it tests the webhook, not an agent.
pub(crate) fn ping_webhook(
    config: &Config,
    webhook: Option<&str>,
    agent: Option<String>,
    origin: Origin<'_>,
) -> Result<Sent> {
    let webhook = find_webhook(config, webhook)?;
    let from_agent = sender(agent, origin).agent;
    let notification = build_notification(Status::Test, "nudge test ping. No action needed.", from_agent, origin)?;
    let http_status = post_to_webhook(webhook, &notification)?;
    Ok(Sent {
        webhook: webhook.name.clone(),
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

    use super::{NotifyStatus, Origin, Status, notify_webhook, ping_webhook, reply_hint};
    use crate::config::{Config, Webhook};
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
}
