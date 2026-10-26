//! The `nudge` command on PATH: what `~/.local/bin/nudge` is, linking it, and repointing it after plugin updates.
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use crate::env::Env;
use crate::error::{Error, Result};

pub(crate) fn command_link_path(env: &Env) -> PathBuf {
    env.home().join(".local").join("bin").join("nudge")
}

/// What `~/.local/bin/nudge` currently is.
#[derive(Debug)]
pub(crate) struct CommandLink {
    pub(crate) path: PathBuf,
    pub(crate) exists: bool,
    /// The symlink's target; `None` if it's missing or not a symlink.
    pub(crate) target: Option<PathBuf>,
}

pub(crate) fn command_link(env: &Env) -> CommandLink {
    let path = command_link_path(env);
    match fs::symlink_metadata(&path) {
        Ok(meta) => {
            let target = if meta.is_symlink() {
                fs::read_link(&path).ok()
            } else {
                None
            };
            CommandLink {
                path,
                exists: true,
                target,
            }
        }
        Err(_) => CommandLink {
            path,
            exists: false,
            target: None,
        },
    }
}

/// Point `~/.local/bin/nudge` at `<root>/bin/nudge`. Refuses to replace a regular file.
pub(crate) fn link_command(root: &Path, env: &Env) -> Result<PathBuf> {
    let link = command_link(env);
    if link.exists && link.target.is_none() {
        return Err(Error::Other(format!(
            "{} exists and is not a symlink; remove it first",
            link.path.display()
        )));
    }
    if let Some(dir) = link.path.parent() {
        fs::create_dir_all(dir)?;
    }
    if link.exists {
        fs::remove_file(&link.path)?;
    }
    symlink(root.join("bin").join("nudge"), &link.path)?;
    Ok(link.path)
}

/// herdr installs each plugin update into a new directory next to the old one (and later deletes the old one). If
/// the user linked `~/.local/bin/nudge` into an older nudge install, repoint it at this one. Links anywhere else
/// (`~/.cargo/bin/nudge`, a development checkout) and regular files are left alone.
pub(crate) fn refresh_command_link(root: &Path, env: &Env) -> Result<bool> {
    let wanted = root.join("bin").join("nudge");
    let older_install = |target: &PathBuf| {
        // `<install>/bin/nudge`, where `<install>` sits in the same directory as this install.
        let install = target
            .parent()
            .and_then(Path::parent)
            .filter(|_| target.ends_with("bin/nudge"));
        install
            .and_then(Path::parent)
            .is_some_and(|parent| Some(parent) == root.parent())
    };
    let stale = command_link(env)
        .target
        .is_some_and(|target| target != wanted && older_install(&target));
    if stale {
        let _path = link_command(root, env)?;
    }
    Ok(stale)
}

#[cfg(test)]
mod tests;
