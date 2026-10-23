//! Terminal helpers for commands that print to the terminal (setup, doctor). Progress goes to stderr.
use std::fmt;
use std::io::{IsTerminal as _, Read as _, Write as _};
use std::sync::LazyLock;

use rustix::termios::{
    ControlModes, InputModes, LocalModes, OptionalActions, OutputModes, SpecialCodeIndex, Termios, tcgetattr, tcsetattr,
};

use crate::error::{Error, Result};

static COLOR: LazyLock<bool> = LazyLock::new(|| {
    std::io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none_or(|value| value.is_empty())
});

#[derive(Clone, Copy, Debug)]
pub(crate) enum Style {
    Green,
    Red,
    Yellow,
    Dim,
    Bold,
}

impl Style {
    /// `text` in this style when stderr is a color terminal (and `NO_COLOR` is unset), else as-is.
    pub(crate) fn paint(self, text: impl fmt::Display) -> String {
        if !*COLOR {
            return text.to_string();
        }
        let code = match self {
            Self::Green => "32",
            Self::Red => "31",
            Self::Yellow => "33",
            Self::Dim => "2",
            Self::Bold => "1",
        };
        format!("\x1b[{code}m{text}\x1b[0m")
    }
}

/// Status marks: ✓ ✗ !
#[derive(Clone, Copy, Debug)]
pub(crate) enum Mark {
    Ok,
    Fail,
    Warn,
}

impl fmt::Display for Mark {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&match self {
            Self::Ok => Style::Green.paint("✓"),
            Self::Fail => Style::Red.paint("✗"),
            Self::Warn => Style::Yellow.paint("!"),
        })
    }
}

/// Write `text` to stderr now, without a newline.
fn progress(text: &str) {
    let mut stderr = std::io::stderr();
    stderr.write_all(text.as_bytes()).and_then(|()| stderr.flush()).ok();
}

/// stdin in raw mode (like Node's `setRawMode`) until dropped: no echo, no line editing, Ctrl-C as a byte.
/// Output processing stays on, so `\n` still starts a new line.
#[derive(Debug)]
struct RawMode(Termios);

impl RawMode {
    fn enable() -> Result<Self> {
        let stdin = rustix::stdio::stdin();
        let original = tcgetattr(stdin).map_err(|error| Error::Other(format!("terminal: {error}")))?;
        let mut raw = original.clone();
        raw.input_modes
            .remove(InputModes::BRKINT | InputModes::ICRNL | InputModes::INPCK | InputModes::ISTRIP | InputModes::IXON);
        raw.output_modes.insert(OutputModes::ONLCR);
        raw.control_modes.insert(ControlModes::CS8);
        raw.local_modes
            .remove(LocalModes::ECHO | LocalModes::ICANON | LocalModes::IEXTEN | LocalModes::ISIG);
        raw.special_codes[SpecialCodeIndex::VMIN] = 1;
        raw.special_codes[SpecialCodeIndex::VTIME] = 0;
        tcsetattr(stdin, OptionalActions::Now, &raw).map_err(|error| Error::Other(format!("terminal: {error}")))?;
        Ok(Self(original))
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        tcsetattr(rustix::stdio::stdin(), OptionalActions::Now, &self.0).ok();
    }
}

/// Wait for any key; used before a herdr pane closes so its output stays readable.
pub(crate) fn wait_for_key() {
    if !std::io::stdin().is_terminal() {
        return;
    }
    progress(&Style::Dim.paint("Press any key to close"));
    if let Ok(_raw) = RawMode::enable() {
        let mut byte = [0; 1];
        std::io::stdin().lock().read_exact(&mut byte).ok();
    }
    progress("\n");
}

/// Recognizable but safe preview of a secret: the scheme, then dots and the last 4 characters. Values shorter
/// than 16 characters (after the scheme) show no characters at all.
/// `Bearer abcdefghijklmnopqrstuvwxyz` → `Bearer ••••••••wxyz`.
pub(crate) fn mask_secret(value: &str) -> String {
    const DOTS: &str = "••••••••";
    let letters = value.find(|c: char| !c.is_ascii_alphabetic()).unwrap_or(value.len());
    let (scheme, secret) = match value.split_at_checked(letters) {
        Some((scheme, rest)) if !scheme.is_empty() && rest.starts_with(char::is_whitespace) => {
            let secret = rest.trim_start();
            if secret.is_empty() || secret.contains('\n') {
                ("", value)
            } else {
                (scheme, secret)
            }
        }
        Some(_) | None => ("", value),
    };
    let scheme = if scheme.is_empty() {
        String::new()
    } else {
        format!("{scheme} ")
    };
    let chars = secret.chars().count();
    if chars >= 16 {
        format!("{scheme}{DOTS}{}", secret.chars().skip(chars - 4).collect::<String>())
    } else {
        format!("{scheme}{DOTS}")
    }
}

#[cfg(test)]
mod tests {
    use super::mask_secret;

    #[test]
    fn mask_keeps_the_scheme_and_the_last_4_characters_of_long_secrets() {
        assert_eq!(mask_secret("Bearer abcdefghijklmnopqrstuvwxyz"), "Bearer ••••••••wxyz");
        assert_eq!(mask_secret("abcdefghijklmnopqrstuvwxyz"), "••••••••wxyz");
    }

    #[test]
    fn mask_reveals_nothing_of_short_secrets() {
        assert_eq!(mask_secret("Bearer short-token-1"), "Bearer ••••••••");
        assert_eq!(mask_secret("Bearer 123456789012345"), "Bearer ••••••••");
        assert_eq!(mask_secret("Bearer 1234567890123456"), "Bearer ••••••••3456");
    }
}
