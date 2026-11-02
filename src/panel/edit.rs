//! Adding and taking out the panel's block in a layout file's text, leaving every other byte as it was: a layout
//! of your own reads the same, byte for byte, after `nudge panel install` then `nudge panel remove`, however it's
//! formatted. The block goes in after the root row's last child, with the same gap before it.
use std::collections::BTreeMap;
use std::ops::Range;

use serde::Serialize;
use serde_json::value::RawValue;

use super::layout::{self, PANEL_ID, PANEL_MOD};
use crate::error::{Error, Result};

/// The panel's block, 320 px wide as Herdr docks a mod that asks for `"place": "right"`; keys in Herdr's order.
#[derive(Debug, Serialize)]
struct Block {
    id: &'static str,
    #[serde(rename = "type")]
    kind: &'static str,
    content: Content,
    size: Size,
}

#[derive(Debug, Serialize)]
struct Content {
    #[serde(rename = "mod")]
    name: &'static str,
}

#[derive(Debug, Serialize)]
struct Size {
    px: u32,
}

const BLOCK: Block = Block {
    id: PANEL_ID,
    kind: "block",
    content: Content { name: PANEL_MOD },
    size: Size { px: 320 },
};

/// A JSON object's fields, each value still a slice of the file's text.
type Fields<'a> = BTreeMap<String, &'a RawValue>;

/// The block as one line of JSON, as install shows it.
pub(super) fn block_line() -> Result<String> {
    Ok(serde_json::to_string(&BLOCK)?)
}

/// `text` with the panel's block added as the last child of its root row.
pub(super) fn dock(text: &str) -> Result<String> {
    let root: Fields<'_> = serde_json::from_str(text)?;
    let kind = root
        .get("type")
        .and_then(|kind| serde_json::from_str::<String>(kind.get()).ok());
    let Some(children) = root.get("children").filter(|_| kind.as_deref() == Some("row")) else {
        return Err(Error::Other(
            "this layout isn't a row with children at its top, so the panel can't dock on its right".into(),
        ));
    };
    let (at, insert) = match items(children)?.last() {
        Some(entry) => {
            let after = span(text, entry.get())?;
            let before = text.get(..after.start).unwrap_or_default();
            let gap = before.strip_prefix(before.trim_end()).unwrap_or_default();
            (after.end, format!(",{gap}{}", block_text(gap)?))
        }
        None => (span(text, children.get())?.start + 1, block_line()?),
    };
    let (head, tail) = text
        .split_at_checked(at)
        .ok_or_else(|| Error::Other("nudge lost its place in the layout's text".into()))?;
    Ok(format!("{head}{insert}{tail}"))
}

/// The block indented like a child that follows `gap`: on one line when the children share one.
fn block_text(gap: &str) -> Result<String> {
    let Some((_, indent)) = gap.rsplit_once('\n') else {
        return block_line();
    };
    let pretty = serde_json::to_string_pretty(&BLOCK)?;
    Ok(pretty.replace('\n', &format!("\n{indent}")))
}

/// `text` without the blocks install added (found by their id), wherever they are, each with the gap before it;
/// `None` when it has none.
pub(super) fn undock(text: &str) -> Result<Option<String>> {
    let mut cuts = Vec::new();
    collect_cuts(text, text, &mut cuts)?;
    if cuts.is_empty() {
        return Ok(None);
    }
    cuts.sort_by_key(|cut| cut.start);
    let mut kept = String::with_capacity(text.len());
    let mut from = 0;
    for cut in cuts {
        let between = text.get(from..cut.start);
        kept.push_str(between.ok_or_else(|| Error::Other("nudge lost its place in the layout's text".into()))?);
        from = cut.end;
    }
    kept.push_str(text.get(from..).unwrap_or_default());
    Ok(Some(kept))
}

/// Add the cuts for the panel's blocks in `node` (a slice of `text`) and below it.
fn collect_cuts(text: &str, node: &str, cuts: &mut Vec<Range<usize>>) -> Result<()> {
    let fields: Fields<'_> = serde_json::from_str(node)?;
    for key in ["children", "floating"] {
        let Some(list) = fields.get(key).filter(|list| list.get() != "null") else {
            continue;
        };
        let entries = items(list)?;
        let ours = entries
            .iter()
            .map(|entry| Ok(layout::parse(entry.get(), key)?.is_our_block()))
            .collect::<Result<Vec<bool>>>()?;
        cuts_in_list(text, list, &entries, &ours, cuts)?;
        for (entry, _) in entries.iter().zip(&ours).filter(|(_, ours)| !**ours) {
            collect_cuts(text, entry.get(), cuts)?;
        }
    }
    Ok(())
}

/// The cuts that take the entries marked `ours` out of `list`: a leading run up to the first kept entry, each
/// later one from the end of the entry before it (so its comma and gap go too), all of them by emptying the list.
fn cuts_in_list(
    text: &str,
    list: &RawValue,
    entries: &[&RawValue],
    ours: &[bool],
    cuts: &mut Vec<Range<usize>>,
) -> Result<()> {
    let spans = entries
        .iter()
        .map(|entry| span(text, entry.get()))
        .collect::<Result<Vec<_>>>()?;
    let Some(first_kept) = ours.iter().position(|ours| !ours) else {
        if !spans.is_empty() {
            let brackets = span(text, list.get())?;
            cuts.push(brackets.start + 1..brackets.end.saturating_sub(1));
        }
        return Ok(());
    };
    if let (Some(first), Some(kept)) = (spans.first(), spans.get(first_kept))
        && first_kept > 0
    {
        cuts.push(first.start..kept.start);
    }
    for (pair, _) in spans
        .windows(2)
        .zip(ours.iter().skip(1))
        .skip(first_kept)
        .filter(|(_, ours)| **ours)
    {
        if let [before, this] = pair {
            cuts.push(before.end..this.end);
        }
    }
    Ok(())
}

/// A list's entries, each still a slice of the file's text.
fn items(list: &RawValue) -> Result<Vec<&RawValue>> {
    Ok(serde_json::from_str(list.get())?)
}

/// Where `part`, a slice of `text`, sits in it.
fn span(text: &str, part: &str) -> Result<Range<usize>> {
    let start = part.as_ptr().addr().checked_sub(text.as_ptr().addr());
    let range = start.and_then(|start| Some(start..start.checked_add(part.len())?));
    range
        .filter(|range| {
            text.get(range.clone())
                .is_some_and(|found| found.as_ptr() == part.as_ptr())
        })
        .ok_or_else(|| Error::Other("nudge lost its place in the layout's text".into()))
}

#[cfg(test)]
mod tests;
