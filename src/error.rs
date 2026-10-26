//! The errors nudge reports, how each maps to an exit code, and the one panic it allows (a broken built-in regex).
use std::fmt;

/// Every failure nudge reports. The variant picks the exit code and whether the herdr plugin shows a toast.
/// Messages never contain secret values.
#[derive(Debug)]
pub(crate) enum Error {
    /// Bad command line: exit 2 with a usage hint.
    Usage(String),
    /// Missing or invalid config, or an unknown webhook.
    Config(String),
    /// The webhook could not be reached, rejected the notification, or there was nothing to send.
    Webhook(WebhookError),
    /// `nudge setup` could not finish.
    Setup(String),
    /// The user pressed Ctrl-C at a prompt.
    Cancelled,
    /// Anything else: file system, herdr.
    Other(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usage(message) | Self::Config(message) | Self::Setup(message) | Self::Other(message) => {
                f.write_str(message)
            }
            Self::Webhook(error) => f.write_str(&error.message),
            Self::Cancelled => f.write_str("cancelled"),
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::Other(error.to_string())
    }
}

impl From<WebhookError> for Error {
    fn from(error: WebhookError) -> Self {
        Self::Webhook(error)
    }
}

/// Which value set up in `nudge setup` a failed webhook call points at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Blame {
    Authorization,
    Url,
}

#[derive(Debug)]
pub(crate) struct WebhookError {
    pub(crate) message: String,
    /// HTTP status when the webhook answered with an error; `None` for network failures and input errors.
    pub(crate) http_status: Option<u16>,
}

impl WebhookError {
    /// 401/403 blame the Authorization header; a redirect (nudge doesn't follow them) or 404/405/410 blames the
    /// URL; anything else blames neither.
    pub(crate) const fn blames(&self) -> Option<Blame> {
        match self.http_status {
            Some(401 | 403) => Some(Blame::Authorization),
            Some(301 | 302 | 303 | 307 | 308 | 404 | 405 | 410) => Some(Blame::Url),
            Some(_) | None => None,
        }
    }
}

pub(crate) type Result<T, E = Error> = std::result::Result<T, E>;

/// The literal patterns in this crate are covered by tests; a broken one is a programming error.
#[expect(clippy::panic, reason = "only reachable if a literal regex in the source is invalid")]
pub(crate) fn unreachable_regex(error: &regex::Error) -> regex::Regex {
    panic!("invalid built-in regex: {error}")
}

#[cfg(test)]
mod tests;
