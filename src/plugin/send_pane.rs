//! Sending a pane's recent output to its webhook (its route's, else the default) on behalf of the agent in it.
use std::thread;

use super::Plugin;
use crate::auto::{Trigger, auto_message};
use crate::config::{Backend, Config, Webhook, find_webhook, routes_name_workspaces};
use crate::env::{Run as _, System};
use crate::error::Result;
use crate::herdr::{AgentInfo, Sender, SenderIds, herdr_agent};
use crate::webhook::{NotifyStatus, Origin, build_notification, deliver};

/// What the webhook accepted from [`Plugin::send_pane`].
#[derive(Debug)]
pub(super) struct PaneSent {
    pub(super) http_status: u16,
    /// The agent the webhook's automation will reply to: its name, else the pane id.
    pub(super) target: String,
}

impl Plugin<'_> {
    /// Send the pane's recent output to its webhook on behalf of its agent. `ids`: the sender's names and ids, when
    /// the caller already looked them up (with the workspace if routes name one).
    pub(super) fn send_pane(
        &self,
        config: &Config,
        pane_id: &str,
        status: NotifyStatus,
        trigger: Trigger,
        ids: Option<&SenderIds>,
    ) -> Result<PaneSent> {
        // Without routes it's the default webhook: checked first, so a config problem shows before herdr is asked.
        let fixed = config
            .route
            .is_empty()
            .then(|| find_webhook(config, None))
            .transpose()?;
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
        let webhook = match fixed {
            Some(webhook) => webhook,
            None => self.routed_webhook(config, pane_id, agent.as_ref(), ids)?,
        };
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
        let notification = build_notification(status.into(), &message, Some(target.clone()), &webhook.name, origin)?;
        let http_status = deliver(self.env, webhook, &notification)?;
        Ok(PaneSent { http_status, target })
    }

    /// The webhook the route for the agent in `pane_id` names, else the default webhook.
    fn routed_webhook<'a>(
        &self,
        config: &'a Config,
        pane_id: &str,
        agent: Option<&AgentInfo>,
        ids: Option<&SenderIds>,
    ) -> Result<&'a Webhook> {
        let looked_up;
        let ids = if let Some(ids) = ids {
            ids
        } else {
            let sender = agent.map_or_else(|| Sender::in_pane(pane_id), |agent| agent.sender.clone());
            let workspace = routes_name_workspaces(&config.route, Backend::Tui);
            looked_up = SenderIds::lookup(&sender, workspace, self.env, &System);
            &looked_up
        };
        find_webhook(config, ids.route(&config.route).map(|route| route.webhook.as_str()))
    }
}
