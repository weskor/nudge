use std::fmt;

/// Every failure nudge reports. The variant picks the exit code and whether the herdr plugin shows a toast.
/// Messages never contain secret values.
#[derive(Debug)]
pub(crate) enum Error {
    /// Bad command line: exit 2 with a usage hint.
    Usage(String),
    /// Missing or invalid config, or an unknown desk.
    Config(String),
    /// The webhook could not be reached, rejected the notification, or there was nothing to send.
    Desk(DeskError),
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
            Self::Desk(error) => f.write_str(&error.message),
            Self::Cancelled => f.write_str("cancelled"),
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::Other(error.to_string())
    }
}

impl From<DeskError> for Error {
    fn from(error: DeskError) -> Self {
        Self::Desk(error)
    }
}

/// Which value set up in `nudge setup` a failed webhook call points at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Blame {
    Authorization,
    Url,
}

#[derive(Debug)]
pub(crate) struct DeskError {
    pub(crate) message: String,
    /// HTTP status when the webhook answered with an error; `None` for network failures and input errors.
    pub(crate) http_status: Option<u16>,
}

impl DeskError {
    /// 401/403 blame the Authorization header, 404/405/410 the URL; anything else blames neither.
    pub(crate) const fn blames(&self) -> Option<Blame> {
        match self.http_status {
            Some(401 | 403) => Some(Blame::Authorization),
            Some(404 | 405 | 410) => Some(Blame::Url),
            Some(_) | None => None,
        }
    }
}

pub(crate) type Result<T, E = Error> = std::result::Result<T, E>;

#[cfg(test)]
mod tests {
    use super::{Blame, DeskError};

    #[test]
    fn blames_points_at_the_authorization_header_or_the_url() {
        let blame = |status| {
            DeskError {
                message: String::new(),
                http_status: status,
            }
            .blames()
        };
        assert_eq!(blame(Some(401)), Some(Blame::Authorization));
        assert_eq!(blame(Some(403)), Some(Blame::Authorization));
        assert_eq!(blame(Some(404)), Some(Blame::Url));
        assert_eq!(blame(Some(500)), None);
        assert_eq!(blame(None), None);
    }
}
