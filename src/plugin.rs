//! herdr plugin entrypoints (see herdr-plugin.toml), reached through `nudge plugin …`:
//!   event          `pane.agent_status_changed` hook
//!   action <id>    enable | disable | toggle | ask-desk | setup | status | settings | only-workspace | only-agent |
//!                  all-agents
//!   startup        server start: keep `~/.local/bin/nudge` pointing at this install
//!
//! Decisions and failures go to stderr, which herdr keeps in `herdr plugin log list`.
use std::path::PathBuf;
use std::thread;

use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::config::{Auto, Config, Scope, default_config_path, find_desk, load_config, read_config, write_config};
use crate::context::{Env, Run as _, Sender, System, herdr_agent, in_scope, pane_workspace, workspace_label};
use crate::error::{Error, Result};
use crate::install::{PLUGIN_ID, nudge_root, refresh_command_link};
use crate::notify::{NotifyStatus, Origin, build_notification, post_to_desk};
use crate::state::{now_ms, update_state};
use crate::watch::{Decision, HerdrStatus, Settled, Trigger, auto_message, clean_tail, decide};

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

/// What the desk accepted from [`Plugin::send_pane`].
#[derive(Debug)]
struct PaneSent {
    http_status: u16,
    /// The agent the desk will reply to: its name, else the pane id.
    target: String,
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

    /// Send the pane's recent output to the default desk on behalf of its agent.
    fn send_pane(&self, config: &Config, pane_id: &str, status: NotifyStatus, trigger: Trigger) -> Result<PaneSent> {
        let desk = find_desk(config, None)?;
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
        let lines = usize::try_from(config.auto.tail_lines).unwrap_or(usize::MAX);
        let message = auto_message(trigger, &clean_tail(raw.as_deref().unwrap_or_default(), lines));
        let home = self.home.to_string_lossy();
        let (target, cwd) = match agent {
            Some(agent) => (
                agent.sender.agent.unwrap_or_else(|| pane_id.to_owned()),
                agent.cwd.unwrap_or_else(|| home.into_owned()),
            ),
            None => (pane_id.to_owned(), home.into_owned()),
        };
        let origin = Origin {
            env: self.env,
            cwd: &cwd,
            run: &System,
        };
        let notification = build_notification(status.into(), &message, Some(target.clone()), origin)?;
        let http_status = post_to_desk(desk, &notification)?;
        Ok(PaneSent { http_status, target })
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
        let path = default_config_path(self.env);
        // A missing or broken config only matters once there is something to send; that load reports it.
        let config = if settles {
            read_config(&path).ok().flatten()
        } else {
            None
        };
        let rules = config.as_ref().map_or_else(Auto::default, |config| config.auto);
        let agent = if settles {
            herdr_agent(&pane_id, self.env, &self.home, &System)
        } else {
            None
        };
        let focused = agent.as_ref().is_some_and(|agent| agent.focused);
        let now = now_ms();
        let decision = update_state(self.env, |state| decide(state, &pane_id, status, now, focused, rules))?;
        let settled = match decision {
            Decision::Skip(reason) => {
                if settles {
                    log(&format!("{pane_id} {}: skipped ({reason})", status.as_str()));
                }
                return Ok(());
            }
            Decision::Notify(settled) => settled,
        };
        let config = match config {
            Some(config) => config,
            None => load_config(&path)?,
        };
        let sender = agent.map_or_else(
            || Sender {
                agent: None,
                workspace_id: None,
                pane_id: Some(pane_id.clone()),
            },
            |agent| agent.sender,
        );
        if !in_scope(&config.scope, &sender, self.env, &System) {
            log(&format!(
                "{pane_id} {}: skipped (outside nudge's scope)",
                status.as_str()
            ));
            return Ok(());
        }
        let (notify_status, done) = match settled {
            Settled::Done => (NotifyStatus::Done, "finished a turn"),
            Settled::Blocked => (NotifyStatus::Blocked, "is blocked"),
        };
        let sent = self.send_pane(&config, &pane_id, notify_status, Trigger::Settled(settled))?;
        log(&format!(
            "{pane_id} {}: notified desk (HTTP {})",
            status.as_str(),
            sent.http_status
        ));
        if config.auto.toast {
            self.toast(&format!("Told the desk: {} {done}", sent.target));
        }
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

    /// Replace the config's `[scope]` and confirm with a toast.
    fn set_scope(&self, scope: Scope, confirmation: &str) -> Result<()> {
        let path = default_config_path(self.env);
        let mut config = load_config(&path)?;
        config.scope = scope;
        write_config(&path, &config)?;
        self.toast(confirmation);
        Ok(())
    }

    fn on_action(&self, action: &str) -> Result<()> {
        let context = self
            .json_env::<ActionContext>("HERDR_PLUGIN_CONTEXT_JSON")
            .unwrap_or_default();
        let pane_id = context
            .focused_pane_id
            .or_else(|| self.env.get("HERDR_PANE_ID").map(str::to_owned))
            .filter(|id| !id.is_empty());
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
                let Some(pane_id) = pane_id else {
                    self.toast("No pane to send");
                    return Ok(());
                };
                let blocked = context.focused_pane_status.as_deref() == Some("blocked");
                let status = if blocked {
                    NotifyStatus::Blocked
                } else {
                    NotifyStatus::Question
                };
                // You chose to send this pane, so [scope] doesn't apply.
                let config = load_config(&default_config_path(self.env))?;
                let sent = self.send_pane(&config, &pane_id, status, Trigger::Manual)?;
                self.toast(&format!("Sent this pane to the desk (HTTP {})", sent.http_status));
            }
            "setup" | "status" | "settings" => self.open_pane(action)?,
            "only-workspace" => {
                let Some(workspace_id) = pane_id.and_then(|pane| pane_workspace(&pane, self.env, &self.home, &System))
                else {
                    self.toast("No workspace for this pane");
                    return Ok(());
                };
                let name = workspace_label(&workspace_id, self.env, &self.home, &System).unwrap_or(workspace_id);
                let confirmation = format!("Only workspace \"{name}\" can notify the desk");
                self.set_scope(
                    Scope {
                        workspaces: vec![name],
                        agents: Vec::new(),
                    },
                    &confirmation,
                )?;
            }
            "only-agent" => {
                let agent = pane_id
                    .and_then(|pane| herdr_agent(&pane, self.env, &self.home, &System))
                    .and_then(|agent| agent.sender.agent);
                let Some(name) = agent else {
                    self.toast("No agent in this pane");
                    return Ok(());
                };
                let confirmation = format!("Only agent \"{name}\" can notify the desk");
                self.set_scope(
                    Scope {
                        workspaces: Vec::new(),
                        agents: vec![name],
                    },
                    &confirmation,
                )?;
            }
            "all-agents" => self.set_scope(Scope::default(), "All agents can notify the desk")?,
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
