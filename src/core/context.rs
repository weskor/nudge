//! The desktop app's context (`nudge core context '<json>'`): which terminal on this machine has focus, whether
//! the window is active, and this machine's spaces with their names. The panel publishes it; nudge parses it once and
//! stores it in `context.json` in the state dir, and the watcher reads it for "you're watching this terminal" and for
//! space names.
use std::fs::{self, DirBuilder, OpenOptions};
use std::io::Write as _;
use std::os::unix::fs::{DirBuilderExt as _, OpenOptionsExt as _};

use serde::{Deserialize, Serialize};

use crate::env::Env;
use crate::error::{Error, Result};
use crate::source::AgentRef;
use crate::state::state_dir;

/// A context older than this counts as gone (the app closed or crashed, the laptop slept): nothing is watched.
pub(crate) const FRESH_MS: i64 = 90_000;

/// A space (the desktop app's word for a workspace) on this machine.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SpaceRef {
    pub(crate) id: String,
    pub(crate) name: String,
}

/// What the panel last said about this machine. Unknown fields are ignored, so a newer panel works with this nudge.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct AppContext {
    /// The focused terminal's address on this machine (`t7`), or none.
    pub(crate) terminal: Option<String>,
    pub(crate) window_active: bool,
    #[serde(default)]
    pub(crate) spaces: Vec<SpaceRef>,
    /// When the panel sent it, epoch ms.
    pub(crate) at: i64,
}

impl AppContext {
    /// Whether the person is looking at the terminal at `address` at `now`: it has focus, the window is active, and
    /// the context is fresh.
    pub(crate) fn watches(&self, address: &str, now: i64) -> bool {
        self.window_active && self.terminal.as_deref() == Some(address) && self.fresh(now)
    }

    pub(crate) const fn fresh(&self, now: i64) -> bool {
        now - self.at < FRESH_MS
    }

    /// The name of the space with `id`, if the panel named it.
    pub(crate) fn space_name(&self, id: &str) -> Option<String> {
        self.spaces
            .iter()
            .find(|space| space.id == id)
            .map(|space| space.name.clone())
    }
}

/// Give `agent`'s space its name from the stored context, so routes and `[scope]` can match space names.
pub(crate) fn name_space(agent: &mut AgentRef, context: Option<&AppContext>) {
    if let (Some(workspace), Some(context)) = (agent.workspace.as_mut(), context)
        && workspace.name.is_none()
    {
        workspace.name = context.space_name(&workspace.id);
    }
}

/// `nudge core context '<json>'`: parse and store the panel's context. Malformed JSON is refused and nothing is
/// written.
pub(crate) fn save_app_context(env: &Env, json: &str) -> Result<u8> {
    let context: AppContext = serde_json::from_str(json).map_err(|error| {
        Error::Usage(format!(
            "core context takes JSON {{terminal, window_active, spaces: [{{id, name}}], at}}: {error}"
        ))
    })?;
    let dir = state_dir(env);
    DirBuilder::new().recursive(true).mode(0o700).create(&dir)?;
    let text = serde_json::to_string(&context).map_err(|error| Error::Other(error.to_string()))?;
    let temp = dir.join(format!("context.json.{}.tmp", std::process::id()));
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&temp)?;
    file.write_all(text.as_bytes())?;
    fs::rename(&temp, dir.join("context.json"))?;
    Ok(0)
}

/// The stored context; `None` if there is none or it can't be read.
pub(crate) fn read_app_context(env: &Env) -> Option<AppContext> {
    let text = fs::read_to_string(state_dir(env).join("context.json")).ok()?;
    serde_json::from_str(&text).ok()
}

#[cfg(test)]
mod tests;
