//! Find the webhook URL and the Authorization header in pasted or copied text: two lines (as scripted setup reads
//! them), a curl command, or a header line with a URL somewhere in the text. Values come back checked; anything
//! else in the text is never returned, so it can't end up on screen.
use crate::config::{check_authorization, check_webhook_url};
use crate::error::{Error, Result};

/// What a paste held, each value already checked.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Found {
    pub(super) url: Option<String>,
    pub(super) authorization: Option<String>,
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
mod tests {
    use super::{Found, prefill, split_paste};

    const URL: &str = "https://hooks.example.com/h/7f3c2a";
    const HEADER: &str = "Bearer token-0123456789abcdef";

    fn found(url: Option<&str>, authorization: Option<&str>) -> Found {
        Found {
            url: url.map(str::to_owned),
            authorization: authorization.map(str::to_owned),
        }
    }

    #[test]
    fn a_combined_paste_yields_both_values_in_its_usual_shapes() {
        let both = found(Some(URL), Some(HEADER));
        for paste in [
            format!("{URL}\n{HEADER}\n"),
            format!("  {URL}  \n\n  Authorization: {HEADER}  "),
            format!("curl -X POST -H 'Authorization: {HEADER}' -H 'Content-Type: application/json' {URL}"),
            format!("curl \"{URL}\" -H \"authorization: {HEADER}\" -d '{{}}'"),
            format!("Webhook: {URL}\nSend it with the header Authorization: {HEADER}"),
            format!("POST to <{URL}> with {HEADER}."),
        ] {
            assert_eq!(split_paste(&paste), both, "{paste}");
        }
    }

    #[test]
    fn a_paste_with_one_value_or_none_yields_only_what_checks_out() {
        assert_eq!(split_paste(URL), found(Some(URL), None));
        assert_eq!(
            split_paste(&format!("Authorization: {HEADER}")),
            found(None, Some(HEADER))
        );
        assert_eq!(split_paste("hello there, nothing to see"), found(None, None));
        assert_eq!(
            split_paste(&format!("http://example.com/hook\n{HEADER}")),
            found(None, Some(HEADER)),
            "plain http is only allowed for localhost"
        );
        assert_eq!(
            split_paste(&format!("Authorization: {URL}")),
            found(Some(URL), None),
            "a URL is never taken for the header"
        );
    }

    #[test]
    fn the_clipboard_prefills_only_a_lone_url_a_lone_header_or_both() {
        assert_eq!(prefill(&format!(" {URL}\n")), found(Some(URL), None));
        assert_eq!(prefill(HEADER), found(None, Some(HEADER)));
        assert_eq!(prefill(&format!("authorization: {HEADER}")), found(None, Some(HEADER)));
        assert_eq!(prefill(&format!("{URL}\n{HEADER}")), found(Some(URL), Some(HEADER)));
        assert_eq!(
            prefill(&format!("see {URL} for details")),
            found(None, None),
            "a link in prose"
        );
        assert_eq!(prefill("sk-live-0123456789abcdef"), found(None, None), "a bare secret");
        assert_eq!(prefill("Bearer is a word in this sentence"), found(None, None));
    }
}
