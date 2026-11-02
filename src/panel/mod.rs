//! `nudge panel install`: put the Herdr desktop panel in place and dock it, with nothing to do by hand. The mod's
//! files go into Herdr's `mods/nudge/`; then, while Herdr's window is open, the panel is docked on the right of
//! the layout it shows. Herdr's own layouts take no new panels, so one of them is first copied with Herdr's own
//! `layout.clone` (which keeps your sizes and shown/hidden panels) and the copy is shown. Herdr follows the
//! layout files as they change, so nothing restarts: not the window, not herdr-core, not a terminal.
mod files;
mod layout;

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::env::{Env, Run};
use crate::error::{Error, Result};
use layout::{Node, PANEL_MOD};

/// Herdr's app CLI, from PATH: it talks to the open window.
const HERDR_APP: &str = "herdr-app";

/// What `herdr-app status` says about the window, as far as docking needs.
#[derive(Debug, Deserialize)]
struct Window {
    layout: String,
}

/// Where the panel ended up.
#[derive(Debug, PartialEq, Eq)]
enum Docked {
    /// The shown layout already has it.
    Already(String),
    /// Added to the shown layout, one of your own.
    Added(String),
    /// Herdr's layout `from` was copied as `name`, which has the panel and is shown now.
    Copied { name: String, from: String },
    /// Herdr's window isn't open: the files are in place; where the panel is docked can't be checked.
    Closed,
}

/// `nudge panel install`: install the panel's files, then dock it in the open window. Safe to run again.
pub(crate) fn install(env: &Env, run: &dyn Run) -> Result<u8> {
    let home = app_home(env);
    let mods = home.join("mods").join(PANEL_MOD);
    let changed = files::install_mod(&mods)?;
    println!(
        "{} the Nudge panel in {}",
        if changed { "Installed" } else { "Already installed:" },
        mods.display()
    );
    let message = match dock(&home, run)? {
        Docked::Already(name) => format!("It's docked in your layout \"{name}\"."),
        Docked::Added(name) => format!("Docked it on the right of your layout \"{name}\"."),
        Docked::Copied { name, from } => format!(
            "Docked it on the right of \"{name}\", your copy of Herdr's \"{from}\" layout, which Herdr shows now."
        ),
        Docked::Closed => "Herdr's window isn't open, so nudge can't dock the panel or check that it is: run \
                           `nudge panel install` again while it's open."
            .to_owned(),
    };
    println!("{message}");
    Ok(0)
}

/// `$HERDR_APP_HOME`, else `~/.config/herdr-app`, as Herdr itself picks it.
fn app_home(env: &Env) -> PathBuf {
    env.get("HERDR_APP_HOME")
        .map_or_else(|| env.home().join(".config").join("herdr-app"), PathBuf::from)
}

/// Dock the panel in the layout the open window shows, copying it first when it's one of Herdr's own.
fn dock(home: &Path, run: &dyn Run) -> Result<Docked> {
    let Some(status) = run.run(&[HERDR_APP, "status"], Path::new("/")) else {
        return Ok(Docked::Closed);
    };
    let window: Window = serde_json::from_str(&status).map_err(|error| {
        Error::Other(format!(
            "`herdr-app status` answered something nudge can't read: {error}"
        ))
    })?;
    let name = window.layout;
    let layouts = home.join("layouts");
    let shown = read_layout(&layouts, &name)?;
    if shown.shows_panel() {
        return Ok(Docked::Already(name));
    }
    if shown.is_own() {
        add_panel(&layouts, &name, shown)?;
        return Ok(Docked::Added(name));
    }
    let copy = format!("{name}-nudge");
    if layouts.join(format!("{copy}.json")).exists() {
        herdr_app(
            run,
            &["do", "layout.switch", "--name", &copy],
            &format!("show the layout \"{copy}\""),
        )?;
    } else {
        let args = serde_json::json!({ "name": name, "as": copy }).to_string();
        herdr_app(
            run,
            &["do", "layout.clone", &args],
            &format!("copy the layout \"{name}\""),
        )?;
    }
    let copied = read_layout(&layouts, &copy)?;
    if !copied.shows_panel() {
        add_panel(&layouts, &copy, copied)?;
    }
    Ok(Docked::Copied { name: copy, from: name })
}

fn read_layout(layouts: &Path, name: &str) -> Result<Node> {
    let path = layouts.join(format!("{name}.json"));
    let text = fs::read_to_string(&path)
        .map_err(|error| Error::Other(format!("can't read Herdr's layout {}: {error}", path.display())))?;
    layout::parse(&text, name)
}

/// Dock the panel in `layout` and save it as `layouts/<name>.json`; Herdr shows the change at once.
fn add_panel(layouts: &Path, name: &str, mut layout: Node) -> Result<()> {
    layout.dock_panel()?;
    files::write_whole(&layouts.join(format!("{name}.json")), &layout::to_text(&layout)?)
}

/// Run a `herdr-app` command that has to work; `what` says what it was for when it doesn't.
fn herdr_app(run: &dyn Run, args: &[&str], what: &str) -> Result<()> {
    let mut cmd = vec![HERDR_APP];
    cmd.extend_from_slice(args);
    run.run(&cmd, Path::new("/"))
        .map(|_| ())
        .ok_or_else(|| Error::Other(format!("Herdr couldn't {what}")))
}

#[cfg(test)]
mod tests;
