//! Docking the panel in the layout the open Herdr window shows. A layout of your own gets the panel's block; one
//! of Herdr's own is first copied with Herdr's `layout.clone` (which keeps your sizes and shown panels) as
//! `<name>-nudge`, recorded in `panel.json`, and shown. Herdr follows layout files as they change, so nothing
//! restarts.
use std::path::Path;

use super::edit;
use super::record::{self, Made};
use super::window::{self, Window};
use crate::env::Run;
use crate::error::{Error, Result};

/// Where the panel ended up.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Docked {
    /// The shown layout already has it.
    Already(String),
    /// Added to the shown layout, one of your own: the block, as one line of JSON.
    Added { name: String, block: String },
    /// Herdr's layout `from` was copied as `name`, which has the panel and is shown now.
    Copied { name: String, from: String },
    /// Herdr's window isn't open: where the panel is docked can't be checked.
    Closed,
    /// `herdr-app` isn't on PATH, so the window can't be asked.
    NoApp,
}

/// Dock the panel in the window's layout; `home` is Herdr's app home, `record` nudge's `panel.json`.
pub(super) fn dock(home: &Path, record: &Path, window: Window<'_>) -> Result<Docked> {
    let (run, name) = match window {
        Window::NoApp => return Ok(Docked::NoApp),
        Window::Closed => return Ok(Docked::Closed),
        Window::Open { run, layout } => (run, layout),
    };
    let layouts = home.join("layouts");
    let (text, current) = window::read_layout(&layouts, &name).map_err(|error| refused(&layouts, &name, &error))?;
    if current.shows_panel() {
        return Ok(Docked::Already(name));
    }
    if current.is_own() {
        let docked = edit::dock(&text).map_err(|error| refused(&layouts, &name, &error))?;
        window::write_layout(&layouts, &name, &docked)?;
        let block = edit::block_line()?;
        return Ok(Docked::Added { name, block });
    }
    let copy = format!("{name}-nudge");
    if layouts.join(format!("{copy}.json")).exists() {
        window::herdr_app(
            run,
            &["do", "layout.switch", "--name", &copy],
            &format!("show the layout \"{copy}\""),
        )?;
    } else {
        clone(run, &layouts, record, &name, &copy)?;
    }
    let (text, copied) = window::read_layout(&layouts, &copy).map_err(|error| refused(&layouts, &copy, &error))?;
    if !copied.shows_panel() {
        let docked = edit::dock(&text).map_err(|error| refused(&layouts, &copy, &error))?;
        window::write_layout(&layouts, &copy, &docked)?;
    }
    Ok(Docked::Copied { name: copy, from: name })
}

/// Install can't put the block in `name`, so it writes nothing: say why, what to add by hand and where, and how to
/// show the panel without docking it.
fn refused(layouts: &Path, name: &str, problem: &Error) -> Error {
    let block = edit::block_line().unwrap_or_default();
    Error::Other(format!(
        "{problem}\nTo dock the panel by hand, add this block as the last child of the top row in {} (Herdr's own \
         layouts take no new panels: clone one first):\n  {block}\nOr open it over the window: herdr-app do \
         mod.open --name nudge",
        layouts.join(format!("{name}.json")).display()
    ))
}

/// `layout.clone` Herdr's `name` as `copy` (Herdr shows the copy), and record the copy as Herdr wrote it.
fn clone(run: &dyn Run, layouts: &Path, record: &Path, name: &str, copy: &str) -> Result<()> {
    let args = serde_json::json!({ "name": name, "as": copy }).to_string();
    window::herdr_app(
        run,
        &["do", "layout.clone", &args],
        &format!("copy the layout \"{name}\""),
    )?;
    let as_cloned = window::read_layout(layouts, copy)?.1.canonical()?;
    let mut made = record::load(record);
    made.made.insert(
        copy.to_owned(),
        Made {
            from: name.to_owned(),
            as_cloned,
        },
    );
    record::save(record, &made)
}

#[cfg(test)]
mod tests;
