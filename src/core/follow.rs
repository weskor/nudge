//! Following one `hc events` stream: each line through the tracker, then state.json or the shared pipeline, with
//! a log line and a decision for every settled terminal, and `watch.json` rewritten after every line.
use std::borrow::Cow;
use std::collections::VecDeque;
use std::path::PathBuf;

use super::context::{name_space, read_app_context};
use super::events::{CoreLine, TerminalRecord, parse_line};
use super::source::CoreSource;
use super::status_file::{DECISIONS_KEPT, DecisionRecord, TerminalStatus, WatchStatus, write_status};
use super::tracker::{Effect, Tracker};
use crate::config::{Config, default_config_path, default_webhook_name, read_config};
use crate::env::{Env, Run};
use crate::pipeline::{Context, Outcome, handle};
use crate::source::Transition;
use crate::state::{now_ms, state_dir, update_state};

pub(super) fn log(message: &str) {
    eprintln!("[nudge] {message}");
}

/// How this watcher was started, for `watch.json`.
fn started_by(env: &Env) -> &'static str {
    if env.get("INVOCATION_ID").is_some() || env.get("XPC_SERVICE_NAME") == Some("dev.nudge.watch") {
        "service"
    } else if env.get("HC_TERMINAL_ID").is_some() {
        "terminal"
    } else {
        "manual"
    }
}

/// The watcher's memory across lines and connections.
pub(crate) struct Watcher<'a> {
    env: &'a Env,
    /// Runs `hc read` and `git`.
    run: &'a dyn Run,
    home: PathBuf,
    tracker: Tracker,
    started_at: i64,
    started_by: &'static str,
    connected: bool,
    last_seq: Option<u64>,
    last_event_at: Option<i64>,
    decisions: VecDeque<DecisionRecord>,
}

impl<'a> Watcher<'a> {
    pub(crate) fn new(env: &'a Env, run: &'a dyn Run) -> Self {
        Self {
            env,
            run,
            home: env.home(),
            tracker: Tracker::default(),
            started_at: now_ms(),
            started_by: started_by(env),
            connected: false,
            last_seq: None,
            last_event_at: None,
            decisions: VecDeque::new(),
        }
    }

    /// A stream started or ended. A new one starts over: which terminals to follow comes from `[core] watch` now.
    pub(crate) fn set_connected(&mut self, connected: bool) {
        if connected {
            let config = read_config(&default_config_path(self.env)).ok().flatten();
            self.tracker = Tracker::new(config.is_some_and(|config| config.core.watch_connected));
        }
        self.connected = connected;
        self.write_status();
    }

    /// Take in one line of `hc events --json`; whether it was a snapshot.
    pub(crate) fn line(&mut self, text: &str) -> bool {
        let line = parse_line(text);
        let snapshot = matches!(line, CoreLine::Snapshot(_));
        match &line {
            CoreLine::Malformed(why) => log(&format!("skipped a line from hc events: {why}")),
            CoreLine::Snapshot(snapshot) => self.last_seq = Some(snapshot.seq),
            CoreLine::Event { seq, .. } | CoreLine::Heartbeat { seq } => self.last_seq = Some(*seq),
            CoreLine::Other => {}
        }
        let now = now_ms();
        self.last_event_at = Some(now);
        let done = match self.tracker.apply(line, now) {
            Effect::Reconcile(turns) => update_state(self.env, |state| {
                state
                    .panes
                    .retain(|key, _| !key.starts_with("core:") || turns.iter().any(|(followed, _)| followed == key));
                for (key, since) in turns {
                    match since {
                        Some(start) => {
                            state.panes.entry(key).or_default().working_since.get_or_insert(start);
                        }
                        None => {
                            if let Some(pane) = state.panes.get_mut(&key) {
                                pane.working_since = None;
                            }
                        }
                    }
                }
            }),
            Effect::Forget(key) => update_state(self.env, |state| {
                state.panes.remove(&key);
            }),
            Effect::Transition(transition) => {
                self.handle(transition);
                Ok(())
            }
            Effect::Nothing => Ok(()),
        };
        if let Err(error) = done {
            log(&format!("could not update the state: {error}"));
        }
        self.write_status();
        snapshot
    }

    /// Run a status change through the pipeline; log and keep a decision for a settled one. The desktop app's
    /// context says whether you're looking at the terminal, and what its space is called.
    fn handle(&mut self, mut transition: Transition) {
        let app = read_app_context(self.env);
        transition.watched = app
            .as_ref()
            .is_some_and(|app| app.watches(&transition.agent.pane, transition.at_ms));
        name_space(&mut transition.agent, app.as_ref());
        let (address, status) = (transition.agent.pane.clone(), transition.status);
        let settles = status.settles();
        let path = default_config_path(self.env);
        // As for the TUI: the config is read only for a settling status, and loaded by the pipeline when needed.
        let config = settles.then(|| read_config(&path).ok().flatten()).flatten();
        let source = CoreSource {
            home: &self.home,
            run: self.run,
        };
        let ctx = Context {
            env: self.env,
            run: self.run,
            home: &self.home,
            config_path: &path,
        };
        let outcome = match handle(transition, &source, &ctx, config) {
            Ok(Outcome::Skipped(_)) if !settles => return,
            Ok(Outcome::Skipped(reason)) => format!("skipped ({reason})"),
            Ok(Outcome::Sent(sent)) => format!("notified webhook (HTTP {})", sent.http_status),
            Err(error) => format!("failed: {error}"),
        };
        log(&format!("{address} {}: {outcome}", status.as_str()));
        if self.decisions.len() == DECISIONS_KEPT {
            self.decisions.pop_front();
        }
        self.decisions.push_back(DecisionRecord {
            at: now_ms(),
            terminal: address,
            status: status.as_str().to_owned(),
            outcome,
        });
    }

    fn write_status(&self) {
        let config = read_config(&default_config_path(self.env)).ok().flatten();
        let status = WatchStatus {
            pid: std::process::id(),
            started_at: self.started_at,
            started_by: Cow::Borrowed(self.started_by),
            connected: self.connected,
            last_seq: self.last_seq,
            last_event_at: self.last_event_at,
            terminals: self
                .tracker
                .terminals()
                .map(|record| terminal_status(record, config.as_ref()))
                .collect(),
            decisions: Cow::Borrowed(&self.decisions),
        };
        if let Err(error) = write_status(&state_dir(self.env), &status) {
            log(&format!("could not write watch.json: {error}"));
        }
    }
}

/// A followed terminal for `watch.json`: its webhook is its route's, else the default.
fn terminal_status(record: &TerminalRecord, config: Option<&Config>) -> TerminalStatus {
    let agent = record.agent();
    let webhook = config.and_then(|config| {
        agent
            .route(&config.route)
            .map(|route| route.webhook.as_str())
            .or_else(|| default_webhook_name(config))
            .map(str::to_owned)
    });
    TerminalStatus {
        address: agent.pane,
        name: agent.name,
        status: record.status.as_ref().map(|word| word.state.clone()),
        space: agent.workspace.map(|workspace| workspace.id),
        webhook,
    }
}

#[cfg(test)]
mod tests;
