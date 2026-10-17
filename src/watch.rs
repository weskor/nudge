//! Pure logic for the herdr plugin: which agent status changes become desk notifications, and how the
//! pane's terminal output is trimmed and redacted before it leaves the machine.
use std::sync::LazyLock;

use regex::Regex;
use serde::Deserialize;

use crate::config::Auto;
use crate::context::unreachable_regex;
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

/// A status that ends a turn and can be worth telling the desk about.
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
/// - Away mode (`state.away`) overrides the first two: you said you're not at your desk.
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
        return Decision::Skip("agent already notified the desk this turn".into());
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
    let text = AUTH_SCHEME.replace_all(&text, format!("${{1}} {REDACTED}"));
    let text = ASSIGNMENT.replace_all(&text, format!("${{1}}${{2}}{REDACTED}"));
    let text = KNOWN_TOKEN.replace_all(&text, REDACTED);
    redact_long_tokens(&text)
}

/// The last `lines` meaningful lines of a pane: no ANSI, no box-drawing rules, no blank runs.
pub(crate) fn clean_tail(raw: &str, lines: usize) -> String {
    let plain = ANSI.replace_all(raw, "");
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
    /// The user sent the pane with the `ask-desk` action.
    Manual,
}

/// Message for an automatic or on-demand notification built from the pane's output.
pub(crate) fn auto_message(trigger: Trigger, tail: &str) -> String {
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
    let redacted = redact(tail);
    let output = redacted.trim();
    if output.is_empty() {
        format!("{lead}\nNo terminal output captured.")
    } else {
        format!("{lead}\nLast terminal output (secrets redacted):\n\n{output}")
    }
}

#[cfg(test)]
mod tests {
    use super::{Decision, HerdrStatus, Settled, Trigger, auto_message, clean_tail, decide, redact};
    use crate::config::Auto;
    use crate::state::{PaneState, State};

    const P: &str = "w1:p3";
    const T0: i64 = 1_800_000_000_000;

    fn fresh() -> State {
        State {
            auto: true,
            ..State::default()
        }
    }

    const fn at(seconds: i64) -> i64 {
        T0 + seconds * 1000
    }

    /// Feed `(seconds from T0, status)` events; returns each decision.
    fn run(state: &mut State, events: &[(i64, HerdrStatus)], focused: bool) -> Vec<Decision> {
        events
            .iter()
            .map(|&(seconds, status)| decide(state, P, status, at(seconds), focused, Auto::default()))
            .collect()
    }

    fn last(state: &mut State, events: &[(i64, HerdrStatus)]) -> Decision {
        run(state, events, false).pop().unwrap()
    }

    fn skip(reason: &str) -> Decision {
        Decision::Skip(reason.into())
    }

    use HerdrStatus::{Blocked, Done, Idle, Working};

    #[test]
    fn a_long_turn_ending_in_done_notifies() {
        assert_eq!(
            last(&mut fresh(), &[(0, Working), (300, Done)]),
            Decision::Notify(Settled::Done)
        );
    }

    #[test]
    fn a_turn_shorter_than_min_turn_does_not() {
        let mut state = fresh();
        let _started = decide(&mut state, P, Working, T0, false, Auto::default());
        assert_eq!(
            decide(&mut state, P, Done, T0 + 120_000 - 1, false, Auto::default()),
            skip("turn shorter than 120s")
        );
    }

    #[test]
    fn configured_thresholds_replace_the_defaults() {
        let quick = Auto {
            min_turn_seconds: 10,
            cooldown_seconds: 0,
            ..Auto::default()
        };
        let mut state = fresh();
        let mut feed = |seconds, status| decide(&mut state, P, status, at(seconds), false, quick);
        let _started = feed(0, Working);
        assert_eq!(feed(9, Done), skip("turn shorter than 10s"));
        let _again = feed(20, Working);
        assert_eq!(feed(30, Done), Decision::Notify(Settled::Done));
        assert_eq!(feed(31, Blocked), Decision::Notify(Settled::Blocked), "no cooldown");
    }

    #[test]
    fn done_with_no_recorded_start_does_not() {
        assert_eq!(last(&mut fresh(), &[(300, Done)]), skip("turn start unknown"));
    }

    #[test]
    fn turn_length_counts_from_the_first_working_through_blocked() {
        let mut state = fresh();
        let _blocked = run(&mut state, &[(0, Working), (60, Blocked)], false);
        state.panes.get_mut(P).unwrap().last_auto_at = None; // ignore the blocked notification's cooldown
        assert_eq!(
            last(&mut state, &[(90, Working), (360, Done)]),
            Decision::Notify(Settled::Done)
        );
    }

    #[test]
    fn idle_ends_the_turn_without_notifying() {
        let decisions = run(&mut fresh(), &[(0, Working), (300, Idle), (360, Done)], false);
        assert_eq!(decisions[1..], [skip("status idle"), skip("turn start unknown")]);
    }

    #[test]
    fn blocked_notifies_immediately() {
        assert_eq!(
            last(&mut fresh(), &[(0, Working), (6, Blocked)]),
            Decision::Notify(Settled::Blocked)
        );
    }

    #[test]
    fn cooldown_allows_one_automatic_notification_per_pane_per_window() {
        let after_cooldown = 60 + 300 + 120;
        let decisions = run(
            &mut fresh(),
            &[
                (0, Working),
                (60, Blocked),
                (120, Working),
                (240, Done),
                (300, Working),
                (after_cooldown, Done),
            ],
            false,
        );
        assert_eq!(decisions.iter().filter(|d| matches!(d, Decision::Notify(_))).count(), 2);
        assert_eq!(decisions[3], skip("cooldown"));
    }

