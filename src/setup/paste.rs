//! Find the webhook URL and the Authorization header in pasted or copied text: two lines (as scripted setup reads
//! them), a curl command, or a header line with a URL somewhere in the text. Values come back checked; anything
//! else in the text is never returned, so it can't end up on screen.
use std::fmt;

use crate::config::{check_authorization, check_webhook_url};
use crate::error::{Error, Result};
use crate::terminal::tty::Redacted;

/// What a paste held, each value already checked. `Debug` redacts the header.
#[derive(Default, PartialEq, Eq)]
pub(super) struct Found {
    pub(super) url: Option<String>,
    pub(super) authorization: Option<String>,
}

impl fmt::Debug for Found {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Found")
            .field("url", &self.url)
            .field("authorization", &self.authorization.as_deref().map(Redacted))
            .finish()
    }
}

impl Found {
    pub(super) const fn both(&self) -> bool {
        self.url.is_some() && self.authorization.is_some()
    }
}

/// Characters around a value in a command line or prose (a sentence's full stop included) that aren't part of it.
const WRAPPERS: &[char] = &['\'', '"', '`', '<', '>', '(', ')', '[', ']', ',', ';', '.'];
/// What ends a header value inside a command line.
const HEADER_ENDS: &[char] = &['\'', '"', '`', '\n', '\r'];
const HEADER_NAME: &str = "authorization:";
/// Authorization schemes a copied header usually starts with.
const SCHEMES: [&str; 3] = ["bearer", "basic", "token"];
/// The shortest credential taken after a scheme: shorter words are prose ("Bearer is …").
const MIN_CREDENTIAL: usize = 8;

/// `scheme credential` as a header, if `scheme` is a known one and `credential` looks like one.
fn scheme_header(scheme: &str, credential: &str) -> Option<String> {
    let credential = credential.trim_matches(WRAPPERS);
    (SCHEMES.iter().any(|known| scheme.eq_ignore_ascii_case(known)) && credential.chars().count() >= MIN_CREDENTIAL)
        .then(|| check_header(&format!("{scheme} {credential}")).ok())
        .flatten()
}

/// A header value that isn't a URL: a pasted URL is never taken for the header.
pub(super) fn check_header(value: &str) -> Result<String> {
    let checked = check_authorization(value)?;
    let lower = checked.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        return Err(Error::Config(
            "that's a URL, not the Authorization header. Copy the header value from your webhook's settings.".into(),
        ));
    }
    Ok(checked)
}

/// The first https (or local http) URL in `text` that is a valid webhook URL.
fn find_url(text: &str) -> Option<String> {
    text.split_whitespace()
        .map(|token| token.trim_matches(WRAPPERS))
        .filter(|token| token.starts_with("https://") || token.starts_with("http://"))
        .find_map(|token| check_webhook_url(token).ok())
}

/// An `Authorization:` header in `text`, else a `Bearer …` (or `Basic`, `Token`) value.
fn find_header(text: &str) -> Option<String> {
    // ASCII lowercasing keeps byte offsets, so the position found in `lower` is valid in `text`.
    let lower = text.to_ascii_lowercase();
    if let Some(start) = lower.find(HEADER_NAME) {
        let rest = text.get(start + HEADER_NAME.len()..)?;
        return rest
            .split(HEADER_ENDS)
            .next()
            .and_then(|value| check_header(value.trim()).ok());
    }
    let tokens: Vec<&str> = text.split_whitespace().collect();
    tokens.windows(2).find_map(|pair| match pair {
        [scheme, credential] => scheme_header(scheme, credential),
        _ => None,
    })
}

/// Everything `text` holds of the two values.
pub(super) fn split_paste(text: &str) -> Found {
    let lines: Vec<&str> = text.lines().map(str::trim).filter(|line| !line.is_empty()).collect();
    if let [first, second] = lines.as_slice()
        && let (Ok(url), Ok(authorization)) = (check_webhook_url(first), check_header(second))
    {
        return Found {
            url: Some(url),
            authorization: Some(authorization),
        };
    }
    Found {
        url: find_url(text),
        authorization: find_header(text),
    }
}

/// What setup may take from the clipboard without being asked: both values from one combined copy, a lone URL, or
/// a lone header that says it is one (an `Authorization:` line, or a value starting with a known scheme). Anything
/// else, such as a sentence that merely contains a link, is left alone.
pub(super) fn prefill(clipboard: &str) -> Found {
    let found = split_paste(clipboard);
    if found.both() {
        return found;
    }
    let whole = clipboard.trim();
    let words: Vec<&str> = whole.split_whitespace().collect();
    let authorization = if whole.to_ascii_lowercase().starts_with(HEADER_NAME) && whole.lines().count() == 1 {
        check_header(whole).ok()
    } else if let [scheme, credential] = words.as_slice() {
        scheme_header(scheme, credential)
    } else {
        None
    };
    Found {
        url: check_webhook_url(whole).ok(),
        authorization,
    }
}

#[cfg(test)]
mod tests;
