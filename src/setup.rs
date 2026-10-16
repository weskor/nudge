//! `nudge setup`: connect a desk (clipboard-first), test it before saving, then offer the herdr plugin's
//! auto-notify and the `nudge` command on PATH.
use std::io::{IsTerminal as _, Read as _};
use std::path::Path;

use crate::clipboard::{read_clipboard, runtime_dir};
use crate::config::{
    Auto, Config, Desk, check_authorization, check_webhook_url, default_config_path, read_config, upsert_desk,
    validate_desk, write_config,
};
use crate::context::{Env, System};
use crate::error::{Blame, DeskError, Error, Result};
use crate::install::{Plugin, command_link, link_command, nudge_root, plugin};
use crate::notify::{Origin, current_dir, ping_desk};
use crate::state::update_state;
use crate::tty::{Mark, Style, confirm, mask_secret, progress, prompt_line, wait_for_key};

#[derive(Debug, Default)]
pub(crate) struct SetupOptions {
    pub(crate) desk: Option<String>,
    pub(crate) url: Option<String>,
    pub(crate) authorization: Option<String>,
    pub(crate) description: Option<String>,
    /// HMAC signing secret shared with the desk; kept from the existing desk when not given.
    pub(crate) signing_secret: Option<String>,
    pub(crate) make_default: bool,
    /// Test-ping the webhook before saving (off with `--no-test`).
    pub(crate) test: bool,
    /// Running inside a herdr plugin pane: wait for a key before exiting.
    pub(crate) pane: bool,
}

/// A value `nudge setup` asks for.
#[derive(Clone, Copy, Debug)]
enum Field<'a> {
    Url { current: Option<&'a str> },
    Authorization { current: Option<&'a str> },
}

impl Field<'_> {
    const fn label(self) -> &'static str {
        match self {
            Self::Url { .. } => "Webhook URL",
            Self::Authorization { .. } => "Authorization header",
        }
    }

    const fn hidden(self) -> bool {
        matches!(self, Self::Authorization { .. })
    }

    fn check(self, value: &str) -> Result<String> {
        match self {
            Self::Url { .. } => check_webhook_url(value),
            Self::Authorization { .. } => {
                let checked = check_authorization(value)?;
                let lower = checked.to_ascii_lowercase();
                if lower.starts_with("http://") || lower.starts_with("https://") {
                    return Err(Error::Config(
                        "that's a URL, not the Authorization header. Copy the header value from your desk's webhook settings."
                            .into(),
                    ));
                }
                Ok(checked)
            }
        }
    }

    fn preview(self, value: &str) -> String {
        if self.hidden() {
            format!(
                "{} {}",
                mask_secret(value),
                Style::Dim.paint(format!("({} characters)", value.chars().count()))
            )
        } else {
            value.to_owned()
        }
    }
}

/// A validation message without its "invalid …" heading line.
fn problem(message: &str) -> &str {
    match message.split_once('\n') {
        Some((heading, rest)) if heading.starts_with("invalid ") => rest.trim_start(),
        Some(_) | None => message,
    }
}

/// Ask for one value, clipboard-first: Enter on an empty prompt reads the clipboard; typed or pasted text wins.
/// Without clipboard text, ask for manual entry (Enter keeps the current value). Loops until the value
/// validates. Hidden values are never echoed.
fn capture(step: &str, field: Field<'_>, env: &Env) -> Result<String> {
    let current = match field {
        Field::Url { current } | Field::Authorization { current } => current,
    };
    loop {
        let label = field.label();
        let hidden = field.hidden();
        let mut value = prompt_line(
            &format!("{step} Copy the {label} from your desk's webhook settings, then press Enter"),
            hidden,
        )?;
        if value.is_empty() {
            if let Some(clipboard) = read_clipboard(&System, env, &runtime_dir(env)) {
                value = clipboard;
            } else {
                let keep = if current.is_some() {
                    ", or Enter to keep the current one"
                } else {
                    ""
                };
                value = prompt_line(
                    &format!("    No clipboard text found. Paste or type the {label}{keep}"),
                    hidden,
                )?;
                if let (true, Some(current)) = (value.is_empty(), current) {
                    current.clone_into(&mut value);
                }
            }
        }
        match field.check(&value) {
            Ok(checked) => {
                eprintln!("    {} {}", Mark::Ok, field.preview(&checked));
                return Ok(checked);
            }
            Err(Error::Config(message)) => eprintln!("    {} {}", Mark::Fail, problem(&message)),
            Err(other) => return Err(other),
        }
    }
}

