//! Adding and taking out the panel's block in a layout file's text, leaving every other byte as it was: a layout
//! of your own reads the same, byte for byte, after `nudge panel install` then `nudge panel remove`, however it's
//! formatted. The block goes in after the root row's last child, with the same gap before it.
use std::collections::BTreeMap;
use std::ops::Range;

use serde::Serialize;
use serde_json::value::RawValue;

use super::layout::{self, PANEL_ID, PANEL_MOD};
use crate::error::{Error, Result};

/// The panel's block, as Herdr docks a mod that asks for `"place": "right"`; keys in Herdr's order. Herdr saves a
/// width for it in the window's click state and keeps it whatever the window's width, and lays the block out within
/// `min` and `max` whatever it saved: at least 280 px, so every row of the panel fits; at most 360 px, so the rest of
/// a narrow window keeps room. 280 px wide by default: the narrowest it's whole at, leaving the terminal the most.
#[derive(Debug, Serialize)]
struct Block {
    id: &'static str,
    #[serde(rename = "type")]
    kind: &'static str,
    content: Content,
    size: Size,
    min: u32,
    max: u32,
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
    size: Size { px: 280 },
    min: 280,
    max: 360,
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
        // Right after `[`: whatever was between the brackets stays after the block, and remove cuts just the block.
        None => (span(text, children.get())?.start + 1, block_line()?),
    };
    let (head, tail) = text.split_at_checked(at).ok_or_else(lost)?;
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
        kept.push_str(text.get(from..cut.start).ok_or_else(lost)?);
        from = cut.end;
    }
    kept.push_str(text.get(from..).unwrap_or_default());
    Ok(Some(kept))
}

/// `text`, a window's click state (`layouts/.state/<name>.json`), without the panel's node (`nodes` →
/// `nudge-panel`) and the gap before it; `None` when it has none. Every other byte stays.
pub(super) fn forget_node(text: &str) -> Result<Option<String>> {
    let root: Fields<'_> = serde_json::from_str(text)?;
    let Some(nodes) = root.get("nodes") else {
        return Ok(None);
    };
    let (braces, members) = members(text, nodes.get())?;
    let Some(at) = members.iter().position(|member| member.key == PANEL_ID) else {
        return Ok(None);
    };
    let before = at.checked_sub(1).and_then(|index| members.get(index));
    let cut = match (before, members.get(at), members.get(at + 1)) {
        (Some(before), Some(ours), _) => before.value.end..ours.value.end,
        (None, Some(ours), Some(next)) => ours.key_start..next.key_start,
        (None, Some(_), None) => braces.start + 1..braces.end.saturating_sub(1),
        (_, None, _) => return Err(lost()),
    };
    let (head, tail) = text.get(..cut.start).zip(text.get(cut.end..)).ok_or_else(lost)?;
    Ok(Some(format!("{head}{tail}")))
}

/// `text` with `min` and `max` added to each block install made that lacks them (one docked by an older nudge),
/// after its last member and written like it; `None` when every such block has both, or there's none.
pub(super) fn bound(text: &str) -> Result<Option<String>> {
    let mut blocks = Vec::new();
    find_ours(text, &mut blocks)?;
    let mut inserts = Vec::new();
    for block in blocks {
        let (_, members) = members(text, block)?;
        let Some(last) = members.last() else {
            continue;
        };
        let before = text.get(..last.key_start).unwrap_or_default();
        let gap = before.strip_prefix(before.trim_end()).unwrap_or_default();
        let key_and_colon = text.get(last.key_start..last.value.start).unwrap_or_default();
        let colon = key_and_colon
            .rfind(':')
            .and_then(|at| key_and_colon.get(at..))
            .unwrap_or(":");
        let missing = [("min", BLOCK.min), ("max", BLOCK.max)]
            .into_iter()
            .filter(|(key, _)| !members.iter().any(|member| member.key == *key))
            .map(|(key, value)| format!(",{gap}\"{key}\"{colon}{value}"))
            .collect::<Vec<_>>()
            .concat();
        if !missing.is_empty() {
            inserts.push((last.value.end, missing));
        }
    }
    if inserts.is_empty() {
        return Ok(None);
    }
    inserts.sort_by_key(|(at, _)| *at);
    let mut bounded = String::with_capacity(text.len() + inserts.len() * 32);
    let mut from = 0;
    for (at, insert) in inserts {
        bounded.push_str(text.get(from..at).ok_or_else(lost)?);
        bounded.push_str(&insert);
        from = at;
    }
    bounded.push_str(text.get(from..).unwrap_or_default());
    Ok(Some(bounded))
}

/// The blocks install added in `node` (a slice of the file's text) and below it.
fn find_ours<'a>(node: &'a str, found: &mut Vec<&'a str>) -> Result<()> {
    let fields: Fields<'a> = serde_json::from_str(node)?;
    for key in ["children", "floating"] {
        let Some(&list) = fields.get(key).filter(|list| list.get() != "null") else {
            continue;
        };
        for entry in items(list)? {
            if layout::parse(entry.get(), key)?.is_our_block() {
                found.push(entry.get());
            } else {
                find_ours(entry.get(), found)?;
            }
        }
    }
    Ok(())
}

/// A member of a JSON object in the text: its key, where the key's opening quote is, and where its value is.
#[derive(Debug)]
struct Member {
    key: String,
    key_start: usize,
    value: Range<usize>,
}

/// Where `object` (a slice of `text`) is, and its members in the order they're written.
fn members(text: &str, object: &str) -> Result<(Range<usize>, Vec<Member>)> {
    let braces = span(text, object)?;
    let fields: Fields<'_> = serde_json::from_str(object)?;
    let mut values = fields
        .into_iter()
        .map(|(key, value)| Ok((key, span(text, value.get())?)))
        .collect::<Result<Vec<_>>>()?;
    values.sort_by_key(|(_, value)| value.start);
    let mut members = Vec::with_capacity(values.len());
    let mut from = braces.start + 1;
    for (key, value) in values {
        // The key's opening quote is the first `"` after the member before it, or after the `{`.
        let key_start = text.get(from..).and_then(|rest| rest.find('"')).ok_or_else(lost)? + from;
        from = value.end;
        members.push(Member { key, key_start, value });
    }
    Ok((braces, members))
}

fn lost() -> Error {
    Error::Other("nudge lost its place in the file's text".into())
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
        cuts_in_list(text, &entries, &ours, cuts)?;
        for (entry, _) in entries.iter().zip(&ours).filter(|(_, ours)| !**ours) {
            collect_cuts(text, entry.get(), cuts)?;
        }
    }
    Ok(())
}

/// The cuts that take the entries marked `ours` out of a list: a leading run up to the first kept entry, each
/// later one from the end of the entry before it (so its comma and gap go too); when all are ours, the entries and
/// what's between them, leaving the whitespace inside the brackets as it was.
fn cuts_in_list(text: &str, entries: &[&RawValue], ours: &[bool], cuts: &mut Vec<Range<usize>>) -> Result<()> {
    let spans = entries
        .iter()
        .map(|entry| span(text, entry.get()))
        .collect::<Result<Vec<_>>>()?;
    let Some(first_kept) = ours.iter().position(|ours| !ours) else {
        if let (Some(first), Some(last)) = (spans.first(), spans.last()) {
            cuts.push(first.start..last.end);
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
        .ok_or_else(lost)
}

#[cfg(test)]
mod tests;
