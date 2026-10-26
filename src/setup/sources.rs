//! Where setup's values come from before anyone is asked: flags, the `--authorization-file`, the environment, and
//! the saved webhook this run updates.
use std::io::{IsTerminal as _, Read as _};

use super::SetupOptions;
use super::wizard::{Source, Sourced};
use crate::config::{Config, Webhook};
use crate::env::Env;
use crate::error::{Error, Result};

/// The saved webhook named `name`, which this run updates.
pub(super) fn saved_webhook(existing: Option<&Config>, name: &str) -> Option<Webhook> {
    existing
        .and_then(|config| config.webhook.iter().find(|webhook| webhook.name == name))
        .cloned()
}

/// The step-1 values from the command line and the environment, before the saved webhook, with where each came
/// from. Values are as given (scripted setup reports problems with the whole webhook; the wizard checks them first).
pub(super) fn given(options: &SetupOptions, env: &Env) -> Result<(Sourced, Sourced)> {
    let flag = |value: &Option<String>| value.clone().filter(|value| !value.is_empty());
    let from_env = |name: &str, source| env.get(name).map(|value| (value.to_owned(), source));
    let url = flag(&options.url)
        .map(|url| (url, Source::Flag))
        .or_else(|| from_env("NUDGE_WEBHOOK_URL", Source::Environment));
    let file = match (flag(&options.authorization), flag(&options.authorization_file)) {
        (Some(_), Some(_)) => {
            return Err(Error::Usage(
                "use --authorization or --authorization-file, not both".into(),
            ));
        }
        (Some(value), None) => return Ok((url, Some((value, Source::Flag)))),
        (None, file) => file,
    };
    let authorization = match file.as_deref() {
        // Reading a terminal would echo the header as it's typed, so `-` needs a pipe or a redirect.
        Some("-") if std::io::stdin().is_terminal() => {
            return Err(Error::Usage(
                "--authorization-file - reads the header from a pipe, but stdin is a terminal; pipe it in \
                 (pass show hook | nudge setup --authorization-file -) or pass a file path"
                    .into(),
            ));
        }
        Some("-") => {
            let mut input = String::new();
            std::io::stdin().read_to_string(&mut input)?;
            Some((input.trim().to_owned(), Source::File))
        }
        Some(path) => {
            let text = std::fs::read_to_string(path)
                .map_err(|error| Error::Setup(format!("can't read --authorization-file {path}: {error}")))?;
            Some((text.trim().to_owned(), Source::File))
        }
        None => from_env("NUDGE_WEBHOOK_AUTHORIZATION", Source::Environment),
    };
    Ok((url, authorization))
}