/// Plain-language reason for a failed test ping.
fn explain(error: &DeskError) -> String {
    let detail = error
        .message
        .strip_prefix("desk \"")
        .and_then(|rest| rest.split_once("\" webhook "))
        .map_or(error.message.as_str(), |(_, detail)| detail);
    match error.blames() {
        Some(Blame::Authorization) => format!("the webhook rejected the Authorization header ({detail})"),
        Some(Blame::Url) => format!("there's no webhook at that URL ({detail})"),
        None => detail.to_owned(),
    }
}

/// Test-ping `desk` alone. A webhook failure is returned, not raised.
fn ping(desk: &Desk, env: &Env) -> Result<Result<u16, DeskError>> {
    let config = Config {
        desk: vec![desk.clone()],
        ..Config::default()
    };
    let cwd = current_dir()?;
    match ping_desk(
        &config,
        Some(&desk.name),
        None,
        Origin {
            env,
            cwd: &cwd,
            run: &System,
        },
    ) {
        Ok(sent) => Ok(Ok(sent.http_status)),
        Err(Error::Desk(error)) => Ok(Err(error)),
        Err(other) => Err(other),
    }
}

fn test_desk(desk: &Desk, env: &Env) -> Result<Option<DeskError>> {
    progress("    Testing… ");
    Ok(match ping(desk, env)? {
        Ok(status) => {
            eprintln!("{} HTTP {status}: the desk received a test ping", Mark::Ok);
            None
        }
        Err(error) => {
            eprintln!("{} {}", Mark::Fail, explain(&error));
            Some(error)
        }
    })
}

/// Offer the herdr plugin's auto-notify and the `nudge` command on PATH. Interactive only.
fn offer_extras(env: &Env) -> Result<()> {
    eprintln!();
    match plugin(env, &System) {
        Plugin::HerdrUnavailable => eprintln!(
            "{} herdr isn't running, so the plugin step was skipped. Run {} again inside herdr.",
            Mark::Warn,
            Style::Bold.paint("nudge setup")
        ),
        Plugin::NotInstalled => eprintln!(
            "{} The herdr plugin isn't installed: {}",
            Mark::Warn,
            Style::Bold.paint("herdr plugin install weskor/nudge")
        ),
        Plugin::Installed(info) => {
            let enabled = if info.enabled {
                "enabled".to_owned()
            } else {
                Style::Yellow.paint("disabled")
            };
            eprintln!(
                "herdr plugin {} {enabled} {}",
                Mark::Ok,
                Style::Dim.paint(&info.plugin_root)
            );
            let min_turn = read_config(&default_config_path(env))
                .ok()
                .flatten()
                .map_or_else(Auto::default, |config| config.auto)
                .min_turn_words();
            eprintln!(
                "{}",
                Style::Dim
                    .paint("    Auto-notify sends an agent's last terminal lines (with secrets redacted) to the desk")
            );
            eprintln!(
                "{}",
                Style::Dim.paint(format!(
                    "    when it's blocked, or finishes a turn longer than {min_turn} while you're not watching it."
                ))
            );
            let auto = confirm("    Turn on auto-notify?", true)?;
            update_state(env, |state| state.auto = auto)?;
            let mark = if auto { Mark::Ok } else { Mark::Warn };
            eprintln!("    {mark} auto-notify {}", if auto { "on" } else { "off" });
        }
    }

    // Plugin panes get herdr's server PATH, which usually lacks ~/.local/bin, so also accept the link itself.
    let link = command_link(env);
    let installed = env.which("nudge").or_else(|| link.exists.then_some(link.path));
    if let Some(path) = installed {
        eprintln!("nudge command {} {}", Mark::Ok, path.display());
    } else if confirm(
        &format!(
            "Install the {} command into ~/.local/bin so agents can run it?",
            Style::Bold.paint("nudge")
        ),
        true,
    )? {
        match link_command(&nudge_root(env), env) {
            Ok(path) => eprintln!("    {} {}", Mark::Ok, path.display()),
            Err(error) => eprintln!("    {} {error}", Mark::Fail),
        }
    }
    Ok(())
}

/// Save `desk` into the config at `path`; returns the new config.
fn save(path: &Path, existing: Option<Config>, desk: Desk, make_default: bool) -> Result<Config> {
    let config = upsert_desk(existing, desk, make_default);
    write_config(path, &config)?;
    Ok(config)
}

