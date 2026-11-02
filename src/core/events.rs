//! `hc events --json` and `hc get --json`, parsed once at the boundary: terminal records, the snapshot that starts
//! a stream, and the notifications after it (`event`, `heartbeat`, `missed`).
use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::Value;

use crate::source::AgentStatus;

/// The label herdr-desktop puts on a terminal to place it in a space (the core's word for a workspace).
pub(super) const SPACE_LABEL: &str = "herdr.space";

/// A core terminal, as `hc events` and `hc get` describe it: the fields nudge uses.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub(crate) struct TerminalRecord {
    /// `t7` here, `box/t7` on a connected machine.
    pub(crate) address: String,
    #[serde(default)]
    pub(crate) name: Option<String>,
    #[serde(default)]
    pub(crate) labels: BTreeMap<String, String>,
    /// Where its program started, and where it last said it is (OSC 7).
    #[serde(default)]
    pub(crate) cwd: Option<String>,
    #[serde(default)]
    pub(crate) last_cwd: Option<String>,
    /// The program's own status word (`working`, `done`, …), if it reports one.
    #[serde(default)]
    pub(crate) status: Option<StatusWord>,
    /// When that word last changed, epoch ms.
    #[serde(default)]
    pub(crate) status_ms: Option<i64>,
}

/// A terminal's status report; only its word matters here.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub(crate) struct StatusWord {
    pub(crate) state: String,
}

impl TerminalRecord {
    /// Whether it's on a connected machine (`box/t7`) rather than this core.
    pub(super) fn is_remote(&self) -> bool {
        self.address.contains('/')
    }

    pub(super) fn status(&self) -> AgentStatus {
        status_of(self.status.as_ref())
    }
}

/// A status word as the pipeline knows it; no word (or one nudge doesn't know) is `Unknown`.
fn status_of(word: Option<&StatusWord>) -> AgentStatus {
    match word.map(|word| word.state.as_str()) {
        Some("idle") => AgentStatus::Idle,
        Some("working") => AgentStatus::Working,
        Some("done") => AgentStatus::Done,
        Some("blocked") => AgentStatus::Blocked,
        Some("error") => AgentStatus::Error,
        Some(_) | None => AgentStatus::Unknown,
    }
}

/// Every terminal the stream covers, at sequence `seq`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub(crate) struct Snapshot {
    #[serde(default)]
    pub(crate) seq: u64,
    pub(crate) terminals: Vec<TerminalRecord>,
}

/// One line of `hc events --json`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CoreLine {
    /// The first line, and every `missed` (after falling behind): the whole picture, to reconcile without notifying.
    Snapshot(Snapshot),
    /// A fact change about one terminal.
    Event {
        seq: u64,
        terminal: String,
        change: Change,
    },
    Heartbeat {
        seq: u64,
    },
    /// Something nudge doesn't use (another event type, a document): skipped.
    Other,
    /// Not what `hc events` prints; skipped with a log line.
    Malformed(String),
}

/// What changed about a terminal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Change {
    /// Its status word changed (`status` events). A `turn` event repeats what the status change already said, so it
    /// isn't used.
    Status(AgentStatus),
    /// It's new to the stream (`created`): its whole record.
    Created(TerminalRecord),
    /// Its labels changed (a space move).
    Labels(BTreeMap<String, String>),
    /// It finished or was removed: its state goes.
    Gone,
}

#[derive(Deserialize)]
struct Notification {
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Deserialize)]
struct EventParams {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    terminal: Option<String>,
    #[serde(default)]
    data: Value,
    #[serde(default)]
    seq: u64,
}

#[derive(Deserialize)]
struct StatusData {
    #[serde(default)]
    status: Option<StatusWord>,
}

#[derive(Deserialize)]
struct LabelsData {
    #[serde(default)]
    labels: BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct HeartbeatParams {
    #[serde(default)]
    seq: u64,
}

/// Parse one line of `hc events --json`. Never fails: what nudge can't read is `Malformed`, what it doesn't use is
/// `Other`.
pub(crate) fn parse_line(line: &str) -> CoreLine {
    let malformed = |what: &str| CoreLine::Malformed(what.to_owned());
    let Ok(value) = serde_json::from_str::<Value>(line) else {
        return malformed("not JSON");
    };
    if value.get("method").is_none() {
        return serde_json::from_value(value).map_or_else(|_| malformed("not a snapshot"), CoreLine::Snapshot);
    }
    let Ok(note) = serde_json::from_value::<Notification>(value) else {
        return malformed("not a notification");
    };
    match note.method.as_str() {
        "event" => serde_json::from_value(note.params).map_or_else(|_| malformed("bad event"), event),
        "heartbeat" => serde_json::from_value::<HeartbeatParams>(note.params).map_or_else(
            |_| malformed("bad heartbeat"),
            |beat| CoreLine::Heartbeat { seq: beat.seq },
        ),
        "missed" => serde_json::from_value(note.params).map_or_else(|_| malformed("bad missed"), CoreLine::Snapshot),
        _ => CoreLine::Other,
    }
}

fn event(params: EventParams) -> CoreLine {
    let Some(terminal) = params.terminal else {
        return CoreLine::Other;
    };
    let change = match params.kind.as_str() {
        "status" => match serde_json::from_value::<StatusData>(params.data) {
            Ok(data) => Change::Status(status_of(data.status.as_ref())),
            Err(_) => return CoreLine::Malformed("bad status event".to_owned()),
        },
        "created" => match serde_json::from_value(params.data) {
            Ok(record) => Change::Created(record),
            Err(_) => return CoreLine::Malformed("bad created event".to_owned()),
        },
        "labels" => match serde_json::from_value::<LabelsData>(params.data) {
            Ok(data) => Change::Labels(data.labels),
            Err(_) => return CoreLine::Malformed("bad labels event".to_owned()),
        },
        "finished" | "removed" => Change::Gone,
        _ => return CoreLine::Other,
    };
    CoreLine::Event {
        seq: params.seq,
        terminal,
        change,
    }
}

#[cfg(test)]
mod tests;
