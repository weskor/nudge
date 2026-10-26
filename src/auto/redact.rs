//! Redaction: replace anything that looks like a credential before pane output leaves the machine.
use std::sync::LazyLock;

use regex::Regex;

use crate::error::unreachable_regex;

const REDACTED: &str = "[REDACTED]";

/// Compile one of nudge's built-in patterns.
pub(super) fn regex(pattern: &str) -> Regex {
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

#[cfg(test)]
mod tests;
