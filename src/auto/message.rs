//! The message for a notification built from a pane: a lead line saying why, then the pane's last lines,
//! cleaned up and redacted.
use std::sync::LazyLock;

use regex::Regex;

use super::redact::{redact, regex};
use super::rules::Settled;

const TAIL_MAX_CHARS: usize = 4_000;

static ANSI: LazyLock<Regex> = LazyLock::new(|| regex(r"\x1b\[[0-9;?]*[ -/]*[@-~]"));
static RULE_LINE: LazyLock<Regex> = LazyLock::new(|| regex(r"^[\s─━═│┃┄┅┈┉╌╍—_=~·•-]+$"));

/// The last `lines` meaningful lines of a pane, redacted: no ANSI, no box-drawing rules, no blank runs. The whole
/// capture is redacted before it's cut, so a secret that starts above the cut (a private key's BEGIN line) is
/// still recognized.
fn clean_tail(raw: &str, lines: usize) -> String {
    let plain = redact(&ANSI.replace_all(raw, ""));
    let mut kept: Vec<&str> = Vec::new();
    for line in plain.split('\n') {
        let trimmed = line.strip_suffix('\r').unwrap_or(line).trim_end();
        if !trimmed.trim().is_empty() && RULE_LINE.is_match(trimmed) {
            continue;
        }
        if trimmed.is_empty() && kept.last().is_none_or(|last| last.is_empty()) {
            continue;
        }
        kept.push(trimmed);
    }
    while kept.last().is_some_and(|last| last.is_empty()) {
        kept.pop();
    }
    let tail = kept
        .get(kept.len().saturating_sub(lines)..)
        .unwrap_or_default()
        .join("\n");
    let chars = tail.chars().count();
    if chars > TAIL_MAX_CHARS {
        format!("…{}", tail.chars().skip(chars - TAIL_MAX_CHARS).collect::<String>())
    } else {
        tail
    }
}

/// Why a notification built from pane output is being sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Trigger {
    Settled(Settled),
    /// The user sent the pane with the `send-pane` action.
    Manual,
}

/// Message for an automatic or on-demand notification built from the pane's output (`raw`, as captured): its
/// last `lines` meaningful lines, secrets redacted.
pub(crate) fn auto_message(trigger: Trigger, raw: &str, lines: usize) -> String {
    let lead = match trigger {
        Trigger::Settled(Settled::Done) => {
            "Agent finished a turn (auto-detected by herdr; this message was not written by the agent)."
        }
        Trigger::Settled(Settled::Blocked) => {
            "Agent is waiting for approval or input (auto-detected by herdr; this message was not written by the agent)."
        }
        Trigger::Manual => {
            "The user asked you to look at this agent (sent from herdr; this message was not written by the agent)."
        }
    };
    let tail = clean_tail(raw, lines);
    let output = tail.trim();
    if output.is_empty() {
        format!("{lead}\nNo terminal output captured.")
    } else {
        format!("{lead}\nLast terminal output (secrets redacted):\n\n{output}")
    }
}

#[cfg(test)]
mod tests;
