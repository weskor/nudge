//! Where a notification is sent from: environment, working directory and command runner.
use crate::env::{Env, Run};
use crate::error::Result;

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

/// The process's own working directory as a string, for notifications sent from the CLI.
pub(crate) fn current_dir() -> Result<String> {
    Ok(std::env::current_dir()?.to_string_lossy().into_owned())
}
