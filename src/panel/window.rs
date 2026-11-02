//! Herdr's window and its layout files: whether `herdr-app` is there and the window open (and which layout it
//! shows), running `herdr-app do` commands, and reading and writing `layouts/<name>.json`.
use std::fs;
use std::path::Path;

use serde::Deserialize;

use super::files::write_whole;
use super::layout::{self, Node};
use crate::env::{Env, Run};
use crate::error::{Error, Result};

/// Herdr's app CLI, from PATH: it talks to the open window.
const HERDR_APP: &str = "herdr-app";

/// Herdr's window, as far as the panel needs.
pub(super) enum Window<'a> {
    /// `herdr-app` isn't on PATH.
    NoApp,
    /// `herdr-app` is there, but no window answers.
    Closed,
    /// The open window, showing `layout`; `run` runs `herdr-app` commands against it.
    Open { run: &'a dyn Run, layout: String },
}

/// What `herdr-app status` says, as far as the panel needs.
#[derive(Debug, Deserialize)]
struct Status {
    layout: String,
}

/// What `shared.json` says: the layout Herdr shows (also while closed: it opens with it).
#[derive(Debug, Deserialize)]
struct Shared {
    layout: Option<String>,
}

/// Ask `herdr-app status` about the window.
pub(super) fn ask<'a>(env: &Env, run: &'a dyn Run) -> Result<Window<'a>> {
    if env.which(HERDR_APP).is_none() {
        return Ok(Window::NoApp);
    }
    let Some(text) = run.run(&[HERDR_APP, "status"], Path::new("/")) else {
        return Ok(Window::Closed);
    };
    let status: Status = serde_json::from_str(&text).map_err(|error| {
        Error::Other(format!(
            "`herdr-app status` answered something nudge can't read: {error}"
        ))
    })?;
    Ok(Window::Open {
        run,
        layout: status.layout,
    })
}

/// The layout a closed Herdr opens with, from its `shared.json` (read only); `None` if it doesn't say.
pub(super) fn opens_with(home: &Path) -> Option<String> {
    let text = fs::read_to_string(home.join("shared.json")).ok()?;
    serde_json::from_str::<Shared>(&text).ok()?.layout
}

/// `layouts/<name>.json`: its text, and the text read as a tree.
pub(super) fn read_layout(layouts: &Path, name: &str) -> Result<(String, Node)> {
    let path = layouts.join(format!("{name}.json"));
    let text = fs::read_to_string(&path)
        .map_err(|error| Error::Other(format!("can't read Herdr's layout {}: {error}", path.display())))?;
    let tree = layout::parse(&text, name)?;
    Ok((text, tree))
}

/// Save `text` as `layouts/<name>.json`, whole; Herdr shows the change at once.
pub(super) fn write_layout(layouts: &Path, name: &str, text: &str) -> Result<()> {
    write_whole(&layouts.join(format!("{name}.json")), text)
}

/// Run a `herdr-app` command that has to work; `what` says what it was for when it doesn't.
pub(super) fn herdr_app(run: &dyn Run, args: &[&str], what: &str) -> Result<()> {
    let mut cmd = vec![HERDR_APP];
    cmd.extend_from_slice(args);
    run.run(&cmd, Path::new("/"))
        .map(|_| ())
        .ok_or_else(|| Error::Other(format!("Herdr couldn't {what}")))
}
