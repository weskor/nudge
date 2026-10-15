//! herdr plugin entrypoints (see herdr-plugin.toml), reached through `nudge plugin …`:
//!   event          `pane.agent_status_changed` hook
//!   action <id>    enable | disable | toggle | ask-desk | setup | status
//!   startup        server start: keep `~/.local/bin/nudge` pointing at this install
//!
//! Decisions and failures go to stderr, which herdr keeps in `herdr plugin log list`.
use std::path::PathBuf;
use std::thread;

use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::config::{default_config_path, find_desk, load_config};
use crate::context::{Env, Run as _, System, herdr_agent};
use crate::error::{Error, Result};
use crate::install::{PLUGIN_ID, nudge_root, refresh_command_link};
use crate::notify::{NotifyStatus, Origin, build_notification, post_to_desk};
use crate::state::{now_ms, update_state};
use crate::watch::{Decision, HerdrStatus, Settled, TAIL_LINES, Trigger, auto_message, clean_tail, decide};

const USAGE: &str = "usage: nudge plugin event | action <id> | startup";

#[derive(Debug, Deserialize)]
struct StatusEvent {
    data: StatusEventData,
}

#[derive(Debug, Deserialize)]
struct StatusEventData {
    pane_id: String,
    agent_status: HerdrStatus,
}

#[derive(Debug, Default, Deserialize)]
struct ActionContext {
    focused_pane_id: Option<String>,
    focused_pane_status: Option<String>,
}

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

    /// Send the pane's recent output to the default desk on behalf of its agent. Returns the HTTP status.
    fn send_pane(&self, pane_id: &str, status: NotifyStatus, trigger: Trigger) -> Result<u16> {
        let config = load_config(&default_config_path(self.env))?;
        let desk = find_desk(&config, None)?;
        let (agent, raw) = thread::scope(|scope| {
            let read = [
                self.env.herdr(),
                "pane",
                "read",
                pane_id,
                "--source",
                "recent-unwrapped",
                "--lines",
                "200",
                "--format",
                "text",
            ];
            let raw = scope.spawn(move || System.run(&read, &self.home));
            let agent = herdr_agent(pane_id, self.env, &self.home, &System);
            (agent, raw.join().ok().flatten())
        });
        let message = auto_message(trigger, &clean_tail(raw.as_deref().unwrap_or_default(), TAIL_LINES));
        let home = self.home.to_string_lossy();
        let (target, cwd) = match agent {
            Some(agent) => (agent.target, agent.cwd.unwrap_or_else(|| home.into_owned())),
            None => (pane_id.to_owned(), home.into_owned()),
        };
        let origin = Origin {
            env: self.env,
            cwd: &cwd,
            run: &System,
        };
        let notification = build_notification(status.into(), &message, Some(target), origin)?;
        Ok(post_to_desk(desk, &notification)?)
    }

    fn on_status_changed(&self) -> Result<()> {
        let Some(event) = self.json_env::<StatusEvent>("HERDR_PLUGIN_EVENT_JSON") else {
            log("ignoring event without pane_id/agent_status");
            return Ok(());
        };
        let StatusEventData {
            pane_id,
            agent_status: status,
        } = event.data;
        let settles = matches!(status, HerdrStatus::Done | HerdrStatus::Blocked);
        let focused =
            settles && herdr_agent(&pane_id, self.env, &self.home, &System).is_some_and(|agent| agent.focused);
        let now = now_ms();
        let decision = update_state(self.env, |state| decide(state, &pane_id, status, now, focused))?;
        let settled = match decision {
            Decision::Skip(reason) => {
                if settles {
                    log(&format!("{pane_id} {}: skipped ({reason})", status.as_str()));
                }
                return Ok(());
            }
            Decision::Notify(settled) => settled,
        };
        let notify_status = match settled {
            Settled::Done => NotifyStatus::Done,
            Settled::Blocked => NotifyStatus::Blocked,
        };
        let http_status = self.send_pane(&pane_id, notify_status, Trigger::Settled(settled))?;
        log(&format!(
            "{pane_id} {}: notified desk (HTTP {http_status})",
            status.as_str()
        ));
        Ok(())
    }

    /// Open one of the plugin's overlay panes (declared in herdr-plugin.toml) over the active pane.
    fn open_pane(&self, entrypoint: &str) -> Result<()> {
        // herdr places overlays over the active pane and rejects an explicit --target-pane for them.
        let args = [
            self.env.herdr(),
            "plugin",
            "pane",
            "open",
            "--plugin",
            PLUGIN_ID,
            "--entrypoint",
            entrypoint,
            "--focus",
        ];
        System
            .run(&args, &self.home)
            .map(|_| ())
            .ok_or_else(|| Error::Other(format!("could not open the {entrypoint} pane")))
    }

    fn on_action(&self, action: &str) -> Result<()> {
        let context = self
            .json_env::<ActionContext>("HERDR_PLUGIN_CONTEXT_JSON")
            .unwrap_or_default();
        let pane_id = context
            .focused_pane_id
            .or_else(|| self.env.get("HERDR_PANE_ID").map(str::to_owned));
        match action {
            "enable" | "disable" | "toggle" => {
                let auto = update_state(self.env, |state| {
                    state.auto = if action == "toggle" {
                        !state.auto
                    } else {
                        action == "enable"
                    };
                    state.auto
                })?;
                self.toast(if auto { "Auto-notify on" } else { "Auto-notify off" });
            }
            "ask-desk" => {
                let Some(pane_id) = pane_id.filter(|id| !id.is_empty()) else {
                    self.toast("No pane to send");
                    return Ok(());
                };
                let blocked = context.focused_pane_status.as_deref() == Some("blocked");
                let status = if blocked {
                    NotifyStatus::Blocked
                } else {
                    NotifyStatus::Question
                };
                let http_status = self.send_pane(&pane_id, status, Trigger::Manual)?;
                self.toast(&format!("Sent this pane to the desk (HTTP {http_status})"));
            }
            "setup" | "status" => self.open_pane(action)?,
            other => return Err(Error::Other(format!("unknown action \"{other}\""))),
        }
        Ok(())
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
            if let Error::Config(message) | Error::Desk(crate::error::DeskError { message, .. }) = &error {
                plugin.toast(&format!(
                    "Desk notify failed: {}",
                    message.lines().next().unwrap_or_default()
                ));
            }
            1
        }
    }
}
