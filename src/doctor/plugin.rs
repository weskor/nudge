//! herdr plugin checks: herdr itself, the plugin's install, auto-notify and away mode, and recent plugin runs.
use std::path::Path;

use super::check::{Check, Level};
use crate::config::{Auto, default_config_path, read_config, seconds_in_words};
use crate::env::{Env, Run as _, System};
use crate::herdr::{PLUGIN_ID, Plugin, plugin, plugin_logs};
use crate::state::{current_state, now_ms};

pub(super) fn check_plugin(env: &Env) -> Vec<Check> {
    let Some(herdr_version) = System.run(&[env.herdr(), "--version"], Path::new("/")) else {
        return vec![
            Check::new(Level::Warn, "herdr not found")
                .fix("nudge notify works without herdr; the plugin and replies need https://herdr.dev"),
        ];
    };
    let mut checks = match check_install(plugin(env, &System), herdr_version) {
        Ok(checks) => Vec::from(checks),
        Err(check) => return vec![check],
    };
    checks.extend(check_auto(env));
    checks.extend(check_recent_runs(env));
    checks
}

/// Whether the plugin is installed and enabled, and which binary it runs; the one failed check when herdr's
/// server can't be reached or the plugin isn't installed.
fn check_install(plugin: Plugin, herdr_version: String) -> Result<[Check; 2], Check> {
    let info = match plugin {
        Plugin::HerdrUnavailable => {
            return Err(
                Check::new(Level::Warn, format!("{herdr_version}, but its server isn't reachable"))
                    .fix("start herdr, then run nudge doctor again"),
            );
        }
        Plugin::NotInstalled => {
            return Err(Check::new(Level::Fail, "herdr plugin not installed")
                .detail(herdr_version)
                .fix("herdr plugin install weskor/nudge"));
        }
        Plugin::Installed(info) => info,
    };
    let state = if info.enabled { "enabled" } else { "disabled" };
    let mut status = Check::new(
        if info.enabled { Level::Ok } else { Level::Fail },
        format!("herdr plugin {PLUGIN_ID} {} {state}", info.version),
    )
    .detail(&info.plugin_root);
    if !info.enabled {
        status = status.fix(format!("herdr plugin enable {PLUGIN_ID}"));
    }
    let binary = Path::new(&info.plugin_root).join("dist").join("nudge");
    let build = if binary.exists() {
        Check::new(Level::Ok, "Plugin runs the prebuilt binary").detail(binary.display().to_string())
    } else {
        Check::new(Level::Ok, "Plugin runs a local build").detail("no dist/nudge (linked checkout)")
    };
    Ok([status, build])
}

/// Auto-notify on or off (with the configured turn length), and away mode when it's on.
fn check_auto(env: &Env) -> Vec<Check> {
    let rules = read_config(&default_config_path(env))
        .ok()
        .flatten()
        .map_or_else(Auto::default, |config| config.auto);
    let saved = current_state(env);
    let auto = if saved.auto {
        Check::new(Level::Ok, "Auto-notify on").detail(format!(
            "blocked, or done after {} or more while you're not watching",
            seconds_in_words(rules.min_turn_seconds)
        ))
    } else {
        Check::new(Level::Warn, "Auto-notify off").fix(format!("herdr plugin action invoke {PLUGIN_ID}.enable"))
    };
    let mut checks = vec![auto];
    if saved.away {
        checks.push(
            Check::new(Level::Ok, "Away mode on")
                .detail("notifying even for the pane you're looking at")
                .fix(format!(
                    "when you're back: herdr plugin action invoke {PLUGIN_ID}.away-off"
                )),
        );
    }
    checks
}

/// The last line of the five most recent plugin runs that wrote to stderr.
fn check_recent_runs(env: &Env) -> Vec<Check> {
    let logs = plugin_logs(env, &System);
    let recent: Vec<_> = logs.iter().filter(|log| !log.stderr.trim().is_empty()).collect();
    recent
        .iter()
        .skip(recent.len().saturating_sub(5))
        .map(|log| {
            let line = log.stderr.trim().lines().next_back().unwrap_or_default();
            let line = line.strip_prefix("[nudge] ").unwrap_or(line);
            let level = if log.status == "succeeded" {
                Level::Ok
            } else {
                Level::Warn
            };
            Check::new(level, format!("Recent: {line}")).detail(ago(log.started_unix_ms))
        })
        .collect()
}

/// How long ago `unix_ms` was, e.g. "5m ago".
fn ago(unix_ms: i64) -> String {
    let seconds = now_ms().saturating_sub(unix_ms) / 1000;
    match seconds {
        ..60 => "just now".to_owned(),
        60..3_600 => format!("{}m ago", seconds / 60),
        3_600..86_400 => format!("{}h ago", seconds / 3_600),
        86_400.. => format!("{}d ago", seconds / 86_400),
    }
}
