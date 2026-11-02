//! The panel's files, built into nudge so installing it needs nothing but this binary; taking them out again; and
//! writing files (the mod's, layouts) whole: to a hidden temp file beside the target, then renamed over it.
use std::fs::{self, OpenOptions};
use std::io::Write as _;
use std::path::Path;

use crate::error::Result;

/// The mod as Herdr loads it: `mods/nudge/<name>` and its text. `RECIPE.md`, `keys.json` and `preview.png` are
/// for people and herdr.dev, not for Herdr.
const MOD_FILES: [(&str, &str); 5] = [
    ("mod.json", include_str!("../../herdr-app/mods/nudge/mod.json")),
    ("main.js", include_str!("../../herdr-app/mods/nudge/main.js")),
    ("rows.js", include_str!("../../herdr-app/mods/nudge/rows.js")),
    ("popup.js", include_str!("../../herdr-app/mods/nudge/popup.js")),
    (
        "popup-view.js",
        include_str!("../../herdr-app/mods/nudge/popup-view.js"),
    ),
];

/// Write the panel's files into `dir` (created if missing), leaving files that already match alone, so a
/// running Herdr only reloads the panel when it changed. Returns whether any file was written.
pub(super) fn install_mod(dir: &Path) -> Result<bool> {
    fs::create_dir_all(dir)?;
    let mut changed = false;
    for (name, text) in MOD_FILES {
        let path = dir.join(name);
        if fs::read_to_string(&path).is_ok_and(|current| current == text) {
            continue;
        }
        write_whole(&path, text)?;
        changed = true;
    }
    Ok(changed)
}

/// Files Herdr itself writes beside a mod (`herdr-app check`, the editor types); they go with the panel.
const HERDR_WRITES: [&str; 3] = ["jsconfig.json", "gpui-kit.d.ts", "herdr.d.ts"];

/// What taking the panel's files out of its folder did.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Removed {
    /// The folder had no panel files.
    Nothing,
    /// The panel's files and the folder are gone.
    Folder,
    /// The panel's files are gone; the folder stays because it holds these other files.
    Kept(Vec<String>),
}

/// Delete the panel's files (and the ones Herdr wrote beside them) from `dir`, then `dir` itself if that left
/// it empty. Anything else in it is left alone.
pub(super) fn remove_mod(dir: &Path) -> Result<Removed> {
    let mut any = false;
    for name in MOD_FILES.iter().map(|(name, _)| *name).chain(HERDR_WRITES) {
        match fs::remove_file(dir.join(name)) {
            Ok(()) => any |= MOD_FILES.iter().any(|(ours, _)| *ours == name),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return Ok(if any { Removed::Folder } else { Removed::Nothing });
    };
    let mut left: Vec<String> = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.file_name().to_string_lossy().into_owned()))
        .collect();
    left.sort();
    if !left.is_empty() {
        return Ok(if any { Removed::Kept(left) } else { Removed::Nothing });
    }
    fs::remove_dir(dir)?;
    Ok(if any { Removed::Folder } else { Removed::Nothing })
}

/// Replace `path` with `text` in one step, so Herdr never reads half a file. The temp file is hidden, which
/// Herdr's folder watch ignores.
pub(super) fn write_whole(path: &Path, text: &str) -> Result<()> {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let temp = path.with_file_name(format!(".{name}.nudge-{}.tmp", std::process::id()));
    let mut file = OpenOptions::new().write(true).create(true).truncate(true).open(&temp)?;
    file.write_all(text.as_bytes())?;
    file.sync_all()?;
    fs::rename(&temp, path)?;
    Ok(())
}
