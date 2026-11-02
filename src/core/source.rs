//! herdr-core as an [`AgentSource`], through the `hc` CLI: a terminal as an `AgentRef`, its output (`hc read`),
//! the reply hint (`hc send`), and who runs `nudge notify` inside a core terminal (`$HC_TERMINAL_ID`).
use std::path::Path;

use super::context::{name_space, read_app_context};
use super::events::{SPACE_LABEL, TerminalRecord};
use crate::env::{Env, Run};
use crate::source::{AgentRef, AgentSource, Backend, WorkspaceRef, shell_word};

/// The `hc` CLI, from PATH.
pub(super) const HC: &str = "hc";

impl TerminalRecord {
    /// This terminal as the pipeline sees it: its address, name, space (the `herdr.space` label) and directory.
    pub(super) fn agent(&self) -> AgentRef {
        let non_empty = |text: &Option<String>| text.clone().filter(|text| !text.is_empty());
        AgentRef {
            backend: Backend::Core,
            pane: self.address.clone(),
            name: non_empty(&self.name),
            workspace: self.labels.get(SPACE_LABEL).map(|id| WorkspaceRef {
                id: id.clone(),
                name: None,
            }),
            cwd: non_empty(&self.last_cwd).or_else(|| non_empty(&self.cwd)),
        }
    }
}

/// herdr-core, reached through `hc` from `home`.
#[derive(Clone, Copy)]
pub(crate) struct CoreSource<'a> {
    pub(crate) home: &'a Path,
    pub(crate) run: &'a dyn Run,
}

impl CoreSource<'_> {
    /// The terminal at `address` (or with that name), as `hc get` describes it; `None` if hc can't find it.
    pub(crate) fn agent(&self, address: &str) -> Option<AgentRef> {
        let json = self.run.run(&[HC, "get", address, "--json"], self.home)?;
        serde_json::from_str::<TerminalRecord>(&json)
            .ok()
            .map(|record| record.agent())
    }
}

/// How the webhook's automation replies to the core terminal at `address`.
pub(crate) fn core_reply_hint(address: &str) -> String {
    format!("reply with: hc send {} \"...\"", shell_word(address))
}

impl AgentSource for CoreSource<'_> {
    /// `hc read` with history; its own `[hc] …` notes (only the screen while working, …) are not terminal output.
    fn tail(&self, agent: &AgentRef, lines: usize) -> Option<String> {
        let lines = lines.to_string();
        let text = self.run.run(&[HC, "read", &agent.pane, "--lines", &lines], self.home)?;
        let output: Vec<&str> = text.lines().filter(|line| !line.starts_with("[hc] ")).collect();
        Some(output.join("\n"))
    }

    fn reply_hint(&self, agent: &AgentRef) -> String {
        core_reply_hint(&agent.pane)
    }

    /// The space from the terminal's label, named from the app context by whoever built `agent`; core itself knows
    /// no space names.
    fn workspace(&self, agent: &AgentRef) -> Option<WorkspaceRef> {
        agent.workspace.clone()
    }

    /// herdr-core has no toasts.
    fn toast(&self, _text: &str) {}
}

/// The core terminal `nudge notify` runs in (`$HC_TERMINAL_ID`), unless it runs in a herdr TUI pane: as hc
/// describes it (its space named from the app context), else just its address.
pub(crate) fn core_sender(env: &Env, run: &dyn Run) -> Option<AgentRef> {
    if env.get("HERDR_PANE_ID").is_some() {
        return None;
    }
    let address = env.get("HC_TERMINAL_ID").filter(|id| !id.is_empty())?;
    let source = CoreSource {
        home: Path::new("/"),
        run,
    };
    let mut agent = source
        .agent(address)
        .unwrap_or_else(|| AgentRef::in_pane(Backend::Core, address.to_owned()));
    name_space(&mut agent, read_app_context(env).as_ref());
    Some(agent)
}

#[cfg(test)]
mod tests;
