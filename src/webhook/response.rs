//! A webhook's non-2xx answer as an error that is safe to print and log.
use ureq::Body;
use ureq::http::Response;

use crate::config::Webhook;
use crate::error::WebhookError;

const ERROR_BODY_MAX_CHARS: usize = 300;

/// The error for a response the webhook didn't accept: its HTTP status, where a redirect points, and the start of
/// the body, all [`shown`] safely.
pub(super) fn rejection(webhook: &Webhook, response: Response<Body>) -> WebhookError {
    let status = response.status().as_u16();
    let moved = response.status().is_redirection().then(|| {
        let location = response.headers().get("location").and_then(|value| value.to_str().ok());
        let location = shown(location.unwrap_or_default(), webhook);
        format!(" (a redirect: nudge doesn't follow redirects; set the webhook URL to where it points: {location})")
    });
    let text = shown(&response.into_body().read_to_string().unwrap_or_default(), webhook);
    WebhookError {
        message: format!(
            "webhook \"{}\" returned HTTP {status}{}{}",
            webhook.name,
            moved.unwrap_or_default(),
            detail(text.trim())
        ),
        http_status: Some(status),
    }
}

/// `": <text>"`, cut after [`ERROR_BODY_MAX_CHARS`] characters; nothing for an empty body.
fn detail(text: &str) -> String {
    if text.chars().count() > ERROR_BODY_MAX_CHARS {
        format!(": {}…", text.chars().take(ERROR_BODY_MAX_CHARS).collect::<String>())
    } else if text.is_empty() {
        String::new()
    } else {
        format!(": {text}")
    }
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

#[cfg(test)]
mod tests;
