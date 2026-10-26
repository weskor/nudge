//! Where this copy of nudge lives: the checkout or plugin install with herdr-plugin.toml.
use std::path::PathBuf;

use crate::env::Env;

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
