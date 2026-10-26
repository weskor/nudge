//! The pane actions: `send-pane` sends the focused pane to the webhook; `setup`, `status` and `settings` open
//! nudge's overlay panes.
use crate::auto::Trigger;
use crate::config::{default_config_path, load_config};
use crate::env::{Run as _, System};
use crate::error::{Error, Result};
use crate::herdr::PLUGIN_ID;
use crate::plugin::Plugin;
use crate::webhook::NotifyStatus;

impl Plugin<'_> {
    /// Send the focused pane to the webhook: a question, or blocked if herdr says the pane is blocked.
    pub(super) fn send_focused_pane(&self, pane_id: Option<String>, pane_status: Option<&str>) -> Result<()> {
        let Some(pane_id) = pane_id else {
            self.toast("No pane to send");
            return Ok(());
        };
        let blocked = pane_status == Some("blocked");
        let status = if blocked {
            NotifyStatus::Blocked
        } else {
            NotifyStatus::Question
        };
        // You chose to send this pane, so [scope] doesn't apply.
        let config = load_config(&default_config_path(self.env))?;
        let sent = self.send_pane(&config, &pane_id, status, Trigger::Manual)?;
        self.toast(&format!("Sent this pane to the webhook (HTTP {})", sent.http_status));
        Ok(())
    }

    /// Open one of the plugin's overlay panes (declared in herdr-plugin.toml) over the active pane.
    pub(super) fn open_pane(&self, entrypoint: &str) -> Result<()> {
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
}
