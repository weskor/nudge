//! The rows the cursor can stop on, in screen order, and what each one does.
use super::state::Settings;

/// A line the cursor can sit on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Row {
    AutoOn,
    Away,
    Toast,
    MinTurn,
    Cooldown,
    TailLines,
    Everyone,
    OnlyThese,
    Workspace(usize),
    Agent(usize),
    Webhook(usize),
}

/// What the row under the cursor does, shown under the list.
pub(super) const fn describe(row: Row) -> &'static str {
    match row {
        Row::AutoOn => {
            "Tell the webhook when an agent is blocked, or done after a long turn, in a pane you aren't looking at."
        }
        Row::Away => "Count the pane on screen as unwatched too, and notify even with auto-notify off.",
        Row::Toast => "Show a herdr toast each time an automatic notification goes out.",
        Row::MinTurn => "Skip done turns shorter than this. Blocked agents always count.",
        Row::Cooldown => "At most one automatic notification per pane in this window; a brake on reply loops.",
        Row::TailLines => "How many of the pane's last lines go with the event, with credentials redacted.",
        Row::Everyone => "Any agent in herdr can reach the webhook.",
        Row::OnlyThese => "Only agents in the ticked workspaces, and the ticked agents, can reach the webhook.",
        Row::Workspace(_) => "Agents in this workspace can notify.",
        Row::Agent(_) => "This agent can notify, whichever workspace it is in.",
        Row::Webhook(_) => "Notifications go here unless they name another webhook. t sends a test ping.",
    }
}

impl Settings {
    pub(super) fn rows(&self) -> Vec<Row> {
        let mut rows = vec![
            Row::AutoOn,
            Row::Away,
            Row::Toast,
            Row::MinTurn,
            Row::Cooldown,
            Row::TailLines,
            Row::Everyone,
            Row::OnlyThese,
        ];
        // The lists only matter for "Only these", so they are hidden (and not cursor stops) for "Every agent".
        if self.only {
            rows.extend((0..self.workspaces.len()).map(Row::Workspace));
            rows.extend((0..self.agents.len()).map(Row::Agent));
        }
        rows.extend((0..self.webhooks.len()).map(Row::Webhook));
        rows
    }

    pub(super) fn current(&self) -> Option<Row> {
        self.rows().get(self.cursor).copied()
    }
}

#[cfg(test)]
mod tests;
