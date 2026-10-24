//! Small persistent state shared by the CLI and the herdr plugin, e.g. `~/.local/state/nudge/state.json`.
use std::collections::BTreeMap;
use std::fs::{self, DirBuilder, OpenOptions};
use std::io::{ErrorKind, Write as _};
use std::os::unix::fs::{DirBuilderExt as _, OpenOptionsExt as _};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::context::Env;
use crate::error::{Error, Result};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PaneState {
    /// When the current turn started (first `working` after idle/done), epoch ms.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) working_since: Option<i64>,
    /// Last automatic notification sent by the plugin for this pane, epoch ms.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) last_auto_at: Option<i64>,
    /// Last time the agent in this pane ran `nudge notify` itself, epoch ms.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) self_notified_at: Option<i64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct State {
    /// Automatic notifications from the herdr plugin. Off until turned on (setup asks), since they send
    /// terminal output.
    #[serde(default)]
    pub(crate) auto: bool,
    /// You said you're away: notify even for the focused pane, and even with auto-notify off. Set with the
    /// `away` herdr action or in `nudge settings`; absent from the file while off.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub(crate) away: bool,
    #[serde(default)]
    pub(crate) panes: BTreeMap<String, PaneState>,
}

const LOCK_RETRIES: u32 = 100;
const LOCK_RETRY: Duration = Duration::from_millis(20);
const LOCK_STALE: Duration = Duration::from_secs(5);

/// Milliseconds since the Unix epoch.
pub(crate) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| i64::try_from(since.as_millis()).unwrap_or(i64::MAX))
}

/// `$NUDGE_STATE_DIR`, else `$XDG_STATE_HOME/nudge`, else `~/.local/state/nudge`.
pub(crate) fn state_dir(env: &Env) -> PathBuf {
    if let Some(dir) = env.get("NUDGE_STATE_DIR") {
        return PathBuf::from(dir);
    }
    env.get("XDG_STATE_HOME")
        .map_or_else(|| env.home().join(".local").join("state"), PathBuf::from)
        .join("nudge")
}

/// Missing or corrupt state starts fresh; it only holds timestamps and a toggle.
fn read_state(path: &Path) -> State {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// The state as saved, without taking the lock or rewriting the file.
pub(crate) fn current_state(env: &Env) -> State {
    read_state(&state_dir(env).join("state.json"))
}

/// An exclusive lock: a directory next to the state file, removed on drop.
#[derive(Debug)]
struct Lock(PathBuf);

impl Lock {
    fn acquire(path: PathBuf) -> Result<Self> {
        for attempt in 0.. {
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                    let age = fs::metadata(&path)
                        .and_then(|meta| meta.modified())
                        .ok()
                        .and_then(|at| at.elapsed().ok());
                    if age.is_some_and(|age| age > LOCK_STALE) {
                        fs::remove_dir_all(&path).ok();
                    } else if attempt >= LOCK_RETRIES {
                        return Err(Error::Other(format!("timed out waiting for {}", path.display())));
                    } else {
                        thread::sleep(LOCK_RETRY);
                    }
                }
                Err(error) => return Err(error.into()),
            }
        }
        Err(Error::Other(format!("timed out waiting for {}", path.display())))
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).ok();
    }
}

/// Read-modify-write the state under a lock. `change` mutates the state and returns a result.
pub(crate) fn update_state<T>(env: &Env, change: impl FnOnce(&mut State) -> T) -> Result<T> {
    let dir = state_dir(env);
    DirBuilder::new().recursive(true).mode(0o700).create(&dir)?;
    let path = dir.join("state.json");
    let _lock = Lock::acquire(dir.join("state.json.lock"))?;
    let mut state = read_state(&path);
    let result = change(&mut state);
    let temp = dir.join(format!("state.json.{}.tmp", std::process::id()));
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&temp)?;
    file.write_all(
        serde_json::to_string(&state)
            .map_err(|error| Error::Other(error.to_string()))?
            .as_bytes(),
    )?;
    fs::rename(&temp, &path)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use std::thread;

    use super::{PaneState, update_state};
    use crate::context::Env;

    #[test]
    fn concurrent_writers_lose_no_updates() {
        let dir = tempfile::tempdir().unwrap();
        let env = Env::from([("NUDGE_STATE_DIR", dir.path().to_str().unwrap())]);
        thread::scope(|scope| {
            for i in 0..20 {
                let env = &env;
                let _writer = scope.spawn(move || {
                    update_state(env, |state| {
                        let pane = PaneState {
                            last_auto_at: Some(i),
                            ..PaneState::default()
                        };
                        state.panes.insert(format!("p{i}"), pane)
                    })
                    .unwrap()
                });
            }
        });
        assert_eq!(update_state(&env, |state| state.panes.len()).unwrap(), 20);
    }

    #[test]
    fn corrupt_state_starts_fresh_with_auto_off() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("state.json"), "{not json").unwrap();
        let env = Env::from([("NUDGE_STATE_DIR", dir.path().to_str().unwrap())]);
        assert!(!update_state(&env, |state| state.auto).unwrap());
    }
}
