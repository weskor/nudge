//! Herdr's layout files (`layouts/<name>.json`), as far as the panel needs: whether a layout shows the panel,
//! whether it's a layout of your own (made by `layout.clone`, so Herdr never replaces it), and adding or taking
//! out the panel's block on the right. Every other field is kept as read.
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::error::{Error, Result};

/// The mod folder the panel lives in.
pub(super) const PANEL_MOD: &str = "nudge";
/// The id of the block `nudge panel install` adds; `nudge panel remove` takes out only that block.
const PANEL_ID: &str = "nudge-panel";

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
    /// The block install adds: the panel, 320 px wide, as Herdr docks a mod that asks for `"place": "right"`.
    pub(super) fn panel_block() -> Self {
        let mut rest = Map::new();
        rest.insert("id".into(), json!(PANEL_ID));
        rest.insert("type".into(), json!("block"));
        rest.insert("size".into(), json!({ "px": 320 }));
        Self {
            content: Some(json!({ "mod": PANEL_MOD })),
            children: None,
            floating: None,
            rest,
        }
    }

    fn is_panel(&self) -> bool {
        self.content
            .as_ref()
            .and_then(|content| content.get("mod"))
            .is_some_and(|name| name == PANEL_MOD)
    }

    fn is_our_block(&self) -> bool {
        self.is_panel() && self.rest.get("id").is_some_and(|id| id == PANEL_ID)
    }

    fn nested(&self) -> impl Iterator<Item = &Self> {
        self.children.iter().chain(&self.floating).flatten()
    }

    /// Whether the panel is in this tree, docked or floating, whoever put it there.
    pub(super) fn shows_panel(&self) -> bool {
        self.is_panel() || self.nested().any(Self::shows_panel)
    }

    /// A layout of your own: `layout.clone` marks its copies with `cloned_from`. Herdr's own layouts (and
    /// hand-written ones) don't have it, so they're copied rather than edited.
    pub(super) fn is_own(&self) -> bool {
        self.rest.contains_key("cloned_from")
    }

    /// Add the panel's block as the last child of the root row.
    pub(super) fn dock_panel(&mut self) -> Result<()> {
        if self.rest.get("type").and_then(Value::as_str) != Some("row") {
            return Err(Error::Other(
                "this layout isn't a row at its top, so the panel can't dock on its right".into(),
            ));
        }
        self.children.get_or_insert_with(Vec::new).push(Self::panel_block());
        Ok(())
    }

    /// Take out every block install added (by its id), anywhere in the tree; a panel put there some other way
    /// stays. Returns whether there was one.
    pub(super) fn undock_panel(&mut self) -> bool {
        let mut removed = false;
        for list in [&mut self.children, &mut self.floating].into_iter().flatten() {
            let before = list.len();
            list.retain(|node| !node.is_our_block());
            removed |= list.len() != before;
            for node in list.iter_mut() {
                removed |= node.undock_panel();
            }
        }
        removed
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

/// A tree as layout file text.
pub(super) fn to_text(node: &Node) -> Result<String> {
    serde_json::to_string_pretty(node).map_err(|error| Error::Other(error.to_string()))
}

/// A tree as one line of JSON, to show what was added.
pub(super) fn to_line(node: &Node) -> Result<String> {
    serde_json::to_string(node).map_err(|error| Error::Other(error.to_string()))
}

#[cfg(test)]
mod tests;
