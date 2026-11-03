//! Undoing `nudge panel install`'s docking: show `<name>` again when the window shows the `<name>-nudge` copy,
//! take the panel's block out of every layout of your own, and delete the copies install made, but only while
//! each is still just what Herdr's `layout.clone` wrote. Herdr's own layouts are never written. What the window
//! saved for the panel (a dragged width, hidden) goes too, from each of those layouts' click state. Works with the
//! window closed too: then a copy Herdr will open with is kept until the next run.
use std::fs;
use std::path::Path;

use super::window::{self, Window};
use super::{clicks, edit, record};
use crate::env::Run;
use crate::error::Result;

/// What undocking did.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Undocked {
    /// The window showed the copy (first) and now shows the layout it copies (second).
    pub(super) switched: Option<(String, String)>,
    /// Layouts of your own the panel's block was taken out of.
    pub(super) taken_out: Vec<String>,
    /// Copies install made, deleted.
    pub(super) deleted: Vec<String>,
    /// Copies install made, kept, and why.
    pub(super) kept: Vec<(String, Kept)>,
}

/// Why a copy install made stays.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Kept {
    /// It's changed since Herdr made it: it's yours now, and nudge forgets it made it.
    Changed,
    /// Herdr opens with it and its window isn't open to switch away: the next run deletes it.
    Shown,
    /// The window showed it, but `nudge panel install` didn't make it (no record), so it isn't nudge's to delete.
    NotMade,
}

/// Undock the panel; `home` is Herdr's app home, `record` nudge's `panel.json`.
pub(super) fn undock(home: &Path, record_path: &Path, window: Window<'_>) -> Result<Undocked> {
    let layouts = home.join("layouts");
    let mut out = Undocked::default();
    let shown = match window {
        Window::Open { run, layout } => match layout.strip_suffix("-nudge") {
            Some(base) if layouts.join(format!("{base}.json")).exists() => {
                let what = format!("show the layout \"{base}\"");
                window::herdr_app(run, &["do", "layout.switch", "--name", base], &what)?;
                out.switched = Some((layout.clone(), base.to_owned()));
                Some(base.to_owned())
            }
            _ => {
                restore_shown(run, &layouts, &layout)?;
                Some(layout)
            }
        },
        Window::Closed | Window::NoApp => window::opens_with(home),
    };
    for name in layout_names(&layouts) {
        let Ok((text, tree)) = window::read_layout(&layouts, &name) else {
            continue;
        };
        if tree.is_own()
            && let Some(undocked) = edit::undock(&text)?
        {
            window::write_layout(&layouts, &name, &undocked)?;
            out.taken_out.push(name);
        }
    }
    delete_made(&layouts, record_path, shown.as_deref(), &mut out)?;
    for name in out.taken_out.iter().chain(&out.deleted) {
        clicks::forget(&layouts, name)?;
    }
    Ok(out)
}

/// The window keeps the shown layout's click state in memory and would save the panel's again after the block is
/// gone, so first ask it to put the panel back to what the file says, which drops its entry.
fn restore_shown(run: &dyn Run, layouts: &Path, name: &str) -> Result<()> {
    let Ok((_, tree)) = window::read_layout(layouts, name) else {
        return Ok(());
    };
    if let Some(block) = tree.our_block().filter(|_| tree.is_own()) {
        clicks::restore(run, layouts, name, block.px())?;
    }
    Ok(())
}

/// Delete the copies install made that are still as Herdr made them and not the layout Herdr shows (or opens
/// with); say why each other one stays.
fn delete_made(layouts: &Path, record_path: &Path, shown: Option<&str>, out: &mut Undocked) -> Result<()> {
    let mut made = record::load(record_path);
    if let Some((copy, _)) = &out.switched
        && !made.made.contains_key(copy)
    {
        out.kept.push((copy.clone(), Kept::NotMade));
    }
    for (name, entry) in std::mem::take(&mut made.made) {
        let path = layouts.join(format!("{name}.json"));
        if !path.exists() {
            continue;
        }
        let unchanged = window::read_layout(layouts, &name)
            .and_then(|(_, tree)| tree.canonical())
            .is_ok_and(|now| now == entry.as_cloned);
        if !unchanged {
            out.kept.push((name, Kept::Changed));
        } else if shown == Some(name.as_str()) {
            out.kept.push((name.clone(), Kept::Shown));
            made.made.insert(name, entry);
        } else {
            fs::remove_file(&path)?;
            out.taken_out.retain(|taken| *taken != name);
            out.deleted.push(name);
        }
    }
    record::save(record_path, &made)
}

/// The layouts in `layouts/`, by name, sorted.
fn layout_names(layouts: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(layouts)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .filter_map(|file| file.strip_suffix(".json").map(str::to_owned))
        .filter(|name| !name.starts_with('.'))
        .collect();
    names.sort();
    names
}

#[cfg(test)]
mod tests;
