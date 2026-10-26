//! Where this copy of nudge lives, its herdr plugin registration, and the `nudge` command on PATH.
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::detect::herdr_json;
use crate::env::{Env, Run};
use crate::error::{Error, Result};

pub(crate) const PLUGIN_ID: &str = "nudge";

/// Root of this nudge checkout or plugin install (the directory with herdr-plugin.toml): `$NUDGE_ROOT` (set by
/// `bin/nudge`), else two levels above the binary (`<root>/dist/nudge`).
pub(crate) fn nudge_root(env: &Env) -> PathBuf {
    if let Some(root) = env.get("NUDGE_ROOT") {
        return PathBuf::from(root);
    }
    std::env::current_exe()
        .ok()
        .and_then(|exe| Some(exe.parent()?.parent()?.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub(crate) struct PluginInfo {
    pub(crate) enabled: bool,
    pub(crate) plugin_root: String,
    pub(crate) version: String,
}

#[derive(Debug, Deserialize)]
struct PluginList {
    result: PluginListResult,
}

#[derive(Debug, Deserialize)]
struct PluginListResult {
    plugins: Vec<PluginInfo>,
}

/// The herdr plugin as herdr sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Plugin {
    /// herdr isn't installed or its server isn't running.
    HerdrUnavailable,
    NotInstalled,
    Installed(PluginInfo),
}

pub(crate) fn plugin(env: &Env, run: &dyn Run) -> Plugin {
    let args = ["plugin", "list", "--plugin", PLUGIN_ID, "--json"];
    match herdr_json::<PluginList>(&args, env, Path::new("/"), run) {
        None => Plugin::HerdrUnavailable,
        Some(list) => list
            .result
            .plugins
            .into_iter()
            .next()
            .map_or(Plugin::NotInstalled, Plugin::Installed),
    }
}

/// One plugin hook or action run from `herdr plugin log list`.
#[derive(Clone, Debug, Deserialize)]
pub(crate) struct PluginLog {
    pub(crate) started_unix_ms: i64,
    pub(crate) status: String,
    #[serde(default)]
    pub(crate) stderr: String,
}

#[derive(Debug, Deserialize)]
struct LogList {
    result: LogListResult,
}

#[derive(Debug, Deserialize)]
struct LogListResult {
    logs: Vec<PluginLog>,
}

/// Most recent plugin hook/action runs, newest last.
pub(crate) fn plugin_logs(env: &Env, run: &dyn Run) -> Vec<PluginLog> {
    let args = ["plugin", "log", "list", "--plugin", PLUGIN_ID];
    let mut logs = herdr_json::<LogList>(&args, env, Path::new("/"), run)
        .map(|list| list.result.logs)
        .unwrap_or_default();
    logs.sort_by_key(|log| log.started_unix_ms);
    logs
}

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
