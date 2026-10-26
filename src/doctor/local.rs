//! Checks on this machine: the `nudge` command on PATH and the clipboard tools setup uses.
use super::check::{Check, Level};
use crate::clipboard::Desktop;
use crate::env::Env;
use crate::herdr::command_link;

pub(super) fn check_command(env: &Env) -> Check {
    if let Some(path) = env.which("nudge") {
        return Check::new(Level::Ok, "nudge command on PATH").detail(path.display().to_string());
    }
    let link = command_link(env);
    let shown = link.path.display().to_string();
    if link.exists && env.get("HERDR_PLUGIN_ID").is_some() {
        // Plugin panes get herdr's server PATH, which usually lacks ~/.local/bin; agents' shells don't.
        return Check::new(Level::Ok, "nudge command installed").detail(shown);
    }
    if link.exists {
        Check::new(Level::Warn, "nudge command not on PATH")
            .detail(format!("{shown} exists, but ~/.local/bin isn't on PATH"))
            .fix("add ~/.local/bin to PATH")
    } else {
        Check::new(Level::Warn, "nudge command not on PATH")
            .fix("nudge setup offers to install it (agents need it for nudge notify)")
    }
}

pub(super) fn check_clipboard(env: &Env) -> Check {
    let desktop = Desktop::CURRENT;
    let found: Vec<&str> = desktop
        .clipboard_tools()
        .iter()
        .copied()
        .filter(|tool| env.which(tool).is_some())
        .collect();
    if found.is_empty() {
        Check::new(Level::Warn, "No clipboard tool")
            .detail("nudge setup falls back to typing values")
            .fix(desktop.clipboard_fix())
    } else {
        Check::new(Level::Ok, format!("Clipboard: {}", found.join(", "))).detail("used by nudge setup")
    }
}
