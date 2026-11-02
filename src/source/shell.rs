//! Quoting a reply target for the shell command in a reply hint.

/// `word` as one shell word: as is when it's plainly safe, else single-quoted.
pub(crate) fn shell_word(word: &str) -> String {
    let safe = !word.is_empty()
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._:@%+=,-".contains(c));
    if safe {
        word.to_owned()
    } else {
        format!("'{}'", word.replace('\'', r"'\''"))
    }
}
