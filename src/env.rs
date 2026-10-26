//! Where nudge runs: the environment variables it reads and the external commands it runs (herdr, git, clipboard
//! tools). Logic gets both passed in, so tests substitute their own.
use std::collections::HashMap;
use std::io::Read as _;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

/// A snapshot of environment variables. Empty values count as unset.
#[derive(Clone, Debug, Default)]
pub(crate) struct Env(HashMap<String, String>);

impl Env {
    pub(crate) fn from_process() -> Self {
        Self(
            std::env::vars_os()
                .filter_map(|(key, value)| Some((key.into_string().ok()?, value.into_string().ok()?)))
                .collect(),
        )
    }

    pub(crate) fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(String::as_str).filter(|value| !value.is_empty())
    }

    /// `$HOME`, else the account's home directory, else `/`.
    pub(crate) fn home(&self) -> PathBuf {
        self.get("HOME")
            .map(PathBuf::from)
            .or_else(std::env::home_dir)
            .unwrap_or_else(|| PathBuf::from("/"))
    }

    /// The herdr CLI: `$HERDR_BIN_PATH` (set by herdr for plugin commands), else `herdr` on PATH.
    pub(crate) fn herdr(&self) -> &str {
        self.get("HERDR_BIN_PATH").unwrap_or("herdr")
    }

    /// The executable `name` on `$PATH`, like `which`.
    pub(crate) fn which(&self, name: &str) -> Option<PathBuf> {
        std::env::split_paths(self.get("PATH")?)
            .map(|dir| dir.join(name))
            .find(|path| {
                path.metadata()
                    .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
            })
    }
}

impl<const N: usize> From<[(&str, &str); N]> for Env {
    fn from(pairs: [(&str, &str); N]) -> Self {
        Self(
            pairs
                .into_iter()
                .map(|(key, value)| (key.to_owned(), value.to_owned()))
                .collect(),
        )
    }
}

/// Runs external commands. Tests substitute a fake keyed by the command line.
pub(crate) trait Run: Sync {
    /// Run `cmd` in `cwd`: its trimmed stdout on exit 0; `None` on failure, a missing binary or a timeout.
    fn run(&self, cmd: &[&str], cwd: &Path) -> Option<String>;
}

/// Runs real processes, giving each at most [`COMMAND_TIMEOUT`].
#[derive(Debug)]
pub(crate) struct System;

const COMMAND_TIMEOUT: Duration = Duration::from_secs(3);

impl Run for System {
    fn run(&self, cmd: &[&str], cwd: &Path) -> Option<String> {
        let (program, args) = cmd.split_first()?;
        let mut child = Command::new(program)
            .args(args)
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let mut stdout = child.stdout.take()?;
        let reader = thread::spawn(move || {
            let mut out = Vec::new();
            stdout.read_to_end(&mut out).map(|_| out)
        });
        let deadline = Instant::now() + COMMAND_TIMEOUT;
        let status = loop {
            if let Some(status) = child.try_wait().ok()? {
                break status;
            }
            if Instant::now() >= deadline {
                child.kill().ok();
                child.wait().ok();
                return None;
            }
            thread::sleep(Duration::from_millis(2));
        };
        let out = reader.join().ok()?.ok()?;
        status
            .success()
            .then(|| String::from_utf8_lossy(&out).trim().to_owned())
    }
}

#[cfg(test)]
pub(crate) mod tests;
