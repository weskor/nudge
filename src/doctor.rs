//! `nudge doctor`: check every piece and say how to fix what's broken.
use std::path::Path;

use crate::clipboard::CLIPBOARD_TOOLS;
use crate::config::{Auto, default_config_path, default_desk_name, read_config, seconds_in_words};
use crate::context::{Env, Run as _, System};
use crate::error::{Blame, Error, Result};
use crate::install::{PLUGIN_ID, Plugin, command_link, nudge_root, plugin, plugin_logs};
use crate::notify::{Origin, current_dir, ping_desk};
use crate::state::{current_state, now_ms, state_dir};
use crate::tty::{Mark, Style, wait_for_key};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Level {
    Ok,
    Warn,
    Fail,
}

#[derive(Debug)]
struct Check {
    level: Level,
    title: String,
    detail: Option<String>,
    fix: Option<String>,
}

impl Check {
    fn new(level: Level, title: impl Into<String>) -> Self {
        Self {
            level,
            title: title.into(),
            detail: None,
            fix: None,
        }
    }

    fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    fn fix(mut self, fix: impl Into<String>) -> Self {
        self.fix = Some(fix.into());
        self
    }

    fn print(&self) {
        let mark = match self.level {
            Level::Ok => Mark::Ok,
            Level::Warn => Mark::Warn,
            Level::Fail => Mark::Fail,
        };
        let detail = self
            .detail
            .as_ref()
            .map(|detail| format!("  {}", Style::Dim.paint(detail)))
            .unwrap_or_default();
        println!("{mark} {}{detail}", self.title);
        if let Some(fix) = &self.fix {
            println!("    {} {fix}", Style::Dim.paint("→"));
        }
    }
}

