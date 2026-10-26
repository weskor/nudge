//! Command-line parsing: the options every command shares, read with lexopt.
use crate::error::{Error, Result};
use crate::terminal::tty::Redacted;

/// Parsed command line. Options may appear anywhere, as `--name value` or `--name=value`. `Debug` redacts the
/// Authorization header and the signing secret.
#[derive(Default)]
#[expect(clippy::struct_excessive_bools, reason = "independent command-line flags")]
pub(super) struct Args {
    pub(super) positionals: Vec<String>,
    pub(super) webhook: Option<String>,
    pub(super) status: Option<String>,
    pub(super) agent: Option<String>,
    pub(super) url: Option<String>,
    pub(super) authorization: Option<String>,
    pub(super) authorization_file: Option<String>,
    pub(super) description: Option<String>,
    pub(super) signing_secret: Option<String>,
    pub(super) make_default: bool,
    pub(super) no_test: bool,
    pub(super) yes: bool,
    pub(super) no_auto_notify: bool,
    pub(super) no_install_command: bool,
    pub(super) ping: bool,
    pub(super) pane: bool,
    pub(super) json: bool,
    pub(super) help: bool,
    pub(super) version: bool,
}

impl std::fmt::Debug for Args {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Args")
            .field("positionals", &self.positionals)
            .field("webhook", &self.webhook)
            .field("url", &self.url)
            .field("authorization", &self.authorization.as_deref().map(Redacted))
            .field("signing_secret", &self.signing_secret.as_deref().map(Redacted))
            .finish_non_exhaustive()
    }
}

impl From<lexopt::Error> for Error {
    fn from(error: lexopt::Error) -> Self {
        Self::Usage(error.to_string())
    }
}

/// `argv` is UTF-8 already (checked in `main`), so option values are too.
pub(crate) fn utf8(value: std::ffi::OsString) -> Result<String> {
    value
        .into_string()
        .map_err(|_| Error::Usage("arguments must be UTF-8".into()))
}

pub(super) fn parse_args(argv: Vec<String>) -> Result<Args> {
    use lexopt::Arg::{Long, Short, Value};

    let mut parsed = Args::default();
    let mut parser = lexopt::Parser::from_args(argv);
    while let Some(token) = parser.next()? {
        match token {
            Long("webhook") => parsed.webhook = Some(utf8(parser.value()?)?),
            Long("status") => parsed.status = Some(utf8(parser.value()?)?),
            Long("agent") => parsed.agent = Some(utf8(parser.value()?)?),
            Long("url") => parsed.url = Some(utf8(parser.value()?)?),
            Long("authorization") => parsed.authorization = Some(utf8(parser.value()?)?),
            Long("authorization-file") => parsed.authorization_file = Some(utf8(parser.value()?)?),
            Long("description") => parsed.description = Some(utf8(parser.value()?)?),
            Long("signing-secret") => parsed.signing_secret = Some(utf8(parser.value()?)?),
            Long("default") => parsed.make_default = true,
            Long("no-test") => parsed.no_test = true,
            Long("yes") | Short('y') => parsed.yes = true,
            Long("no-auto-notify") => parsed.no_auto_notify = true,
            Long("no-install-command") => parsed.no_install_command = true,
            Long("ping") => parsed.ping = true,
            Long("pane") => parsed.pane = true,
            Long("json") => parsed.json = true,
            Long("help") | Short('h') => parsed.help = true,
            Long("version") | Short('v') => parsed.version = true,
            Value(positional) => parsed.positionals.push(utf8(positional)?),
            unexpected @ (Long(_) | Short(_)) => return Err(unexpected.unexpected().into()),
        }
    }
    Ok(parsed)
}

/// Refuse extra words after a command that takes none.
pub(super) fn no_args(rest: &[String], command: &str) -> Result<()> {
    if rest.is_empty() {
        Ok(())
    } else {
        Err(Error::Usage(format!("{command} takes no arguments")))
    }
}
