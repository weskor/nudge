//! The on/off actions: nudge itself (`on`, `off`, `switch`), auto-notify (`enable`, `disable`, `toggle`) and away
//! mode (`away`, `away-on`, `away-off`).
use crate::error::Result;
use crate::plugin::Plugin;
use crate::state::update_state;

impl Plugin<'_> {
    /// Turn nudge on (`on`), off (`off`) or over (`switch`), and confirm with a toast. While off, nothing is sent.
    pub(super) fn switch_nudge(&self, action: &str) -> Result<()> {
        let on = update_state(self.env, |state| {
            state.on = if action == "switch" { !state.on } else { action == "on" };
            state.on
        })?;
        self.toast(if on {
            "Nudge on"
        } else {
            "Nudge off: nothing is sent from this machine"
        });
        Ok(())
    }

    /// Turn auto-notify on (`enable`), off (`disable`) or over (`toggle`), and confirm with a toast.
    pub(super) fn switch_auto(&self, action: &str) -> Result<()> {
        let auto = update_state(self.env, |state| {
            state.auto = if action == "toggle" {
                !state.auto
            } else {
                action == "enable"
            };
            state.auto
        })?;
        self.toast(if auto { "Auto-notify on" } else { "Auto-notify off" });
        Ok(())
    }

    /// Turn away mode on (`away-on`), off (`away-off`) or over (`away`), and confirm with a toast.
    pub(super) fn switch_away(&self, action: &str) -> Result<()> {
        let away = update_state(self.env, |state| {
            state.away = match action {
                "away-on" => true,
                "away-off" => false,
                _ => !state.away,
            };
            state.away
        })?;
        self.toast(if away {
            "Away: the webhook hears from every agent, even the pane on screen"
        } else {
            "Back: nudge stays quiet for the pane you're looking at"
        });
        Ok(())
    }
}
