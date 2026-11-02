//! Docking the panel in the layout the open Herdr window shows. A layout of your own gets the panel's block; one
//! of Herdr's own is first copied with Herdr's `layout.clone` (which keeps your sizes and shown panels) as
//! `<name>-nudge`, recorded in `panel.json`, and shown. Herdr follows layout files as they change, so nothing
//! restarts.
use std::path::Path;

use super::layout::{self, Node};
use super::record::{self, Made};
use super::window::{self, Window};
use crate::env::Run;
use crate::error::Result;

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
    let mut current = window::read_layout(&layouts, &name)?;
    if current.shows_panel() {
        return Ok(Docked::Already(name));
    }
    if current.is_own() {
        current.dock_panel()?;
        window::write_layout(&layouts, &name, &current)?;
        let block = layout::to_line(&Node::panel_block())?;
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
    let mut copied = window::read_layout(&layouts, &copy)?;
    if !copied.shows_panel() {
        copied.dock_panel()?;
        window::write_layout(&layouts, &copy, &copied)?;
    }
    Ok(Docked::Copied { name: copy, from: name })
}

/// `layout.clone` Herdr's `name` as `copy` (Herdr shows the copy), and record the copy as Herdr wrote it.
fn clone(run: &dyn Run, layouts: &Path, record: &Path, name: &str, copy: &str) -> Result<()> {
    let args = serde_json::json!({ "name": name, "as": copy }).to_string();
    window::herdr_app(
        run,
        &["do", "layout.clone", &args],
        &format!("copy the layout \"{name}\""),
    )?;
    let as_cloned = window::read_layout(layouts, copy)?.canonical()?;
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
