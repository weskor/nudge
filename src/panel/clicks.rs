//! Herdr's click state for the panel's block: what the window saved in `layouts/.state/<name>.json` for the node
//! `nudge-panel`, a width (a divider drag saves one) or a hidden flag. Herdr keeps a saved width whatever the
//! window's width, so a wide one in a narrow window can leave the rest of the window no room.
//!
//! The window keeps the shown layout's click state in memory and saves it again on the next change, so for that
//! layout nudge asks the window (`layout.resize` back to the file's width, `layout.show`). Any other layout's state
//! is read from the file when it's shown, so nudge cuts the node out of the file, leaving every other byte.
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::edit;
use super::files::write_whole;
use super::layout::PANEL_ID;
use super::window;
use crate::env::Run;
use crate::error::Result;

/// What the window had saved for the panel and what came of putting it back: the width it had (when it wasn't the
/// file's) and that it's back, whether it was hidden and is shown again, and what the window refused, if anything.
/// Putting it back is a courtesy: a refusal is reported, never an error.
#[derive(Debug, Default, PartialEq)]
pub(super) struct Restored {
    pub(super) width: Option<f64>,
    pub(super) hidden: bool,
    pub(super) refused: Vec<String>,
}

/// A layout's click state, as far as the panel needs.
#[derive(Debug, Default, Deserialize)]
struct State {
    #[serde(default)]
    nodes: BTreeMap<String, Clicks>,
}

#[derive(Debug, Default, Deserialize)]
struct Clicks {
    size: Option<Size>,
    hidden: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
struct Size {
    px: Option<f64>,
}

fn path(layouts: &Path, name: &str) -> PathBuf {
    layouts.join(".state").join(format!("{name}.json"))
}

/// What the window saved for the panel in `name`; nothing when there's no state file or it can't be read.
fn saved(layouts: &Path, name: &str) -> Clicks {
    fs::read_to_string(path(layouts, name))
        .ok()
        .and_then(|text| serde_json::from_str::<State>(&text).ok())
        .and_then(|mut state| state.nodes.remove(PANEL_ID))
        .unwrap_or_default()
}

/// Put the panel in `name`, the layout the open window shows, back to what its file says: `px` wide (the block's
/// width in the file, when it has one), and shown.
pub(super) fn restore(run: &dyn Run, layouts: &Path, name: &str, px: Option<f64>) -> Restored {
    let clicks = saved(layouts, name);
    let mut restored = Restored::default();
    if let (Some(was), Some(px)) = (clicks.size.and_then(|size| size.px), px)
        && (was - px).abs() >= 0.5
    {
        let px = px.to_string();
        let args = ["do", "layout.resize", "--id", PANEL_ID, "--px", &px];
        let what = format!("set the panel's width in \"{name}\" back to {px} px (it had saved {was} px)");
        match window::herdr_app(run, &args, &what) {
            Ok(()) => restored.width = Some(was),
            Err(error) => restored.refused.push(error.to_string()),
        }
    }
    if clicks.hidden == Some(true) {
        let args = ["do", "layout.show", "--id", PANEL_ID];
        match window::herdr_app(run, &args, &format!("show the panel in \"{name}\" again")) {
            Ok(()) => restored.hidden = true,
            Err(error) => restored.refused.push(error.to_string()),
        }
    }
    restored
}

/// Cut the panel's node out of `name`'s click state file, if it has one. A file nudge can't read is Herdr's to
/// sort out, and is left as it is, as `saved` treats it as empty.
pub(super) fn forget(layouts: &Path, name: &str) -> Result<()> {
    let path = path(layouts, name);
    let Ok(text) = fs::read_to_string(&path) else {
        return Ok(());
    };
    if let Ok(Some(kept)) = edit::forget_node(&text) {
        write_whole(&path, &kept)?;
    }
    Ok(())
}
