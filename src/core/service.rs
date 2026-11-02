//! `nudge core service install | remove | status`: `nudge core watch` as a login service, through
//! `systemctl --user` (Linux) or `launchctl` (macOS). Every command goes through `Run`, so tests use fakes.
use std::fs::{self, DirBuilder};
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::unit_file::{LABEL, ServiceKind, UNIT, service_file};
use crate::env::{Env, Run, System};
use crate::error::{Error, Result};
use crate::herdr::command_link;

/// `nudge core service <action>` on this machine, with its service manager.
pub(crate) fn service(action: ServiceAction, env: &Env) -> Result<u8> {
    Service::here(env, &System).act(action, &nudge_command(env)?)
}

/// The command the service runs: `~/.local/bin/nudge` when it exists (herdr keeps it pointing at the current
/// install, so plugin updates don't strand the service), else this binary.
fn nudge_command(env: &Env) -> Result<PathBuf> {
    let link = command_link(env);
    if link.exists {
        return Ok(link.path);
    }
    Ok(std::env::current_exe()?.canonicalize()?)
}

/// What `nudge core service` does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ServiceAction {
    Install,
    Remove,
    Status,
}

/// The login service on this machine: its kind, where its file goes, and how to run its manager.
pub(crate) struct Service<'a> {
    pub(crate) kind: ServiceKind,
    pub(crate) env: &'a Env,
    pub(crate) run: &'a dyn Run,
    /// The user's id, for launchd's `gui/<uid>` domain.
    pub(crate) uid: u32,
}

/// The login service as `status` reports it.
#[derive(Debug, Serialize)]
pub(crate) struct ServiceState {
    pub(crate) installed: bool,
    pub(crate) running: bool,
    pub(crate) path: PathBuf,
}

impl<'a> Service<'a> {
    /// This machine's service, for this user.
    pub(crate) fn here(env: &'a Env, run: &'a dyn Run) -> Self {
        Self {
            kind: ServiceKind::CURRENT,
            env,
            run,
            uid: rustix::process::getuid().as_raw(),
        }
    }

    /// Whether it's installed and running, and where its file is.
    pub(crate) fn state(&self) -> ServiceState {
        let path = self.kind.path(self.env);
        let installed = path.exists();
        ServiceState {
            running: installed && self.running(),
            installed,
            path,
        }
    }

    /// Run a service-manager command from `/`; its output, or `None` if it failed.
    fn manager(&self, args: &[&str]) -> Option<String> {
        self.run.run(args, Path::new("/"))
    }

    /// Run a command that has to work, else say which one failed.
    fn required(&self, args: &[&str]) -> Result<()> {
        self.manager(args)
            .map(|_| ())
            .ok_or_else(|| Error::Other(format!("`{}` failed", args.join(" "))))
    }

    fn domain(&self) -> String {
        format!("gui/{}", self.uid)
    }

    fn target(&self) -> String {
        format!("gui/{}/{LABEL}", self.uid)
    }

    /// Run `action`; `nudge` is the command the service runs. Returns the exit code.
    pub(crate) fn act(&self, action: ServiceAction, nudge: &Path) -> Result<u8> {
        let path = self.kind.path(self.env);
        let shown = path.display();
        match action {
            ServiceAction::Install => {
                self.install(&path, nudge)?;
                println!("Installed the login service: {shown}");
                println!(
                    "It runs `{} core watch` at login and restarts it if it fails.",
                    nudge.display()
                );
                if let Some(note) = self.linger_note() {
                    println!("{note}");
                }
            }
            ServiceAction::Remove if !path.exists() => println!("The login service isn't installed."),
            ServiceAction::Remove => {
                self.remove(&path)?;
                println!("Removed the login service ({shown}).");
            }
            ServiceAction::Status => {
                let state = self.state();
                let words = match (state.installed, state.running) {
                    (false, _) => "not installed",
                    (true, true) => "installed, running",
                    (true, false) => "installed, not running",
                };
                println!("Login service: {words}\n  {shown}");
            }
        }
        Ok(0)
    }

    /// Write the file and start the service now and at every login. A reinstall restarts a running watcher, so it
    /// runs what the new file says.
    fn install(&self, path: &Path, nudge: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            DirBuilder::new().recursive(true).create(dir)?;
        }
        // Only absolute entries: an empty or relative one would resolve against the service's working directory.
        let absolute: Vec<&str> = self
            .env
            .get("PATH")
            .unwrap_or_default()
            .split(':')
            .filter(|entry| entry.starts_with('/'))
            .collect();
        let path_var = if absolute.is_empty() {
            "/usr/local/bin:/usr/bin:/bin".to_owned()
        } else {
            absolute.join(":")
        };
        fs::write(path, service_file(self.kind, nudge, &path_var))?;
        let file = path.to_string_lossy();
        match self.kind {
            ServiceKind::SystemdUser => {
                self.required(&["systemctl", "--user", "daemon-reload"])?;
                self.required(&["systemctl", "--user", "enable", UNIT])?;
                // `restart` starts a stopped unit too, and picks up a changed file in a running one.
                self.required(&["systemctl", "--user", "restart", UNIT])
            }
            ServiceKind::LaunchdAgent => {
                // Loaded already (a reinstall): unload it first, or bootstrap refuses.
                let _unloaded = self.manager(&["launchctl", "bootout", &self.target()]);
                self.required(&["launchctl", "bootstrap", &self.domain(), &file])
            }
        }
    }

    /// Stop the service, keep it from starting at login, and delete its file.
    fn remove(&self, path: &Path) -> Result<()> {
        match self.kind {
            ServiceKind::SystemdUser => {
                // Best effort: a unit that isn't loaded can still be deleted.
                let _disabled = self.manager(&["systemctl", "--user", "disable", "--now", UNIT]);
                fs::remove_file(path)?;
                self.required(&["systemctl", "--user", "daemon-reload"])
            }
            ServiceKind::LaunchdAgent => {
                let _unloaded = self.manager(&["launchctl", "bootout", &self.target()]);
                fs::remove_file(path)?;
                Ok(())
            }
        }
    }

    fn running(&self) -> bool {
        match self.kind {
            ServiceKind::SystemdUser => {
                self.manager(&["systemctl", "--user", "is-active", UNIT]).as_deref() == Some("active")
            }
            ServiceKind::LaunchdAgent => self
                .manager(&["launchctl", "print", &self.target()])
                .is_some_and(|text| text.contains("state = running")),
        }
    }

    /// On Linux, a user service stops at logout unless lingering is on; say how to turn it on when it's off. nudge
    /// doesn't change it: that's the machine owner's call.
    fn linger_note(&self) -> Option<String> {
        let user = self.env.get("USER")?;
        match self.kind {
            ServiceKind::SystemdUser => {
                let linger = self.manager(&["loginctl", "show-user", user, "--property=Linger", "--value"])?;
                (linger == "no").then(|| {
                    format!(
                        "! Lingering is off for {user}, so the service stops when you log out. On a server, keep it \
                         running with: loginctl enable-linger {user}"
                    )
                })
            }
            ServiceKind::LaunchdAgent => None,
        }
    }
}

#[cfg(test)]
mod tests;
