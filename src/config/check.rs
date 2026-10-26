//! Value checks shared by the config parser and `nudge setup`: webhook names, URLs, Authorization values and
//! signing secrets.
use super::model::Webhook;
use super::problems::Problems;
use crate::error::Result;

const PLACEHOLDER_PREFIX: &str = "REPLACE_WITH";
const LOOPBACK_HOSTS: [&str; 3] = ["localhost", "127.0.0.1", "[::1]"];
const AUTHORIZATION_PREFIX: &str = "authorization:";

impl Problems {
    /// Trimmed `raw`, which must be non-empty and free of `REPLACE_WITH…` placeholders.
    pub(super) fn non_empty(&mut self, path: &str, raw: &str) -> String {
        let value = raw.trim();
        if value.is_empty() {
            self.add(path, "must be a non-empty string");
        }
        if value.contains(PLACEHOLDER_PREFIX) {
            self.add(path, &format!("still contains a {PLACEHOLDER_PREFIX}… placeholder"));
        }
        value.to_owned()
    }

    pub(super) fn webhook_name(&mut self, path: &str, raw: &str) -> String {
        let name = self.non_empty(path, raw);
        let mut chars = name.chars();
        let valid = chars.next().is_some_and(|first| first.is_ascii_alphanumeric())
            && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
        if !valid {
            self.add(path, r#"may only contain letters, digits, "-" and "_""#);
        }
        name
    }

    pub(super) fn webhook_url(&mut self, path: &str, raw: &str) -> String {
        let url = self.non_empty(path, raw);
        let allowed = url::Url::parse(&url).is_ok_and(|parsed| match parsed.scheme() {
            "https" => true,
            "http" => parsed.host_str().is_some_and(|host| LOOPBACK_HOSTS.contains(&host)),
            _ => false,
        });
        if !allowed {
            self.add(path, "must be an https URL (http is allowed only for localhost)");
        }
        url
    }

    /// The header value; a pasted "Authorization:" header name is stripped.
    pub(super) fn authorization(&mut self, path: &str, raw: &str) -> String {
        let trimmed = raw.trim();
        let value = match trimmed.split_at_checked(AUTHORIZATION_PREFIX.len()) {
            Some((name, rest)) if name.eq_ignore_ascii_case(AUTHORIZATION_PREFIX) => rest.trim_start(),
            Some(_) | None => trimmed,
        };
        let value = self.non_empty(path, value);
        if value.is_empty() || !value.chars().all(|c| matches!(c, ' '..='~')) {
            self.add(path, "must be printable ASCII on a single line");
        }
        value
    }

    pub(super) fn signing_secret(&mut self, path: &str, raw: &str) -> String {
        let value = self.non_empty(path, raw);
        if value.chars().count() < 16 || !value.chars().all(|c| c.is_ascii_graphic()) {
            self.add(path, "must be at least 16 printable ASCII characters, without spaces");
        }
        value
    }
}

/// Validate a webhook URL entered during setup.
pub(crate) fn check_webhook_url(value: &str) -> Result<String> {
    let mut problems = Problems::default();
    let url = problems.webhook_url("", value);
    problems.into_result(url, "invalid webhook URL")
}

/// Validate an Authorization header value entered during setup (strips a pasted "Authorization:" prefix).
pub(crate) fn check_authorization(value: &str) -> Result<String> {
    let mut problems = Problems::default();
    let authorization = problems.authorization("", value);
    problems.into_result(authorization, "invalid Authorization value")
}

/// Validate a webhook name entered during setup.
pub(crate) fn check_webhook_name(value: &str) -> Result<String> {
    let mut problems = Problems::default();
    let name = problems.webhook_name("", value);
    problems.into_result(name, "invalid webhook name")
}

/// Validate one webhook entered via `nudge setup`.
pub(crate) fn validate_webhook(
    name: &str,
    url: &str,
    authorization: &str,
    description: Option<&str>,
    signing_secret: Option<&str>,
) -> Result<Webhook> {
    let mut problems = Problems::default();
    let webhook = Webhook {
        name: problems.webhook_name("name", name),
        description: description.map(str::to_owned),
        url: problems.webhook_url("url", url),
        authorization: problems.authorization("authorization", authorization),
        signing_secret: signing_secret.map(|secret| problems.signing_secret("signing_secret", secret)),
    };
    problems.into_result(webhook, "invalid webhook")
}

#[cfg(test)]
mod tests;