/// Interactive setup on a terminal; returns the exit code.
fn interactive(options: &SetupOptions, env: &Env) -> Result<u8> {
    let path = default_config_path(env);
    let existing = read_config(&path)?;
    let name = options.desk.as_deref().unwrap_or("desk");
    let current = existing
        .as_ref()
        .and_then(|config| config.desk.iter().find(|desk| desk.name == name))
        .cloned();
    let action = if current.is_some() { "update" } else { "add" };
    eprintln!(
        "{}: {action} desk \"{name}\" {}",
        Style::Bold.paint("nudge setup"),
        Style::Dim.paint(format!("({})", path.display()))
    );
    eprintln!();

    let mut url = match options.url.as_deref().filter(|url| !url.is_empty()) {
        Some(url) => check_webhook_url(url)?,
        None => capture(
            "1/2",
            Field::Url {
                current: current.as_ref().map(|desk| desk.url.as_str()),
            },
            env,
        )?,
    };
    let mut authorization = match options.authorization.as_deref().filter(|value| !value.is_empty()) {
        Some(value) => check_authorization(value)?,
        None => capture(
            "2/2",
            Field::Authorization {
                current: current.as_ref().map(|desk| desk.authorization.as_str()),
            },
            env,
        )?,
    };
    let signing_secret = options
        .signing_secret
        .clone()
        .or_else(|| current.as_ref().and_then(|desk| desk.signing_secret.clone()));
    let description = options
        .description
        .clone()
        .or_else(|| current.and_then(|desk| desk.description));

    loop {
        let desk = validate_desk(
            name,
            &url,
            &authorization,
            description.as_deref(),
            signing_secret.as_deref(),
        )?;
        let failure = if options.test { test_desk(&desk, env)? } else { None };
        match failure.as_ref().map(DeskError::blames) {
            Some(Some(Blame::Url)) => url = capture("  ↻", Field::Url { current: None }, env)?,
            Some(Some(Blame::Authorization)) => {
                authorization = capture("  ↻", Field::Authorization { current: None }, env)?;
            }
            Some(None) | None => {
                if failure.is_some() && !confirm("    Save anyway?", false)? {
                    return Ok(1);
                }
                let config = save(&path, existing, desk, options.make_default)?;
                let as_default = if config.default_desk.as_deref() == Some(name) {
                    " as the default desk"
                } else {
                    ""
                };
                let location = Style::Dim.paint(format!("({}, mode 600)", path.display()));
                eprintln!("    {} Saved \"{name}\"{as_default} {location}", Mark::Ok);
                break;
            }
        }
    }

    offer_extras(env)?;
    eprintln!();
    eprintln!(
        "Done. Check everything any time with {}.",
        Style::Bold.paint("nudge doctor")
    );
    Ok(0)
}

/// Non-interactive setup from flags and stdin (URL, then Authorization, one per line).
fn scripted(options: &SetupOptions, env: &Env) -> Result<u8> {
    let path = default_config_path(env);
    let existing = read_config(&path)?;
    let name = options.desk.as_deref().unwrap_or("desk");
    let current = existing
        .as_ref()
        .and_then(|config| config.desk.iter().find(|desk| desk.name == name))
        .cloned();
    let mut url = options.url.clone();
    let mut authorization = options.authorization.clone();
    if url.is_none() || authorization.is_none() {
        let mut input = String::new();
        std::io::stdin().read_to_string(&mut input)?;
        let mut lines = input
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_owned);
        url = url.or_else(|| lines.next());
        authorization = authorization.or_else(|| lines.next());
    }
    let url = url.or_else(|| current.as_ref().map(|desk| desk.url.clone()));
    let authorization = authorization.or_else(|| current.as_ref().map(|desk| desk.authorization.clone()));
    let (Some(url), Some(authorization)) = (url, authorization) else {
        return Err(Error::Setup(
            "setup needs the webhook URL and Authorization value: run it in a terminal, pass --url/--authorization, \
             or pipe them on stdin (one per line)"
                .into(),
        ));
    };
    let signing_secret = options
        .signing_secret
        .clone()
        .or_else(|| current.as_ref().and_then(|desk| desk.signing_secret.clone()));
    let description = options
        .description
        .clone()
        .or_else(|| current.and_then(|desk| desk.description));
    let desk = validate_desk(
        name,
        &url,
        &authorization,
        description.as_deref(),
        signing_secret.as_deref(),
    )?;
    if options.test {
        match ping(&desk, env)? {
            Ok(status) => println!("Test ping to \"{}\": HTTP {status}.", desk.name),
            Err(error) => {
                return Err(Error::Setup(format!(
                    "test ping failed, nothing saved: {}. Use --no-test to save without testing.",
                    explain(&error)
                )));
            }
        }
    }
    let config = save(&path, existing, desk, options.make_default)?;
    let as_default = if config.default_desk.as_deref() == Some(name) {
        " (default)"
    } else {
        ""
    };
    println!("Saved desk \"{name}\"{as_default} to {} (mode 600).", path.display());
    Ok(0)
}

