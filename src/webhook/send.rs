//! POST a notification to a webhook: the HTTP client, the request headers and the request signature.
use std::time::Duration;

use ring::hmac;
use ureq::RequestBuilder;
use ureq::typestate::WithBody;

use super::hex::hex;
use super::payload::Notification;
use super::response::rejection;
use crate::VERSION;
use crate::config::Webhook;
use crate::error::WebhookError;
use crate::state::now_ms;

const WEBHOOK_TIMEOUT: Duration = Duration::from_secs(15);

/// POST `payload` to the webhook with its Authorization header (and signature, if the webhook has a signing
/// secret); returns the 2xx status.
pub(crate) fn post_to_webhook(webhook: &Webhook, payload: &Notification) -> Result<u16, WebhookError> {
    let body = serde_json::to_string(payload).map_err(|error| unreachable_error(webhook, &error))?;
    let response = request(webhook, &body)
        .send(body)
        .map_err(|error| unreachable_error(webhook, &error))?;
    let status = response.status().as_u16();
    if response.status().is_success() {
        return Ok(status);
    }
    Err(rejection(webhook, response))
}

fn unreachable_error(webhook: &Webhook, detail: &dyn std::fmt::Display) -> WebhookError {
    WebhookError {
        message: format!("could not reach webhook \"{}\": {detail}", webhook.name),
        http_status: None,
    }
}

/// The POST request for `body`, with the webhook's headers and, if it has a signing secret, the signature.
fn request(webhook: &Webhook, body: &str) -> RequestBuilder<WithBody> {
    // A redirect would turn the POST into a body-less GET (and drop the headers), so it's reported, not followed.
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(WEBHOOK_TIMEOUT))
        .http_status_as_error(false)
        .max_redirects(0)
        .build()
        .into();
    let request = agent
        .post(&webhook.url)
        .header("authorization", &webhook.authorization)
        .header("content-type", "application/json")
        .header("user-agent", format!("nudge/{VERSION}"));
    let Some(secret) = &webhook.signing_secret else {
        return request;
    };
    let timestamp = now_ms().div_euclid(1000);
    request
        .header("x-nudge-timestamp", timestamp.to_string())
        .header("x-nudge-signature", signature(secret, timestamp, body))
}

/// `sha256=<hex HMAC-SHA256(secret, "<timestamp>.<body>")>`, the `x-nudge-signature` header value.
fn signature(secret: &str, timestamp: i64, body: &str) -> String {
    let key = hmac::Key::new(hmac::HMAC_SHA256, secret.as_bytes());
    let tag = hmac::sign(&key, format!("{timestamp}.{body}").as_bytes());
    format!("sha256={}", hex(tag.as_ref()))
}

#[cfg(test)]
mod tests;
