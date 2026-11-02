//! Herdr's layout files (`layouts/<name>.json`), as far as docking the panel needs: whether a layout shows the
//! panel, whether it's a layout of your own (made by `layout.clone`, so Herdr never replaces it), and adding the
//! panel on the right. Every other field is kept as read.
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::error::{Error, Result};

/// The mod folder the panel lives in.
pub(super) const PANEL_MOD: &str = "nudge";

/// A layout node: a block, row, column or stack, with its children and floating blocks.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(super) struct Node {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    content: Option<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    children: Vec<Self>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    floating: Vec<Self>,
    /// `id`, `type`, `size`, the layout's look (on the root), `cloned_from`, and anything newer.
    #[serde(flatten)]
    rest: Map<String, Value>,
}

impl Node {
    /// Whether the panel is in this tree, docked or floating.
    pub(super) fn shows_panel(&self) -> bool {
        let here = self
            .content
            .as_ref()
            .and_then(|content| content.get("mod"))
            .is_some_and(|name| name == PANEL_MOD);
        here || self.children.iter().chain(&self.floating).any(Self::shows_panel)
    }

    /// A layout of your own: `layout.clone` marks its copies with `cloned_from`. Herdr's own layouts (and
    /// hand-written ones) don't have it, so they're copied rather than edited.
    pub(super) fn is_own(&self) -> bool {
        self.rest.contains_key("cloned_from")
    }

    /// Add the panel as the last child of the root row, 320 px wide: where Herdr docks a mod that asks for
    /// `"place": "right"`.
    pub(super) fn dock_panel(&mut self) -> Result<()> {
        if self.rest.get("type").and_then(Value::as_str) != Some("row") {
            return Err(Error::Other(
                "this layout isn't a row at its top, so the panel can't dock on its right".into(),
            ));
        }
        let mut rest = Map::new();
        rest.insert("id".into(), json!("nudge-panel"));
        rest.insert("type".into(), json!("block"));
        rest.insert("size".into(), json!({ "px": 320 }));
        self.children.push(Self {
            content: Some(json!({ "mod": PANEL_MOD })),
            children: Vec::new(),
            floating: Vec::new(),
            rest,
        });
        Ok(())
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

#[cfg(test)]
mod tests;
