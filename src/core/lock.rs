//! One watcher per machine: an advisory `flock` on `watch.lock` in the state dir, which the kernel releases when
//! the process dies, so a crash never leaves a stale lock. The holder writes its pid into the file.
use std::fs::{self, File, OpenOptions};
use std::io::{Seek as _, Write as _};
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::Path;

use rustix::fs::{FlockOperation, flock};
use rustix::io::Errno;
use rustix::process::{Pid, test_kill_process};

use crate::error::{Error, Result};

/// The held lock; dropping it (or exiting) releases it.
#[derive(Debug)]
pub(crate) struct WatchLock {
    _file: File,
}

/// Who has the lock.
#[derive(Debug)]
pub(crate) enum Locked {
    /// This process: keep it for as long as the watcher runs.
    Here(WatchLock),
    /// Another watcher, with this pid if it wrote one.
    Elsewhere(Option<u32>),
}

/// Take `path` (created, mode 600) for this process, unless another process holds it.
pub(crate) fn lock_watch(path: &Path) -> Result<Locked> {
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(path)?;
    match flock(&file, FlockOperation::NonBlockingLockExclusive) {
        Ok(()) => {
            file.set_len(0)?;
            file.rewind()?;
            file.write_all(std::process::id().to_string().as_bytes())?;
            Ok(Locked::Here(WatchLock { _file: file }))
        }
        Err(Errno::WOULDBLOCK) => {
            let pid = fs::read_to_string(path)?;
            Ok(Locked::Elsewhere(pid.trim().parse().ok()))
        }
        Err(error) => Err(Error::Other(format!("could not lock {}: {error}", path.display()))),
    }
}

/// The pid of the running watcher: the one the lock file at `path` names, if that process is alive. It reads the
/// file without taking the lock, so asking never stops a watcher from starting.
pub(crate) fn watcher_pid(path: &Path) -> Option<u32> {
    let pid: u32 = fs::read_to_string(path).ok()?.trim().parse().ok()?;
    let alive = Pid::from_raw(i32::try_from(pid).ok()?)?;
    // EPERM: it exists, under another user.
    match test_kill_process(alive) {
        Ok(()) => Some(pid),
        Err(error) if error == Errno::PERM => Some(pid),
        Err(_) => None,
    }
}

#[cfg(test)]
mod tests;
