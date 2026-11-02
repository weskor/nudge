//! `watch.json` in the state dir: what the watcher is doing, for the desktop panel (`nudge status --json`). Written
//! after every line it reads (heartbeats included, so at least every 15 s), never with secrets.
use std::borrow::Cow;
use std::collections::VecDeque;
use std::fs::{self, DirBuilder, OpenOptions};
use std::io::Write as _;
use std::os::unix::fs::{DirBuilderExt as _, OpenOptionsExt as _};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// How many recent decisions the file keeps.
pub(crate) const DECISIONS_KEPT: usize = 20;

/// One thing the watcher decided about a settled terminal.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct DecisionRecord {
    /// Epoch ms.
    pub(crate) at: i64,
    pub(crate) terminal: String,
    pub(crate) status: String,
    /// `notified webhook "main" (HTTP 200)`, `skipped (cooldown)`, `failed: …`.
    pub(crate) outcome: String,
}

/// A followed terminal as the panel shows it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct TerminalStatus {
    pub(crate) address: String,
    pub(crate) name: Option<String>,
    pub(crate) status: Option<String>,
    /// Its space id (the `herdr.space` label).
    pub(crate) space: Option<String>,
    /// The webhook its notifications go to now: its route's, else the default; `None` without a usable config.
    pub(crate) webhook: Option<String>,
}

/// The whole file. The watcher writes it from borrowed parts; `nudge status` reads it back owned.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct WatchStatus<'a> {
    pub(crate) pid: u32,
    /// Epoch ms.
    pub(crate) started_at: i64,
    /// `service` (systemd or launchd), `terminal` (a core terminal) or `manual`.
    pub(crate) started_by: Cow<'a, str>,
    /// Whether `hc events` is streaming now.
    pub(crate) connected: bool,
    pub(crate) last_seq: Option<u64>,
    /// When the last line arrived, epoch ms.
    pub(crate) last_event_at: Option<i64>,
    pub(crate) terminals: Vec<TerminalStatus>,
    pub(crate) decisions: Cow<'a, VecDeque<DecisionRecord>>,
}

/// `dir/watch.json`, if a watcher has written one that parses.
pub(crate) fn read_status(dir: &Path) -> Option<WatchStatus<'static>> {
    let text = fs::read_to_string(dir.join("watch.json")).ok()?;
    serde_json::from_str(&text).ok()
}

/// Write `status` to `dir/watch.json` (mode 600), replacing it whole.
pub(crate) fn write_status(dir: &Path, status: &WatchStatus<'_>) -> Result<()> {
    let text = serde_json::to_string_pretty(status).map_err(|error| Error::Other(error.to_string()))?;
    DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
    let temp = dir.join(format!("watch.json.{}.tmp", std::process::id()));
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&temp)?;
    file.write_all(text.as_bytes())?;
    fs::rename(&temp, dir.join("watch.json"))?;
    Ok(())
}
