//! herdr plugin entrypoints (see herdr-plugin.toml), reached through `nudge plugin …`:
//!   event          `pane.agent_status_changed` hook
//!   action <id>    enable | disable | toggle | away | away-on | away-off | send-pane | setup | status | settings |
//!                  only-workspace | only-agent | all-agents
//!   startup        server start: keep `~/.local/bin/nudge` pointing at this install
//!
//! Decisions and failures go to stderr, which herdr keeps in `herdr plugin log list`.
//! This file owns the entry: argument dispatch, exit code, and the [`Plugin`] every entrypoint runs on.
mod actions;
mod send_pane;
mod status;

use std::path::PathBuf;

use serde::de::DeserializeOwned;

use crate::env::{Env, Run as _, System};
use crate::error::{Error, Result};
use crate::herdr::{nudge_root, refresh_command_link};

const USAGE: &str = "usage: nudge plugin event | action <id> | startup";

fn log(message: &str) {
    eprintln!("[nudge] {message}");
}

/// What a plugin entrypoint needs: the environment herdr gave it.
#[derive(Debug)]
struct Plugin<'a> {
    env: &'a Env,
    home: PathBuf,
}

impl Plugin<'_> {
    fn json_env<T: DeserializeOwned>(&self, name: &str) -> Option<T> {
        serde_json::from_str(self.env.get(name)?).ok()
    }

    fn toast(&self, body: &str) {
        let _shown = System.run(
            &[self.env.herdr(), "notification", "show", "nudge", "--body", body],
            &self.home,
        );
    }

    fn run(&self, args: &[String]) -> Result<()> {
        match args {
            [mode] if mode == "event" => self.on_status_changed(),
            [mode, action] if mode == "action" => self.on_action(action),
            [mode] if mode == "startup" => {
                if refresh_command_link(&nudge_root(self.env), self.env)? {
                    log("repointed ~/.local/bin/nudge at this install");
                }
                Ok(())
            }
            _ => Err(Error::Other(USAGE.into())),
        }
    }
}

/// Run a plugin entrypoint; returns the exit code.
pub(crate) fn run_plugin(args: &[String], env: &Env) -> u8 {
    let home = env.get("HOME").map_or_else(|| PathBuf::from("/"), PathBuf::from);
    let plugin = Plugin { env, home };
    match plugin.run(args) {
        Ok(()) => 0,
        Err(error) => {
            log(&format!("failed: {error}"));
            if let Error::Config(message) | Error::Webhook(crate::error::WebhookError { message, .. }) = &error {
                plugin.toast(&format!(
                    "Webhook notify failed: {}",
                    message.lines().next().unwrap_or_default()
                ));
            }
            1
        }
    }
}
