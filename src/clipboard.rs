//! Read the clipboard for `nudge setup`, also from herdr plugin panes that lack the graphical session's env.
use std::fs;
use std::path::{Path, PathBuf};

use crate::env::{Env, Run};

const WL_PASTE: [&str; 4] = ["wl-paste", "--no-newline", "--type", "text"];
const XCLIP: [&str; 4] = ["xclip", "-selection", "clipboard", "-o"];
const PBPASTE: [&str; 1] = ["pbpaste"];

/// The desktop nudge runs on, which decides the clipboard tools.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Desktop {
    MacOs,
    /// Linux and other Unix desktops: Wayland or X11.
    Unix,
}

impl Desktop {
    pub(crate) const CURRENT: Self = if cfg!(target_os = "macos") {
        Self::MacOs
    } else {
        Self::Unix
    };

    /// Clipboard tools `nudge setup` can use here, for doctor's report.
    pub(crate) const fn clipboard_tools(self) -> &'static [&'static str] {
        match self {
            Self::MacOs => &["pbpaste"],
            Self::Unix => &["wl-paste", "xclip"],
        }
    }

    /// What to do when none of [`Self::clipboard_tools`] is found.
    pub(crate) const fn clipboard_fix(self) -> &'static str {
        match self {
            Self::MacOs => "pbpaste comes with macOS; check that /usr/bin is on PATH",
            Self::Unix => "install wl-clipboard or xclip",
        }
    }
}

/// Clipboard commands to try, in order. macOS: `pbpaste`. Elsewhere: wl-paste, then xclip, each as-is first.
/// Processes started by the herdr server (plugin panes) may lack the graphical session's `XDG_RUNTIME_DIR`,
/// `WAYLAND_DISPLAY` or `DISPLAY`, so also try every Wayland socket in the runtime dir and `DISPLAY=:0`.
pub(crate) fn clipboard_commands(
    desktop: Desktop,
    env: &Env,
    runtime_dir: &str,
    wayland_sockets: &[String],
) -> Vec<Vec<String>> {
    let owned = |args: &[&str]| args.iter().map(|&arg| arg.to_owned()).collect::<Vec<String>>();
    match desktop {
        Desktop::MacOs => return vec![owned(&PBPASTE)],
        Desktop::Unix => {}
    }
    let mut commands = vec![owned(&WL_PASTE)];
    for socket in wayland_sockets {
        if env.get("WAYLAND_DISPLAY") == Some(socket.as_str()) && env.get("XDG_RUNTIME_DIR") == Some(runtime_dir) {
            continue;
        }
        let mut command = vec![
            "env".to_owned(),
            format!("XDG_RUNTIME_DIR={runtime_dir}"),
            format!("WAYLAND_DISPLAY={socket}"),
        ];
        command.extend(owned(&WL_PASTE));
        commands.push(command);
    }
    commands.push(owned(&XCLIP));
    if env.get("DISPLAY").is_none() {
        let mut command = vec!["env".to_owned(), "DISPLAY=:0".to_owned()];
        command.extend(owned(&XCLIP));
        commands.push(command);
    }
    commands
}

fn wayland_sockets_in(dir: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .filter(|name| {
            name.strip_prefix("wayland-")
                .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        })
        .collect()
}

/// `$XDG_RUNTIME_DIR`, else `/run/user/<uid>`.
pub(crate) fn runtime_dir(env: &Env) -> PathBuf {
    env.get("XDG_RUNTIME_DIR").map_or_else(
        || PathBuf::from(format!("/run/user/{}", rustix::process::getuid().as_raw())),
        PathBuf::from,
    )
}

/// Trimmed text from the first clipboard command for `desktop` that works and isn't empty; `None` if none do.
pub(crate) fn read_clipboard(desktop: Desktop, run: &dyn Run, env: &Env, runtime_dir: &Path) -> Option<String> {
    let dir = runtime_dir.to_string_lossy();
    let sockets = match desktop {
        Desktop::MacOs => Vec::new(),
        Desktop::Unix => wayland_sockets_in(runtime_dir),
    };
    clipboard_commands(desktop, env, &dir, &sockets)
        .into_iter()
        .find_map(|cmd| {
            let args: Vec<&str> = cmd.iter().map(String::as_str).collect();
            run.run(&args, Path::new("/")).filter(|text| !text.is_empty())
        })
}

#[cfg(test)]
mod tests;