pub(crate) fn setup(options: &SetupOptions, env: &Env) -> Result<u8> {
    if !std::io::stdin().is_terminal() {
        return scripted(options, env);
    }
    let result = match interactive(options, env) {
        Err(Error::Cancelled) => {
            eprintln!("{}", Style::Dim.paint("Cancelled; nothing saved."));
            Ok(1)
        }
        other => other,
    };
    if options.pane {
        wait_for_key();
    }
    result
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt as _;

    use super::{SetupOptions, scripted};
    use crate::config::read_config;
    use crate::context::Env;
    use crate::error::Error;
    use crate::notify::tests::FakeWebhook;

    const GOOD: &str = "Bearer good-token-0123456789";

    /// A webhook at `/hook` that accepts only GOOD, plus a fresh config path.
    fn fixture() -> (FakeWebhook, tempfile::TempDir, Env) {
        let hook = FakeWebhook::start(
            |request| match (request.path.as_str(), request.header("authorization")) {
                ("/hook", Some(GOOD)) => (200, "ok".into()),
                ("/hook", _) => (401, r#"{"message":"Invalid API key"}"#.into()),
                _ => (404, "no such hook".into()),
            },
        );
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config.toml");
        let env = Env::from([
            ("NUDGE_CONFIG", config.to_str().unwrap()),
            ("NUDGE_STATE_DIR", dir.path().to_str().unwrap()),
            ("NUDGE_AGENT", "setup-test"),
        ]);
        (hook, dir, env)
    }

    fn options(url: String, authorization: &str) -> SetupOptions {
        SetupOptions {
            url: Some(url),
            authorization: Some(authorization.into()),
            test: true,
            ..SetupOptions::default()
        }
    }

    fn setup_error(result: crate::error::Result<u8>) -> String {
        match result {
            Err(Error::Setup(message)) => message,
            other => panic!("expected a setup error, got {other:?}"),
        }
    }

    #[test]
    fn tests_the_webhook_before_saving_and_saves_mode_600() {
        let (hook, dir, env) = fixture();
        let url = format!("{}/hook", hook.base);
        assert_eq!(scripted(&options(url.clone(), GOOD), &env).unwrap(), 0);
        assert_eq!(
            hook.requests()
                .iter()
                .map(|r| r.header("authorization").unwrap().to_owned())
                .collect::<Vec<_>>(),
            [GOOD]
        );
        let path = dir.path().join("config.toml");
        let desk = &read_config(&path).unwrap().unwrap().desk[0];
        assert_eq!(
            (desk.name.as_str(), desk.url.as_str(), desk.authorization.as_str()),
            ("desk", url.as_str(), GOOD)
        );
        assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
    }

    #[test]
    fn a_rejected_authorization_header_saves_nothing_and_says_why() {
        let (hook, dir, env) = fixture();
        let message = setup_error(scripted(
            &options(format!("{}/hook", hook.base), "Bearer wrong-token-0123456789"),
            &env,
        ));
        assert!(
            message.contains("nothing saved") && message.contains("rejected the Authorization header"),
            "{message}"
        );
        assert_eq!(read_config(&dir.path().join("config.toml")).unwrap(), None);
    }

    #[test]
    fn a_wrong_url_is_reported_as_such() {
        let (hook, _dir, env) = fixture();
        let message = setup_error(scripted(&options(format!("{}/nope", hook.base), GOOD), &env));
        assert!(message.contains("no webhook at that URL"), "{message}");
    }

    #[test]
    fn no_test_saves_without_contacting_the_webhook() {
        let (hook, dir, env) = fixture();
        let options = SetupOptions {
            test: false,
            ..options(format!("{}/hook", hook.base), "Bearer offline-token-0123456789")
        };
        assert_eq!(scripted(&options, &env).unwrap(), 0);
        assert!(hook.requests().is_empty());
        assert_eq!(
            read_config(&dir.path().join("config.toml"))
                .unwrap()
                .unwrap()
                .desk
                .len(),
            1
        );
    }

    #[test]
    fn errors_never_contain_the_authorization_value() {
        let (hook, _dir, env) = fixture();
        let message = setup_error(scripted(
            &options(format!("{}/hook", hook.base), "Bearer do-not-print-0123456789"),
            &env,
        ));
        assert!(!message.contains("do-not-print"));
    }
}
