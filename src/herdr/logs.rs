//! The plugin's recent hook and action runs, as `herdr plugin log list` keeps them.
use std::path::Path;

use serde::Deserialize;

use super::query::herdr_json;
use super::registration::PLUGIN_ID;
use crate::env::{Env, Run};

/// One plugin hook or action run from `herdr plugin log list`.
#[derive(Clone, Debug, Deserialize)]
pub(crate) struct PluginLog {
    pub(crate) started_unix_ms: i64,
    pub(crate) status: String,
    #[serde(default)]
    pub(crate) stderr: String,
}

#[derive(Debug, Deserialize)]
struct LogList {
    result: LogListResult,
}

#[derive(Debug, Deserialize)]
struct LogListResult {
    logs: Vec<PluginLog>,
}

/// Most recent plugin hook/action runs, newest last.
pub(crate) fn plugin_logs(env: &Env, run: &dyn Run) -> Vec<PluginLog> {
    let args = ["plugin", "log", "list", "--plugin", PLUGIN_ID];
    let mut logs = herdr_json::<LogList>(&args, env, Path::new("/"), run)
        .map(|list| list.result.logs)
        .unwrap_or_default();
    logs.sort_by_key(|log| log.started_unix_ms);
    logs
}
