//! `nudge setup`'s command-line options.
use std::fmt;

use crate::terminal::tty::Redacted;

/// `nudge setup`'s flags. `Debug` redacts the Authorization header and the signing secret.
#[derive(Default)]
#[expect(clippy::struct_excessive_bools, reason = "independent command-line flags")]
pub(crate) struct SetupOptions {
    pub(crate) webhook: Option<String>,
    pub(crate) url: Option<String>,
    pub(crate) authorization: Option<String>,
    /// A file holding the Authorization header (`-`: stdin), so it stays out of shell history.
    pub(crate) authorization_file: Option<String>,
    pub(crate) description: Option<String>,
    /// HMAC signing secret shared with the webhook; kept from the existing webhook when not given.
    pub(crate) signing_secret: Option<String>,
    pub(crate) make_default: bool,
    /// Test-ping the webhook before saving (off with `--no-test`).
    pub(crate) test: bool,
    /// No screen: accept every default, and do the step-3 choices that plain scripted setup leaves alone.
    pub(crate) yes: bool,
    pub(crate) no_auto_notify: bool,
    pub(crate) no_install_command: bool,
    /// Running inside a herdr plugin pane: wait for a key before exiting.
    pub(crate) pane: bool,
}

impl fmt::Debug for SetupOptions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SetupOptions")
            .field("webhook", &self.webhook)
            .field("url", &self.url)
            .field("authorization", &self.authorization.as_deref().map(Redacted))
            .field("authorization_file", &self.authorization_file)
            .field("signing_secret", &self.signing_secret.as_deref().map(Redacted))
            .field("yes", &self.yes)
            .field("test", &self.test)
            .finish_non_exhaustive()
    }
}
