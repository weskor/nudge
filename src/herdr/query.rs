//! Asking herdr: run its CLI and parse the JSON answer.
use std::path::Path;

use serde::de::DeserializeOwned;

use crate::env::{Env, Run};

/// Run `herdr <args>` and parse its JSON output; `None` if herdr fails or answers something unexpected.
pub(crate) fn herdr_json<T: DeserializeOwned>(args: &[&str], env: &Env, cwd: &Path, run: &dyn Run) -> Option<T> {
    let cmd: Vec<&str> = std::iter::once(env.herdr()).chain(args.iter().copied()).collect();
    serde_json::from_str(&run.run(&cmd, cwd)?).ok()
}
