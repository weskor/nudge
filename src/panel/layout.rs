//! Herdr's layout files (`layouts/<name>.json`) read as trees, as far as the panel needs: whether a layout shows
//! the panel, whether it's a layout of your own (made by `layout.clone`, so Herdr never replaces it), and a
//! canonical form to tell two layouts apart. Edits go to the text instead (`edit.rs`), so every other byte stays.
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::error::{Error, Result};

/// The mod folder the panel lives in.
pub(super) const PANEL_MOD: &str = "nudge";
/// The id of the block `nudge panel install` adds; `nudge panel remove` takes out only that block.
pub(super) const PANEL_ID: &str = "nudge-panel";

/// A layout node: a block, row, column or stack, with its children and floating blocks.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(super) struct Node {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    content: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    children: Option<Vec<Self>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    floating: Option<Vec<Self>>,
    /// `id`, `type`, `size`, the layout's look (on the root), `cloned_from`, and anything newer.
    #[serde(flatten)]
    rest: Map<String, Value>,
}

impl Node {
    fn is_panel(&self) -> bool {
        self.content
            .as_ref()
            .and_then(|content| content.get("mod"))
            .is_some_and(|name| name == PANEL_MOD)
    }

    /// The block install added: the panel, with install's id.
    pub(super) fn is_our_block(&self) -> bool {
        self.is_panel() && self.rest.get("id").is_some_and(|id| id == PANEL_ID)
    }

    fn nested(&self) -> impl Iterator<Item = &Self> {
        self.children.iter().chain(&self.floating).flatten()
    }

    /// Whether the panel is in this tree, docked or floating, whoever put it there.
    pub(super) fn shows_panel(&self) -> bool {
        self.is_panel() || self.nested().any(Self::shows_panel)
    }

    /// The block install added, wherever it is in this tree.
    pub(super) fn our_block(&self) -> Option<&Self> {
        if self.is_our_block() {
            return Some(self);
        }
        self.nested().find_map(Self::our_block)
    }

    /// The node's width in the file, when it's given in px.
    pub(super) fn px(&self) -> Option<f64> {
        self.rest.get("size")?.get("px")?.as_f64()
    }

    /// A layout of your own: `layout.clone` marks its copies with `cloned_from`. Herdr's own layouts (and
    /// hand-written ones) don't have it, so they're copied rather than edited.
    pub(super) fn is_own(&self) -> bool {
        self.rest.contains_key("cloned_from")
    }

    /// The tree as one canonical JSON line (keys sorted), to tell whether two layouts are the same.
    pub(super) fn canonical(&self) -> Result<String> {
        serde_json::to_value(self)
            .map(|value| value.to_string())
            .map_err(|error| Error::Other(error.to_string()))
    }
}

/// A layout file's text as a tree; `name` says which in errors.
pub(super) fn parse(text: &str, name: &str) -> Result<Node> {
    serde_json::from_str(text).map_err(|error| {
        Error::Other(format!(
            "Herdr's layout \"{name}\" isn't a layout nudge can read: {error}"
        ))
    })
}

#[cfg(test)]
mod tests;
