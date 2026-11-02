//! The herdr-core half of `nudge status --json`: the watcher, the login service, the app context, and this
//! machine's spaces and followed terminals with where their notifications go now. Names, addresses and webhook
//! names only.
use std::collections::VecDeque;

use serde::Serialize;

use super::context::{AppContext, read_app_context};
use super::lock::watcher_pid;
use super::service::{Service, ServiceState};
use super::status_file::{DecisionRecord, TerminalStatus, WatchStatus, read_status};
use crate::config::{Config, default_webhook_name, resolve};
use crate::env::{Env, Run};
use crate::source::Backend;
use crate::state::{now_ms, state_dir};

#[derive(Debug, Serialize)]
pub(crate) struct CoreStatus {
    watcher: WatcherStatus,
    service: ServiceState,
    context: Option<ContextStatus>,
    spaces: Vec<SpaceRow>,
    terminals: Vec<TerminalRow>,
    decisions: VecDeque<DecisionRecord>,
}

/// Whether a watcher runs now (the lock file names a live process), and what its `watch.json` last said.
#[derive(Debug, Serialize)]
struct WatcherStatus {
    running: bool,
    pid: Option<u32>,
    started_by: Option<String>,
    started_at: Option<i64>,
    /// Streaming `hc events` now; never while no watcher runs.
    connected: bool,
    last_event_at: Option<i64>,
}

/// What the panel last said, and whether it's recent enough to count.
#[derive(Debug, Serialize)]
struct ContextStatus {
    terminal: Option<String>,
    window_active: bool,
    at: i64,
    fresh: bool,
}

/// Where notifications go now: the webhook, and `pane`, `workspace` (the route that matched) or `default`. Both
/// `null` without a usable config.
#[derive(Debug, Serialize)]
struct Destination {
    webhook: Option<String>,
    via: Option<&'static str>,
}

#[derive(Debug, Serialize)]
struct SpaceRow {
    id: String,
    name: String,
    #[serde(flatten)]
    to: Destination,
}

#[derive(Debug, Serialize)]
struct TerminalRow {
    address: String,
    name: Option<String>,
    status: Option<String>,
    space: Option<String>,
    space_name: Option<String>,
    #[serde(flatten)]
    to: Destination,
}

/// The core status for `nudge status --json`; `run` asks the service manager.
pub(crate) fn core_status(env: &Env, run: &dyn Run, config: Option<&Config>) -> CoreStatus {
    let dir = state_dir(env);
    let file = read_status(&dir);
    let context = read_app_context(env);
    let now = now_ms();
    let spaces = context.as_ref().map_or_else(Vec::new, |context| {
        context
            .spaces
            .iter()
            .map(|space| SpaceRow {
                to: destination(config, &[], &[&space.id, &space.name]),
                id: space.id.clone(),
                name: space.name.clone(),
            })
            .collect()
    });
    let terminals = file
        .iter()
        .flat_map(|file| &file.terminals)
        .map(|terminal| terminal_row(terminal, context.as_ref(), config))
        .collect();
    let watcher = watcher(file.as_ref(), watcher_pid(&dir.join("watch.lock")));
    CoreStatus {
        watcher,
        service: Service::here(env, run).state(),
        context: context.map(|context| ContextStatus {
            fresh: context.fresh(now),
            terminal: context.terminal,
            window_active: context.window_active,
            at: context.at,
        }),
        spaces,
        terminals,
        decisions: file.map(|file| file.decisions.into_owned()).unwrap_or_default(),
    }
}

fn watcher(file: Option<&WatchStatus<'_>>, pid: Option<u32>) -> WatcherStatus {
    let running = pid.is_some();
    WatcherStatus {
        running,
        pid,
        connected: running && file.is_some_and(|file| file.connected),
        started_by: file.map(|file| file.started_by.clone().into_owned()),
        started_at: file.map(|file| file.started_at),
        last_event_at: file.and_then(|file| file.last_event_at),
    }
}

/// A followed terminal, its space named from the context, routed with today's config.
fn terminal_row(terminal: &TerminalStatus, context: Option<&AppContext>, config: Option<&Config>) -> TerminalRow {
    let space_name = terminal
        .space
        .as_deref()
        .zip(context)
        .and_then(|(id, context)| context.space_name(id));
    let panes: Vec<&str> = [Some(terminal.address.as_str()), terminal.name.as_deref()]
        .into_iter()
        .flatten()
        .collect();
    let spaces: Vec<&str> = [terminal.space.as_deref(), space_name.as_deref()]
        .into_iter()
        .flatten()
        .collect();
    TerminalRow {
        to: destination(config, &panes, &spaces),
        address: terminal.address.clone(),
        name: terminal.name.clone(),
        status: terminal.status.clone(),
        space: terminal.space.clone(),
        space_name,
    }
}

fn destination(config: Option<&Config>, panes: &[&str], spaces: &[&str]) -> Destination {
    let Some(config) = config else {
        return Destination {
            webhook: None,
            via: None,
        };
    };
    match resolve(&config.route, Backend::Core, panes, spaces) {
        Some(route) => Destination {
            webhook: Some(route.webhook.clone()),
            via: Some(route.selector.key()),
        },
        None => Destination {
            webhook: default_webhook_name(config).map(str::to_owned),
            via: Some("default"),
        },
    }
}

#[cfg(test)]
mod tests;
