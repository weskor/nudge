//! Locating and reading the config file, rewriting an old-format file in the new format on the way.
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use super::migrate::rename_old_keys;
use super::model::Config;
use super::parse::{parse_toml, validate_config};
use super::write::write_config;
use crate::env::Env;
use crate::error::{Error, Result};

/// `$NUDGE_CONFIG`, else `$XDG_CONFIG_HOME/nudge/config.toml`, else `~/.config/nudge/config.toml`.
pub(crate) fn default_config_path(env: &Env) -> PathBuf {
    if let Some(path) = env.get("NUDGE_CONFIG") {
        return PathBuf::from(path);
    }
    let base = env
        .get("XDG_CONFIG_HOME")
        .map_or_else(|| env.home().join(".config"), PathBuf::from);
    base.join("nudge").join("config.toml")
}

/// Load the config, or `None` if the file does not exist. A config in the old format is rewritten in the new one
/// first, with a note on stderr.
pub(crate) fn read_config(path: &Path) -> Result<Option<Config>> {
    let found = read_and_migrate(path)?;
    if let Some((_, Some(note))) = &found {
        eprintln!("nudge: {note}");
    }
    Ok(found.map(|(config, _)| config))
}

/// The config at `path`, if any. A file that uses only the old keys is rewritten with the new ones (keeping every
/// other setting), and the note says so.
pub(super) fn read_and_migrate(path: &Path) -> Result<Option<(Config, Option<String>)>> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(Error::Other(format!("{}: {error}", path.display()))),
    };
    let source = path.to_string_lossy();
    let mut raw = parse_toml(&text, &source)?;
    let old_format = rename_old_keys(&mut raw);
    let config = validate_config(&raw, &source)?;
    if !old_format {
        return Ok(Some((config, None)));
    }
    write_config(path, &config)?;
    let note = format!(
        "updated {} to the new format ([[webhook]], default_webhook)",
        path.display()
    );
    Ok(Some((config, Some(note))))
}

pub(crate) fn load_config(path: &Path) -> Result<Config> {
    read_config(path)?.ok_or_else(|| {
        Error::Config(format!(
            "no config at {}. Run \"nudge setup\" to add a webhook.",
            path.display()
        ))
    })
}

#[cfg(test)]
mod tests;
