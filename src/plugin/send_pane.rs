//! Sending a pane's recent output to the default webhook on behalf of the agent in it.
use std::thread;

use super::Plugin;
use crate::auto::{Trigger, auto_message};
use crate::config::{Config, find_webhook};
use crate::env::{Run as _, System};
use crate::error::Result;
use crate::herdr::herdr_agent;
use crate::webhook::{NotifyStatus, Origin, build_notification, post_to_webhook};

/// What the webhook accepted from [`Plugin::send_pane`].
#[derive(Debug)]
pub(super) struct PaneSent {
    pub(super) http_status: u16,
    /// The agent the webhook's automation will reply to: its name, else the pane id.
    pub(super) target: String,
}

impl Plugin<'_> {
    /// Send the pane's recent output to the default webhook on behalf of its agent.
    pub(super) fn send_pane(
        &self,
        config: &Config,
        pane_id: &str,
        status: NotifyStatus,
        trigger: Trigger,
    ) -> Result<PaneSent> {
        let webhook = find_webhook(config, None)?;
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
        let message = auto_message(trigger, raw.as_deref().unwrap_or_default(), lines);
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
        let http_status = post_to_webhook(webhook, &notification)?;
        Ok(PaneSent { http_status, target })
    }
}
