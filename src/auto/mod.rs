//! Pure logic for the herdr plugin: which agent status changes become webhook notifications, and how the
//! pane's terminal output is trimmed and redacted before it leaves the machine.
use std::sync::LazyLock;

use regex::Regex;
use serde::Deserialize;

use crate::config::Auto;
use crate::error::unreachable_regex;
use crate::state::State;

const TAIL_MAX_CHARS: usize = 4_000;
const REDACTED: &str = "[REDACTED]";

/// An agent status as herdr reports it in `pane.agent_status_changed`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum HerdrStatus {
    Idle,
    Working,
    Blocked,
    Done,
    Unknown,
}

impl HerdrStatus {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Working => "working",
            Self::Blocked => "blocked",
            Self::Done => "done",
            Self::Unknown => "unknown",
        }
    }
}

/// A status that ends a turn and can be worth telling the webhook about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Settled {
    Done,
    Blocked,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Decision {
    Notify(Settled),
    Skip(String),
}

/// Apply one `pane.agent_status_changed` event to `state` and decide whether to notify.
/// - `working` starts a turn; `idle` (seen by you) and `done` end it.
/// - `blocked` always qualifies; `done` only after a turn of at least `rules.min_turn_seconds` with a known start.
/// - Skipped when auto-notify is off, you are looking at the pane (`focused`), the agent already ran
///   `nudge notify` this turn, or the pane is in its `rules.cooldown_seconds` cooldown.
/// - Away mode (`state.away`) overrides the first two: you said you're away.
pub(crate) fn decide(
    state: &mut State,
    pane_id: &str,
    status: HerdrStatus,
    now: i64,
    focused: bool,
    rules: Auto,
) -> Decision {
    let pane = state.panes.entry(pane_id.to_owned()).or_default();
    let turn_start = pane.working_since;
    let settled = match status {
        HerdrStatus::Working => {
            pane.working_since.get_or_insert(now);
            return Decision::Skip("turn started".into());
        }
        HerdrStatus::Idle | HerdrStatus::Unknown => {
            pane.working_since = None;
            return Decision::Skip(format!("status {}", status.as_str()));
        }
        HerdrStatus::Done => {
            pane.working_since = None;
            Settled::Done
        }
        HerdrStatus::Blocked => Settled::Blocked,
    };

    if !state.auto && !state.away {
        return Decision::Skip("auto-notify is off".into());
    }
    if focused && !state.away {
        return Decision::Skip("you are looking at this pane".into());
    }
    let self_notified_since = turn_start.unwrap_or(now - rules.min_turn_ms());
    if pane.self_notified_at.is_some_and(|at| at >= self_notified_since) {
        return Decision::Skip("agent already notified the webhook this turn".into());
    }
    if settled == Settled::Done {
        let Some(start) = turn_start else {
            return Decision::Skip("turn start unknown".into());
        };
        if now - start < rules.min_turn_ms() {
            return Decision::Skip(format!("turn shorter than {}s", rules.min_turn_seconds));
        }
    }
    if pane.last_auto_at.is_some_and(|at| now - at < rules.cooldown_ms()) {
        return Decision::Skip("cooldown".into());
    }
    pane.last_auto_at = Some(now);
    Decision::Notify(settled)
}

fn regex(pattern: &str) -> Regex {
    Regex::new(pattern).unwrap_or_else(|error| unreachable_regex(&error))
}

static PRIVATE_KEY: LazyLock<Regex> =
    LazyLock::new(|| regex(r"(?s)-----BEGIN [A-Z ]*PRIVATE KEY-----.*?(?:-----END [A-Z ]*PRIVATE KEY-----|\z)"));
/// The end of a key whose BEGIN line scrolled out of view: the base64 lines before an END line.
static PRIVATE_KEY_END: LazyLock<Regex> =
    LazyLock::new(|| regex(r"(?m)(?:^[ \t]*[A-Za-z0-9+/=]{4,}[ \t]*\r?\n)*^[ \t]*-----END [A-Z ]*PRIVATE KEY-----"));
static AUTH_SCHEME: LazyLock<Regex> = LazyLock::new(|| regex(r"(?i)\b(Bearer|Basic|Token)\s+[A-Za-z0-9._~+/=-]{8,}"));
static ASSIGNMENT: LazyLock<Regex> = LazyLock::new(|| {
    regex(
        r#"(?i)\b([A-Za-z0-9_]*(?:secret|token|passw(?:or)?d|api[_-]?key|private[_-]?key|credential)[A-Za-z0-9_]*)(\s*[=:]\s*)("[^"]*"|'[^']*'|\S+)"#,
    )
});
static KNOWN_TOKEN: LazyLock<Regex> = LazyLock::new(|| {
    regex(
        r"\b(?:sk-[A-Za-z0-9_-]{16,}|gh[pousr]_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,}|xox[abprs]-[A-Za-z0-9-]{10,}|AKIA[0-9A-Z]{16}|AIza[A-Za-z0-9_-]{30,}|eyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,})",
    )
});
static ANSI: LazyLock<Regex> = LazyLock::new(|| regex(r"\x1b\[[0-9;?]*[ -/]*[@-~]"));
static RULE_LINE: LazyLock<Regex> = LazyLock::new(|| regex(r"^[\s─━═│┃┄┅┈┉╌╍—_=~·•-]+$"));

const fn is_token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

/// Long random-looking tokens: runs of 32+ token characters mixing letters and digits, not part of a path
/// or file name. Pure hex (commit SHAs) is kept.
fn redact_long_tokens(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(is_token_char) {
        let (before, from_token) = rest.split_at(start);
        let end = from_token.find(|c: char| !is_token_char(c)).unwrap_or(from_token.len());
        let (token, after) = from_token.split_at(end);
        let previous_char = before.chars().next_back();
        let in_path = |c: Option<char>| matches!(c, Some('/' | '.'));
        let random = token.len() >= 32
            && !in_path(previous_char)
            && !in_path(after.chars().next())
            && !token.chars().all(|c| c.is_ascii_hexdigit())
            && token.chars().any(|c| c.is_ascii_digit())
            && token.chars().any(|c| c.is_ascii_alphabetic());
        out.push_str(before);
        out.push_str(if random { REDACTED } else { token });
        rest = after;
    }
    out.push_str(rest);
    out
}

/// Replace things that look like credentials. Errs toward over-redacting; this text goes to a remote webhook.
pub(crate) fn redact(text: &str) -> String {
    let text = PRIVATE_KEY.replace_all(text, REDACTED);
    let text = PRIVATE_KEY_END.replace_all(&text, REDACTED);
    let text = AUTH_SCHEME.replace_all(&text, format!("${{1}} {REDACTED}"));
    let text = ASSIGNMENT.replace_all(&text, format!("${{1}}${{2}}{REDACTED}"));
    let text = KNOWN_TOKEN.replace_all(&text, REDACTED);
    redact_long_tokens(&text)
}

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
