//! The panel's files, built into nudge so installing it needs nothing but this binary, and writing them (and
//! layout files) whole: to a temp file beside the target, then renamed over it.
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
