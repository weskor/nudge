//! nudge's herdr plugin registration: its id and whether herdr has it installed.
use std::path::Path;

use serde::Deserialize;

use super::query::herdr_json;
use crate::env::{Env, Run};

pub(crate) const PLUGIN_ID: &str = "nudge";

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub(crate) struct PluginInfo {
    pub(crate) enabled: bool,
    pub(crate) plugin_root: String,
    pub(crate) version: String,
}

#[derive(Debug, Deserialize)]
struct PluginList {
    result: PluginListResult,
}

#[derive(Debug, Deserialize)]
struct PluginListResult {
    plugins: Vec<PluginInfo>,
}

/// The herdr plugin as herdr sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Plugin {
    /// herdr isn't installed or its server isn't running.
    HerdrUnavailable,
    NotInstalled,
    Installed(PluginInfo),
}

pub(crate) fn plugin(env: &Env, run: &dyn Run) -> Plugin {
    let args = ["plugin", "list", "--plugin", PLUGIN_ID, "--json"];
    match herdr_json::<PluginList>(&args, env, Path::new("/"), run) {
        None => Plugin::HerdrUnavailable,
        Some(list) => list
            .result
            .plugins
            .into_iter()
            .next()
            .map_or(Plugin::NotInstalled, Plugin::Installed),
    }
}
