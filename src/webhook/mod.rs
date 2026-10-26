//! Build a notification and POST it to a webhook.
use std::time::Duration;

use ring::hmac;
use ring::rand::{SecureRandom as _, SystemRandom};
use serde::Serialize;

use crate::VERSION;
use crate::config::{Config, Webhook, find_webhook};
use crate::env::{Env, Run};
use crate::error::{Error, Result, WebhookError};
use crate::git::git_info;
use crate::herdr::{Sender, detect_sender, in_scope};
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
pub(crate) mod tests;
