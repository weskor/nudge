//! The pipeline every backend shares: a status change becomes (or doesn't become) a webhook request through the
//! rules, the scope, the route, the terminal tail, redaction, the payload and delivery. It knows agents only as
//! `AgentRef`s and reaches their backend through `AgentSource`.
mod handle;
mod send;

use std::path::Path;

use crate::env::{Env, Run};

pub(crate) use handle::{Outcome, handle};
pub(crate) use send::{locate, send};

/// What the pipeline runs with: the environment, a way to run commands (git), this user's home (the working
/// directory when the agent's is unknown) and where the config lives.
#[derive(Clone, Copy)]
pub(crate) struct Context<'a> {
    pub(crate) env: &'a Env,
    pub(crate) run: &'a dyn Run,
    pub(crate) home: &'a Path,
    pub(crate) config_path: &'a Path,
}
