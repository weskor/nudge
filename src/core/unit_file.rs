//! The login service's file: a systemd user unit (Linux) or a launchd agent plist (macOS) that runs
//! `nudge core watch` at login, restarts it if it fails, and gives it the PATH captured at install so `hc` is found.
use std::path::{Path, PathBuf};

use crate::env::Env;

/// The systemd unit's name, and the launchd agent's label.
pub(crate) const UNIT: &str = "nudge-watch.service";
pub(crate) const LABEL: &str = "dev.nudge.watch";

/// Which service manager runs the watcher.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ServiceKind {
    /// A systemd user unit (Linux).
    SystemdUser,
    /// A launchd agent (macOS).
    LaunchdAgent,
}

impl ServiceKind {
    /// This platform's.
    pub(crate) const CURRENT: Self = if cfg!(target_os = "macos") {
        Self::LaunchdAgent
    } else {
        Self::SystemdUser
    };

    /// Where the file goes: `$XDG_CONFIG_HOME/systemd/user/nudge-watch.service` (else under `~/.config`), or
    /// `~/Library/LaunchAgents/dev.nudge.watch.plist`.
    pub(crate) fn path(self, env: &Env) -> PathBuf {
        match self {
            Self::SystemdUser => env
                .get("XDG_CONFIG_HOME")
                .map_or_else(|| env.home().join(".config"), PathBuf::from)
                .join("systemd")
                .join("user")
                .join(UNIT),
            Self::LaunchdAgent => env
                .home()
                .join("Library")
                .join("LaunchAgents")
                .join(format!("{LABEL}.plist")),
        }
    }
}

/// A systemd unit value in double quotes: `\` and `"` escaped, `%` doubled (systemd's specifier character).
fn systemd_quoted(value: &str) -> String {
    let escaped = value.replace('\\', r"\\").replace('"', "\\\"").replace('%', "%%");
    format!("\"{escaped}\"")
}

/// `value` as XML character data.
fn xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The file's text for `nudge` (the command to run) and the captured `path_var`.
pub(crate) fn service_file(kind: ServiceKind, nudge: &Path, path_var: &str) -> String {
    let nudge = nudge.to_string_lossy();
    match kind {
        ServiceKind::SystemdUser => format!(
            "# Written by `nudge core service install`; `nudge core service remove` deletes it.\n\
             [Unit]\n\
             Description=nudge: notify your webhook about herdr-core agents\n\
             \n\
             [Service]\n\
             ExecStart={} core watch\n\
             Restart=on-failure\n\
             RestartSec=10\n\
             Environment={}\n\
             \n\
             [Install]\n\
             WantedBy=default.target\n",
            systemd_quoted(&nudge),
            systemd_quoted(&format!("PATH={path_var}")),
        ),
        ServiceKind::LaunchdAgent => format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
             <!-- Written by `nudge core service install`; `nudge core service remove` deletes it. -->\n\
             <plist version=\"1.0\">\n\
             <dict>\n\
             \x20 <key>Label</key>\n\
             \x20 <string>{LABEL}</string>\n\
             \x20 <key>ProgramArguments</key>\n\
             \x20 <array>\n\
             \x20   <string>{}</string>\n\
             \x20   <string>core</string>\n\
             \x20   <string>watch</string>\n\
             \x20 </array>\n\
             \x20 <key>EnvironmentVariables</key>\n\
             \x20 <dict>\n\
             \x20   <key>PATH</key>\n\
             \x20   <string>{}</string>\n\
             \x20 </dict>\n\
             \x20 <key>RunAtLoad</key>\n\
             \x20 <true/>\n\
             \x20 <key>KeepAlive</key>\n\
             \x20 <dict>\n\
             \x20   <key>SuccessfulExit</key>\n\
             \x20   <false/>\n\
             \x20 </dict>\n\
             </dict>\n\
             </plist>\n",
            xml(&nudge),
            xml(path_var),
        ),
    }
}

#[cfg(test)]
mod tests;
