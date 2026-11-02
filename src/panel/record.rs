//! `panel.json` in nudge's state dir: the layouts `nudge panel install` made with `layout.clone`, each with the
//! copy as Herdr wrote it, so `nudge panel remove` deletes a copy only while it's still just that (plus the panel).
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::files::write_whole;
use crate::error::{Error, Result};

/// The layouts install made, by name.
#[derive(Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Record {
    #[serde(default)]
    pub(super) made: BTreeMap<String, Made>,
}

/// One layout install made.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Made {
    /// The layout it copies.
    pub(super) from: String,
    /// The copy as Herdr's `layout.clone` wrote it, before the panel was added (`Node::canonical`).
    pub(super) as_cloned: String,
}

/// The record at `path`; none yet, or one nudge can't read, is empty (remove then deletes nothing).
pub(super) fn load(path: &Path) -> Record {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Save `record` at `path`, or delete the file when it's empty.
pub(super) fn save(path: &Path, record: &Record) -> Result<()> {
    if record.made.is_empty() {
        return match fs::remove_file(path) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.into()),
            Ok(()) | Err(_) => Ok(()),
        };
    }
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let text = serde_json::to_string_pretty(record).map_err(|error| Error::Other(error.to_string()))?;
    write_whole(path, &text)
}
