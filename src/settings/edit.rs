//! Changing the value on a row: stepping a number through its presets, or flipping, picking or ticking a choice.
use super::presets::{COOLDOWN_STEPS, TAIL_STEPS, TURN_STEPS, cycle, step};
use super::rows::Row;
use super::state::{Edits, Settings};

impl Settings {
    fn changed(&mut self) {
        self.edits = Edits::Unsaved;
        self.status = None;
    }

    pub(super) fn adjust(&mut self, row: Row, up: bool) {
        let auto = &mut self.auto;
        let (value, steps) = match row {
            Row::MinTurn => (&mut auto.min_turn_seconds, &TURN_STEPS[..]),
            Row::Cooldown => (&mut auto.cooldown_seconds, &COOLDOWN_STEPS[..]),
            Row::TailLines => (&mut auto.tail_lines, &TAIL_STEPS[..]),
            Row::AutoOn
            | Row::Away
            | Row::Toast
            | Row::Everyone
            | Row::OnlyThese
            | Row::Workspace(_)
            | Row::Agent(_)
            | Row::Webhook(_) => return,
        };
        let next = step(*value, steps, up);
        if next != *value {
            *value = next;
            self.changed();
        }
    }

    pub(super) fn toggle(&mut self, row: Row) {
        match row {
            Row::AutoOn => self.auto_on = !self.auto_on,
            Row::Away => self.away = !self.away,
            Row::Toast => self.auto.toast = !self.auto.toast,
            Row::MinTurn => self.auto.min_turn_seconds = cycle(self.auto.min_turn_seconds, &TURN_STEPS),
            Row::Cooldown => self.auto.cooldown_seconds = cycle(self.auto.cooldown_seconds, &COOLDOWN_STEPS),
            Row::TailLines => self.auto.tail_lines = cycle(self.auto.tail_lines, &TAIL_STEPS),
            Row::Everyone => self.only = false,
            Row::OnlyThese => self.only = true,
            Row::Workspace(index) => {
                let Some(choice) = self.workspaces.get_mut(index) else {
                    return;
                };
                choice.checked = !choice.checked;
            }
            Row::Agent(index) => {
                let Some(choice) = self.agents.get_mut(index) else {
                    return;
                };
                choice.checked = !choice.checked;
            }
            Row::Webhook(index) => {
                let Some(webhook) = self.webhooks.get(index) else {
                    return;
                };
                self.default_webhook = Some(webhook.name.clone());
            }
        }
        self.changed();
    }
}

#[cfg(test)]
mod tests;