    #[test]
    fn cooldown_is_per_pane() {
        let mut state = fresh();
        let _blocked = run(&mut state, &[(0, Working), (60, Blocked)], false);
        assert_eq!(
            decide(&mut state, "other:p1", Blocked, at(90), false, Auto::default()),
            Decision::Notify(Settled::Blocked)
        );
    }

    #[test]
    fn skipped_when_the_agent_already_notified_this_turn() {
        let mut state = fresh();
        let _started = run(&mut state, &[(0, Working)], false);
        state.panes.get_mut(P).unwrap().self_notified_at = Some(at(240));
        assert_eq!(
            last(&mut state, &[(300, Done)]),
            skip("agent already notified the desk this turn")
        );
    }

    #[test]
    fn a_self_notify_from_a_previous_turn_does_not_suppress_this_one() {
        let mut state = fresh();
        let earlier = PaneState {
            self_notified_at: Some(at(-60)),
            ..PaneState::default()
        };
        let _previous = state.panes.insert(P.into(), earlier);
        assert_eq!(
            last(&mut state, &[(0, Working), (300, Done)]),
            Decision::Notify(Settled::Done)
        );
    }

    #[test]
    fn away_mode_notifies_for_the_focused_pane_and_with_auto_off_but_keeps_the_other_rules() {
        let mut state = State {
            auto: false,
            away: true,
            ..State::default()
        };
        let decisions = run(&mut state, &[(0, Working), (60, Blocked)], true);
        assert_eq!(decisions[1], Decision::Notify(Settled::Blocked));
        let quick = run(&mut state, &[(70, Working), (80, Done)], true);
        assert_eq!(
            quick[1],
            skip("turn shorter than 120s"),
            "short turns still don't count"
        );
        state.away = false;
        assert_eq!(last(&mut state, &[(500, Blocked)]), skip("auto-notify is off"));
    }

    #[test]
    fn focused_panes_and_auto_off_are_skipped_without_starting_a_cooldown() {
        let mut state = fresh();
        let focused = run(&mut state, &[(0, Working), (60, Blocked)], true);
        assert_eq!(focused[1], skip("you are looking at this pane"));
        assert_eq!(state.panes[P].last_auto_at, None);
        state.auto = false;
        assert_eq!(last(&mut state, &[(120, Blocked)]), skip("auto-notify is off"));
        state.auto = true;
        assert_eq!(last(&mut state, &[(180, Blocked)]), Decision::Notify(Settled::Blocked));
    }

    #[test]
    fn redacts_credentials() {
        for (input, expected) in [
            (
                "Authorization: Bearer abcdef0123456789xyz",
                "Authorization: Bearer [REDACTED]",
            ),
            (
                "export OPENAI_API_KEY=sk-proj-abcdefghijklmnop1234",
                "export OPENAI_API_KEY=[REDACTED]",
            ),
            (r#"password: "hunter2 with spaces""#, "password: [REDACTED]"),
            (
                "token ghp_abcdefghijklmnopqrstuvwxyz0123456789 leaked",
                "token [REDACTED] leaked",
            ),
            ("AWS key AKIAABCDEFGHIJKLMNOP here", "AWS key [REDACTED] here"),
            (
                "jwt eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U",
                "jwt [REDACTED]",
            ),
            ("blob Zx9Qw2Er8Ty4Ui6Op0As3Df5Gh7Jk1Lm9Nb", "blob [REDACTED]"),
            (
                "-----BEGIN OPENSSH PRIVATE KEY-----\nabc\n-----END OPENSSH PRIVATE KEY-----",
                "[REDACTED]",
            ),
        ] {
            assert_eq!(redact(input), expected, "{input}");
        }
    }

    #[test]
    fn keeps_ordinary_output() {
        let text = [
            "commit 8a25740f3c2d1e0b9a8f7e6d5c4b3a2f1e0d9c8b",
            "src/core/very_long_module_name_for_configuration_loading.ts",
            "see /tmp/Zx9Qw2Er8Ty4Ui6Op0As3Df5Gh7Jk1Lm9Nb/x and Zx9Qw2Er8Ty4Ui6Op0As3Df5Gh7Jk1Lm9Nb.log",
            " 51 pass, 0 fail",
            "Finished the refactor; tokens are now rotated nightly.",
        ]
        .join("\n");
        assert_eq!(redact(&text), text);
    }

    #[test]
    fn clean_tail_drops_ansi_rules_and_blank_runs_and_keeps_the_last_lines() {
        let raw = [
            "\x1b[32mline 1\x1b[0m",
            "",
            "",
            "────────────",
            "line 2",
            "line 3",
            "",
            "━━━",
            "❯",
            "",
            "",
        ]
        .join("\n");
        assert_eq!(clean_tail(&raw, 40), "line 1\n\nline 2\nline 3\n\n❯");
        assert_eq!(clean_tail(&raw, 2), "\n❯");
    }

    #[test]
    fn clean_tail_caps_very_long_output() {
        assert!(clean_tail(&"x".repeat(10_000), 40).chars().count() <= 4_001);
    }

    #[test]
    fn auto_message_says_it_was_auto_detected_and_redacts() {
        let message = auto_message(
            Trigger::Settled(Settled::Blocked),
            "Approve rm -rf build? token=abc12345secret",
        );
        assert!(message.starts_with("Agent is waiting for approval or input (auto-detected by herdr"));
        assert!(message.contains("token=[REDACTED]"));
        assert!(auto_message(Trigger::Settled(Settled::Done), "  ").ends_with("No terminal output captured."));
    }
}
