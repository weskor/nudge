//! `nudge core watch`: one long-running watcher per machine. It takes the lock, follows `hc events --json`, and
//! when the stream ends (core restarted, not running yet, `hc` missing) retries with backoff, forever unless
//! `--once`.
use std::fs::DirBuilder;
use std::io::{BufRead, BufReader};
use std::os::unix::fs::DirBuilderExt as _;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use super::follow::{Watcher, log};
use super::lock::{Locked, lock_watch};
use super::source::HC;
use crate::env::{Env, System};
use crate::error::{Error, Result};
use crate::state::state_dir;

/// Waits between attempts to (re)connect: 1, 2, 5, 10, 30 s, then every 30 s. A stream that delivered its snapshot
/// starts the sequence over.
const BACKOFF_SECONDS: [u64; 5] = [1, 2, 5, 10, 30];

/// Where the retry schedule is.
#[derive(Debug, Default)]
pub(crate) struct Backoff {
    attempt: usize,
}

impl Backoff {
    pub(crate) fn next(&mut self) -> Duration {
        let seconds = BACKOFF_SECONDS
            .get(self.attempt)
            .or(BACKOFF_SECONDS.last())
            .copied()
            .unwrap_or(30);
        self.attempt = self.attempt.saturating_add(1);
        Duration::from_secs(seconds)
    }

    pub(crate) const fn reset(&mut self) {
        self.attempt = 0;
    }
}

/// Follow `lines` to the end; whether a snapshot came, so the connection counts as made.
pub(crate) fn follow(lines: impl BufRead, watcher: &mut Watcher<'_>) -> bool {
    watcher.set_connected(true);
    let mut connected = false;
    for line in lines.lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        connected |= watcher.line(&line);
    }
    watcher.set_connected(false);
    connected
}

/// Run `hc events --json` and follow it until it ends; whether it delivered a snapshot.
fn stream(watcher: &mut Watcher<'_>) -> Result<bool> {
    let mut child = Command::new(HC)
        .args(["events", "--json"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| Error::Other(format!("could not run hc events: {error}")))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| Error::Other("hc events has no output".into()))?;
    let connected = follow(BufReader::new(stdout), watcher);
    child.kill().ok();
    child.wait().ok();
    Ok(connected)
}

/// `nudge core watch [--once]`.
pub(crate) fn watch(env: &Env, once: bool) -> Result<u8> {
    let dir = state_dir(env);
    DirBuilder::new().recursive(true).mode(0o700).create(&dir)?;
    let _lock = match lock_watch(&dir.join("watch.lock"))? {
        Locked::Here(lock) => lock,
        Locked::Elsewhere(pid) => {
            let pid = pid.map(|pid| format!(" (pid {pid})")).unwrap_or_default();
            println!("nudge core watch is already running{pid}");
            return Ok(0);
        }
    };
    let mut watcher = Watcher::new(env, &System);
    let mut backoff = Backoff::default();
    loop {
        match stream(&mut watcher) {
            Ok(true) => {
                log("hc events ended");
                backoff.reset();
                if once {
                    return Ok(0);
                }
            }
            Ok(false) if once => {
                return Err(Error::Other(
                    "hc events ended before its first snapshot: is herdr-core running?".into(),
                ));
            }
            Ok(false) => log("hc events ended before its first snapshot"),
            Err(error) if once => return Err(error),
            Err(error) => log(&error.to_string()),
        }
        let wait = backoff.next();
        log(&format!("reconnecting to herdr-core in {}s", wait.as_secs()));
        thread::sleep(wait);
    }
}

#[cfg(test)]
mod tests;
