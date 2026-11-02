//! What the watcher knows about core terminals between lines of `hc events`, and what each line means: start over
//! from a snapshot, a status change for the pipeline, or a terminal that's gone.
use std::collections::BTreeMap;

use super::events::{Change, CoreLine, Snapshot, StatusWord, TerminalRecord};
use crate::source::{AgentRef, AgentStatus, Backend, Transition};

/// What a line asks of the watcher.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Effect {
    /// Start over from a snapshot, without notifying: each followed terminal's state key, and when its current turn
    /// started if it's working now. State for core terminals not listed goes.
    Reconcile(Vec<(String, Option<i64>)>),
    /// A status change for the pipeline.
    Transition(Transition),
    /// A terminal finished or was removed: its state goes.
    Forget(String),
    Nothing,
}

/// The terminals the watcher follows, by address.
#[derive(Debug, Default)]
pub(crate) struct Tracker {
    terminals: BTreeMap<String, TerminalRecord>,
    /// Also follow connected machines' terminals (`[core] watch = "connected"`); else only this core's own.
    connected_too: bool,
}

impl Tracker {
    pub(crate) fn new(connected_too: bool) -> Self {
        Self {
            terminals: BTreeMap::new(),
            connected_too,
        }
    }

    /// The terminals followed now.
    pub(crate) fn terminals(&self) -> impl Iterator<Item = &TerminalRecord> {
        self.terminals.values()
    }

    fn follows(&self, address: &str) -> bool {
        self.connected_too || !address.contains('/')
    }

    /// Take in one line, at `now` (epoch ms).
    pub(crate) fn apply(&mut self, line: CoreLine, now: i64) -> Effect {
        match line {
            CoreLine::Snapshot(snapshot) => self.reconcile(snapshot, now),
            CoreLine::Event { terminal, change, .. } if self.follows(&terminal) => self.change(terminal, change, now),
            CoreLine::Event { .. } | CoreLine::Heartbeat { .. } | CoreLine::Other | CoreLine::Malformed(_) => {
                Effect::Nothing
            }
        }
    }

    fn reconcile(&mut self, snapshot: Snapshot, now: i64) -> Effect {
        self.terminals = snapshot
            .terminals
            .into_iter()
            .filter(|record| self.connected_too || !record.is_remote())
            .map(|record| (record.address.clone(), record))
            .collect();
        let turns = self
            .terminals
            .values()
            .map(|record| {
                let working = record.status() == AgentStatus::Working;
                let since = working.then(|| record.status_ms.unwrap_or(now));
                (record.agent().state_key(), since)
            })
            .collect();
        Effect::Reconcile(turns)
    }

    fn change(&mut self, address: String, change: Change, now: i64) -> Effect {
        match change {
            Change::Status(status) => {
                let record = self.terminals.entry(address.clone()).or_insert_with(|| TerminalRecord {
                    address,
                    ..TerminalRecord::default()
                });
                record.status = Some(StatusWord {
                    state: status.as_str().to_owned(),
                });
                record.status_ms = Some(now);
                Effect::Transition(Transition {
                    agent: record.agent(),
                    status,
                    // Which terminal you're looking at comes from the desktop app, later; until then, none.
                    watched: false,
                    at_ms: now,
                })
            }
            Change::Created(record) => {
                self.terminals.insert(address, record);
                Effect::Nothing
            }
            Change::Labels(labels) => {
                if let Some(record) = self.terminals.get_mut(&address) {
                    record.labels = labels;
                }
                Effect::Nothing
            }
            Change::Gone => {
                self.terminals.remove(&address);
                Effect::Forget(AgentRef::in_pane(Backend::Core, address).state_key())
            }
        }
    }
}

#[cfg(test)]
mod tests;