fn check_config(ping: bool, env: &Env) -> Result<Vec<Check>> {
    let path = default_config_path(env);
    let shown = path.display().to_string();
    let config = match read_config(&path) {
        Ok(Some(config)) => config,
        Ok(None) => {
            return Ok(vec![
                Check::new(Level::Fail, "No desk configured")
                    .detail(shown)
                    .fix("run nudge setup"),
            ]);
        }
        Err(Error::Config(message)) => {
            let fix = message.replace('\n', "\n      ");
            return Ok(vec![
                Check::new(Level::Fail, "Config is invalid").detail(shown).fix(fix),
            ]);
        }
        Err(other) => return Err(other),
    };
    let count = config.desk.len();
    let plural = if count == 1 { "" } else { "s" };
    let Some(default_name) = default_desk_name(&config) else {
        return Ok(vec![
            Check::new(Level::Warn, format!("Config: {count} desk{plural}, default not set"))
                .detail(shown)
                .fix("run nudge setup --desk NAME --default (the plugin sends to the default desk)"),
        ]);
    };
    let scope = if config.scope.is_everyone() {
        Check::new(Level::Ok, "Scope: all agents")
    } else {
        let list = |label: &str, names: &[String]| (!names.is_empty()).then(|| format!("{label} {}", names.join(", ")));
        let parts: Vec<String> = [
            list("workspaces", &config.scope.workspaces),
            list("agents", &config.scope.agents),
        ]
        .into_iter()
        .flatten()
        .collect();
        Check::new(Level::Ok, format!("Scope: {}", parts.join("; "))).fix(format!(
            "herdr plugin action invoke {PLUGIN_ID}.all-agents lets every agent notify"
        ))
    };
    let mut checks = vec![
        Check::new(
            Level::Ok,
            format!("Config: {count} desk{plural}, default \"{default_name}\""),
        )
        .detail(shown),
        scope,
    ];
    if !ping {
        checks.push(Check::new(Level::Ok, "Webhook not tested").detail("nudge doctor --ping sends a test ping"));
        return Ok(checks);
    }
    let cwd = current_dir()?;
    match ping_desk(
        &config,
        Some(default_name),
        None,
        Origin {
            env,
            cwd: &cwd,
            run: &System,
        },
    ) {
        Ok(sent) => checks.push(Check::new(
            Level::Ok,
            format!("Webhook accepted a test ping (HTTP {})", sent.http_status),
        )),
        Err(Error::Desk(error)) => {
            let fix = match error.blames() {
                Some(Blame::Authorization) => {
                    format!("copy the Authorization header again: nudge setup --desk {default_name}")
                }
                Some(Blame::Url) => format!("copy the Webhook URL again: nudge setup --desk {default_name}"),
                None => "check that the desk is running, then nudge test".to_owned(),
            };
            checks.push(
                Check::new(Level::Fail, "Webhook test failed")
                    .detail(error.message)
                    .fix(fix),
            );
        }
        Err(other) => return Err(other),
    }
    Ok(checks)
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

fn check_plugin(env: &Env) -> Vec<Check> {
    let Some(herdr_version) = System.run(&[env.herdr(), "--version"], Path::new("/")) else {
        return vec![
            Check::new(Level::Warn, "herdr not found")
                .fix("nudge notify works without herdr; the plugin and replies need https://herdr.dev"),
        ];
    };
    let info = match plugin(env, &System) {
        Plugin::HerdrUnavailable => {
            return vec![
                Check::new(Level::Warn, format!("{herdr_version}, but its server isn't reachable"))
                    .fix("start herdr, then run nudge doctor again"),
            ];
        }
        Plugin::NotInstalled => {
            return vec![
                Check::new(Level::Fail, "herdr plugin not installed")
                    .detail(herdr_version)
                    .fix("herdr plugin install weskor/nudge"),
            ];
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
    let mut checks = vec![status, build, auto];
    if saved.away {
        checks.push(
            Check::new(Level::Ok, "Away mode on")
                .detail("notifying even for the pane you're looking at")
                .fix(format!(
                    "when you're back: herdr plugin action invoke {PLUGIN_ID}.away-off"
                )),
        );
    }
    let logs = plugin_logs(env, &System);
    let recent: Vec<_> = logs.iter().filter(|log| !log.stderr.trim().is_empty()).collect();
    for log in recent.iter().skip(recent.len().saturating_sub(5)) {
        let line = log.stderr.trim().lines().next_back().unwrap_or_default();
        let line = line.strip_prefix("[nudge] ").unwrap_or(line);
        let level = if log.status == "succeeded" {
            Level::Ok
        } else {
            Level::Warn
        };
        checks.push(Check::new(level, format!("Recent: {line}")).detail(ago(log.started_unix_ms)));
    }
    checks
}

fn check_command(env: &Env) -> Check {
    if let Some(path) = env.which("nudge") {
        return Check::new(Level::Ok, "nudge command on PATH").detail(path.display().to_string());
    }
    let link = command_link(env);
    let shown = link.path.display().to_string();
    if link.exists && env.get("HERDR_PLUGIN_ID").is_some() {
        // Plugin panes get herdr's server PATH, which usually lacks ~/.local/bin; agents' shells don't.
        return Check::new(Level::Ok, "nudge command installed").detail(shown);
    }
    if link.exists {
        Check::new(Level::Warn, "nudge command not on PATH")
            .detail(format!("{shown} exists, but ~/.local/bin isn't on PATH"))
            .fix("add ~/.local/bin to PATH")
    } else {
        Check::new(Level::Warn, "nudge command not on PATH")
            .fix("nudge setup offers to install it (agents need it for nudge notify)")
    }
}

fn check_clipboard(env: &Env) -> Check {
    let found: Vec<&str> = CLIPBOARD_TOOLS
        .into_iter()
        .filter(|tool| env.which(tool).is_some())
        .collect();
    if found.is_empty() {
        Check::new(Level::Warn, "No clipboard tool")
            .detail("nudge setup falls back to typing values")
            .fix("install wl-clipboard or xclip")
    } else {
        Check::new(Level::Ok, format!("Clipboard: {}", found.join(", "))).detail("used by nudge setup")
    }
}

pub(crate) fn doctor(ping: bool, pane: bool, env: &Env) -> Result<u8> {
    println!("{}", Style::Bold.paint("nudge doctor"));
    println!(
        "{}",
        Style::Dim.paint(format!(
            "nudge at {}, state in {}",
            nudge_root(env).display(),
            state_dir(env).display()
        ))
    );
    println!();
    let mut checks = check_config(ping, env)?;
    checks.extend(check_plugin(env));
    checks.extend([check_command(env), check_clipboard(env)]);
    for check in &checks {
        check.print();
    }
    let failures = checks.iter().filter(|check| check.level == Level::Fail).count();
    println!();
    if failures == 0 {
        println!("{} All good.", Mark::Ok);
    } else {
        println!(
            "{} {failures} problem{} found.",
            Mark::Fail,
            if failures == 1 { "" } else { "s" }
        );
    }
    if pane {
        wait_for_key();
    }
    Ok(u8::from(failures != 0))
}
