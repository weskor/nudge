//! How a webhook's automation replies to a herdr TUI agent: `herdr agent prompt <target> "..."`.

/// The reply hint for `agent` (a name or pane id), shell-quoted when it needs to be.
pub(crate) fn reply_hint(agent: Option<&str>) -> String {
    let Some(agent) = agent else {
        return "sender's herdr agent could not be detected, so it cannot be prompted directly".to_owned();
    };
    let shell_safe = !agent.is_empty()
        && agent
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._:@%+=,-".contains(c));
    let target = if shell_safe {
        agent.to_owned()
    } else {
        format!("'{}'", agent.replace('\'', r"'\''"))
    };
    format!("reply with: herdr agent prompt {target} \"...\"")
}

#[cfg(test)]
mod tests;
