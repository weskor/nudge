//! How a webhook's automation replies to a herdr TUI agent: `herdr agent prompt <target> "..."`.
use crate::source::shell_word;

/// The reply hint for `agent` (a name or pane id), shell-quoted when it needs to be.
pub(crate) fn reply_hint(agent: Option<&str>) -> String {
    let Some(agent) = agent else {
        return "sender's herdr agent could not be detected, so it cannot be prompted directly".to_owned();
    };
    format!("reply with: herdr agent prompt {} \"...\"", shell_word(agent))
}

#[cfg(test)]
mod tests;
