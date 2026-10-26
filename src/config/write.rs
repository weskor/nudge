//! Writing the config file atomically and durably, readable only by its owner.
use std::fs::{self, DirBuilder, OpenOptions};
use std::io::{ErrorKind, Write as _};
use std::os::unix::fs::{DirBuilderExt as _, OpenOptionsExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};

use super::model::Config;
use super::parse::parse_config;
use super::serialize::serialize_config;
use crate::error::Result;

/// Atomically and durably write the config with mode 600 (a new directory gets mode 700). A symlinked config is
/// written through the link (the link stays), and nothing is left behind when the write fails.
pub(crate) fn write_config(path: &Path, config: &Config) -> Result<()> {
    let text = serialize_config(config);
    parse_config(&text, &path.to_string_lossy())?; // never write a file we can't read back
    let path = fs::canonicalize(path).unwrap_or_else(|_| path.to_owned());
    if let Some(dir) = path.parent() {
        DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
    }
    let temp = PathBuf::from(format!("{}.{}.tmp", path.display(), std::process::id()));
    // A temp file left by a crashed run with the same pid (or a link planted there) is replaced, never followed.
    match fs::remove_file(&temp) {
        Err(error) if error.kind() != ErrorKind::NotFound => return Err(error.into()),
        Ok(()) | Err(_) => {}
    }
    let written = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temp)
        .and_then(|mut file| {
            file.write_all(text.as_bytes())?;
            file.set_permissions(fs::Permissions::from_mode(0o600))?;
            // On disk before it replaces the old config, so a crash can't leave an empty file holding the secrets.
            file.sync_all()
        })
        .and_then(|()| fs::rename(&temp, &path));
    if written.is_err() {
        fs::remove_file(&temp).ok();
    }
    Ok(written?)
}

#[cfg(test)]
mod tests;
