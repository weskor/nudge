//! `nudge setup`: connect a webhook, test it before saving, then turn on the herdr plugin's auto-notify and put the
//! `nudge` command on PATH. In a terminal it is a step-by-step wizard (`wizard` holds its state, `view` draws and
//! runs it); otherwise it reads flags and stdin.
mod paste;
mod view;
mod wizard;

use std::fmt;
use std::io::{IsTerminal as _, Read as _};
use std::path::Path;

use ratatui::crossterm::event::{DisableBracketedPaste, EnableBracketedPaste};
use ratatui::crossterm::execute;

use self::wizard::{Field, Source, Sourced, Start, Wizard};
use crate::config::{
    Auto, Config, Webhook, check_authorization, check_webhook_url, default_config_path, read_config, seconds_in_words,
    upsert_webhook, validate_webhook, write_config,
};
use crate::context::{Env, System};
use crate::error::{Blame, Error, Result, WebhookError};
use crate::install::{Plugin, command_link, link_command, nudge_root, plugin};
use crate::notify::{Origin, current_dir, ping_webhook};
use crate::state::{current_state, update_state};
use crate::tty::{Mark, Redacted, Style, wait_for_key};

/// `nudge setup`'s flags. `Debug` redacts the Authorization header and the signing secret.
#[derive(Default)]
#[expect(clippy::struct_excessive_bools, reason = "independent command-line flags")]
pub(crate) struct SetupOptions {
    pub(crate) webhook: Option<String>,
    pub(crate) url: Option<String>,
    pub(crate) authorization: Option<String>,
    /// A file holding the Authorization header (`-`: stdin), so it stays out of shell history.
    pub(crate) authorization_file: Option<String>,
    pub(crate) description: Option<String>,
    /// HMAC signing secret shared with the webhook; kept from the existing webhook when not given.
    pub(crate) signing_secret: Option<String>,
    pub(crate) make_default: bool,
    /// Test-ping the webhook before saving (off with `--no-test`).
    pub(crate) test: bool,
    /// No screen: accept every default, and do the step-3 choices that plain scripted setup leaves alone.
    pub(crate) yes: bool,
    pub(crate) no_auto_notify: bool,
    pub(crate) no_install_command: bool,
    /// Running inside a herdr plugin pane: wait for a key before exiting.
    pub(crate) pane: bool,
}

impl fmt::Debug for SetupOptions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SetupOptions")
            .field("webhook", &self.webhook)
            .field("url", &self.url)
            .field("authorization", &self.authorization.as_deref().map(Redacted))
            .field("authorization_file", &self.authorization_file)
            .field("signing_secret", &self.signing_secret.as_deref().map(Redacted))
            .field("yes", &self.yes)
            .field("test", &self.test)
            .finish_non_exhaustive()
    }
}

/// What to do after saving (the wizard's step 3): what's offered and what's chosen.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Choices {
    plugin: Plugin,
    /// Whether to turn on auto-notify; `None` when the herdr plugin isn't installed, so there's nothing to turn on.
    auto_notify: Option<bool>,
    /// Where `nudge` already is, as shown; `None` offers to link it.
    command: Option<String>,
    link_command: bool,
    make_default: bool,
}

/// `path` with the home directory shown as `~`.
fn tilde(path: &Path, home: &Path) -> String {
    path.strip_prefix(home)
        .map_or_else(|_| path.display().to_string(), |rest| format!("~/{}", rest.display()))
}

/// What setup starts from on this machine (herdr's plugin, the command on PATH, the auto-notify state), with the
/// step-3 flags applied.
fn choices(options: &SetupOptions, first_setup: bool, env: &Env) -> Choices {
    let plugin = plugin(env, &System);
    let link = command_link(env);
    Choices {
        // A first setup suggests auto-notify; a rerun keeps what's on.
        auto_notify: matches!(plugin, Plugin::Installed(_))
            .then(|| !options.no_auto_notify && (first_setup || current_state(env).auto)),
        plugin,
        // Plugin panes get herdr's server PATH, which usually lacks ~/.local/bin, so also accept the link itself.
        command: env
            .which("nudge")
            .or_else(|| link.exists.then_some(link.path))
            .map(|path| tilde(&path, &env.home())),
        link_command: !options.no_install_command,
        make_default: options.make_default,
    }
}

/// The step-1 values from the command line and the environment, before the saved webhook, with where each came
/// from. Values are as given (scripted setup reports problems with the whole webhook; the wizard checks them first).
fn given(options: &SetupOptions, env: &Env) -> Result<(Sourced, Sourced)> {
    let flag = |value: &Option<String>| value.clone().filter(|value| !value.is_empty());
    let from_env = |name: &str, source| env.get(name).map(|value| (value.to_owned(), source));
    let url = flag(&options.url)
        .map(|url| (url, Source::Flag))
        .or_else(|| from_env("NUDGE_WEBHOOK_URL", Source::Environment));
    let file = match (flag(&options.authorization), flag(&options.authorization_file)) {
        (Some(_), Some(_)) => {
            return Err(Error::Usage(
                "use --authorization or --authorization-file, not both".into(),
            ));
        }
        (Some(value), None) => return Ok((url, Some((value, Source::Flag)))),
        (None, file) => file,
    };
    let authorization = match file.as_deref() {
        // Reading a terminal would echo the header as it's typed, so `-` needs a pipe or a redirect.
        Some("-") if std::io::stdin().is_terminal() => {
            return Err(Error::Usage(
                "--authorization-file - reads the header from a pipe, but stdin is a terminal; pipe it in \
                 (pass show hook | nudge setup --authorization-file -) or pass a file path"
                    .into(),
            ));
        }
        Some("-") => {
            let mut input = String::new();
            std::io::stdin().read_to_string(&mut input)?;
            Some((input.trim().to_owned(), Source::File))
        }
        Some(path) => {
            let text = std::fs::read_to_string(path)
                .map_err(|error| Error::Setup(format!("can't read --authorization-file {path}: {error}")))?;
            Some((text.trim().to_owned(), Source::File))
        }
        None => from_env("NUDGE_WEBHOOK_AUTHORIZATION", Source::Environment),
    };
    Ok((url, authorization))
}

/// Do what the choices ask for, and say so.
fn apply_choices(choices: &Choices, env: &Env) -> Result<()> {
    match (&choices.plugin, choices.auto_notify) {
        (Plugin::Installed(_), Some(auto)) => {
            update_state(env, |state| state.auto = auto)?;
            let mark = if auto { Mark::Ok } else { Mark::Warn };
            eprintln!("{mark} auto-notify {}", if auto { "on" } else { "off" });
        }
        (Plugin::Installed(_), None) | (Plugin::HerdrUnavailable, _) => eprintln!(
            "{} herdr isn't running, so auto-notify wasn't turned on. Run {} again inside herdr.",
            Mark::Warn,
            Style::Bold.paint("nudge setup")
        ),
        (Plugin::NotInstalled, _) => eprintln!(
            "{} The herdr plugin isn't installed: {}",
            Mark::Warn,
            Style::Bold.paint("herdr plugin install weskor/nudge")
        ),
    }
    match &choices.command {
        Some(shown) => eprintln!("{} nudge command {shown}", Mark::Ok),
        None if choices.link_command => match link_command(&nudge_root(env), env) {
            Ok(path) => eprintln!("{} nudge command {}", Mark::Ok, tilde(&path, &env.home())),
            Err(error) => eprintln!("{} {error}", Mark::Fail),
        },
        None => {}
    }
    Ok(())
}

/// Say how the test ping went and what was saved, do what the choices ask for, and say what happens next. The
/// webhook `name` is already saved in `config`.
fn finish(
    tested: Option<u16>,
    test: bool,
    config: &Config,
    name: &str,
    choices: &Choices,
    min_turn: &str,
    env: &Env,
) -> Result<u8> {
    match tested {
        Some(status) => eprintln!("{} Test ping: HTTP {status}, the webhook received it", Mark::Ok),
        None if test => eprintln!("{} Saved without a successful test ping", Mark::Warn),
        None => {}
    }
    let path = default_config_path(env);
    let as_default = if config.default_webhook.as_deref() == Some(name) {
        " as the default webhook"
    } else {
        ""
    };
    let location = Style::Dim.paint(format!("({}, mode 600)", path.display()));
    eprintln!("{} Saved \"{name}\"{as_default} {location}", Mark::Ok);
    apply_choices(choices, env)?;
    eprintln!();
    eprintln!("{}", Style::Bold.paint("What happens next"));
    if choices.auto_notify == Some(true) {
        eprintln!(
            "  • When an agent is blocked, or done after {min_turn} while you're not watching, your webhook gets an event."
        );
    } else {
        eprintln!("  • Turn on auto-notify in nudge settings to hear when an agent is blocked or done.");
    }
    eprintln!("  • Agents can send their own: nudge notify \"…\" (or the MCP tool notify_webhook).");
    eprintln!("  • Check everything with nudge doctor; change options with nudge settings.");
    Ok(0)
}

/// The shortest turn auto-notify sends, in words.
fn min_turn(config: Option<&Config>) -> String {
    seconds_in_words(config.map_or_else(Auto::default, |config| config.auto).min_turn_seconds)
}

/// The setup wizard in a terminal; returns the exit code.
fn interactive(options: &SetupOptions, env: &Env) -> Result<u8> {
    let existing = read_config(&default_config_path(env))?;
    let name = options.webhook.clone().unwrap_or_else(|| "main".into());
    let current = existing
        .as_ref()
        .and_then(|config| config.webhook.iter().find(|webhook| webhook.name == name))
        .cloned();
    // Values from flags, the file and the environment are checked now: the wizard shows them as accepted.
    let (url, authorization) = given(options, env)?;
    let url = url
        .map(|(url, source)| check_webhook_url(&url).map(|url| (url, source)))
        .transpose()?
        .or_else(|| current.as_ref().map(|webhook| (webhook.url.clone(), Source::Saved)));
    let authorization = authorization
        .map(|(value, source)| check_authorization(&value).map(|value| (value, source)))
        .transpose()?
        .or_else(|| {
            current
                .as_ref()
                .map(|webhook| (webhook.authorization.clone(), Source::Saved))
        });
    let min_turn = min_turn(existing.as_ref());
    let mut wizard = Wizard::new(Start {
        name,
        loaded: current.as_ref().map(|webhook| webhook.name.clone()),
        url,
        authorization,
        description: options
            .description
            .clone()
            .or_else(|| current.as_ref().and_then(|webhook| webhook.description.clone())),
        saved: existing
            .as_ref()
            .map(|config| config.webhook.iter().map(|webhook| webhook.name.clone()).collect())
            .unwrap_or_default(),
        default_webhook: existing.as_ref().and_then(|config| config.default_webhook.clone()),
        choices: choices(options, existing.is_none(), env),
        min_turn: min_turn.clone(),
        signing_secret: options
            .signing_secret
            .clone()
            .or_else(|| current.and_then(|webhook| webhook.signing_secret)),
        test: options.test,
    });

    let path = default_config_path(env);
    let flag_default = options.make_default;
    // The file is read again when saving: something else may have changed it while the wizard was open.
    let save_now = |wizard: &Wizard| {
        let make_default = wizard.final_choices(flag_default).make_default;
        save(&path, read_config(&path)?, wizard.webhook()?, make_default)
    };
    let mut terminal = ratatui::try_init()?;
    let ran = execute!(std::io::stdout(), EnableBracketedPaste)
        .map_err(Error::from)
        .and_then(|()| view::run(&mut terminal, &mut wizard, env, save_now));
    let unpasted = execute!(std::io::stdout(), DisableBracketedPaste);
    ratatui::restore();
    unpasted?;
    let (tested, config) = ran?;
    finish(
        tested,
        options.test,
        &config,
        &wizard.entry(Field::Name).value,
        &wizard.final_choices(flag_default),
        &min_turn,
        env,
    )
}

/// Plain-language reason for a failed test ping.
fn explain(error: &WebhookError) -> String {
    let detail = error
        .message
        .strip_prefix("webhook \"")
        .and_then(|rest| rest.split_once("\" "))
        .map_or(error.message.as_str(), |(_, detail)| detail);
    match error.blames() {
        Some(Blame::Authorization) => format!("the webhook rejected the Authorization header ({detail})"),
        Some(Blame::Url) => format!("there's no webhook at that URL ({detail})"),
        None => detail.to_owned(),
    }
}

/// Test-ping `webhook` alone. A webhook failure is returned, not raised.
fn ping(webhook: &Webhook, env: &Env) -> Result<Result<u16, WebhookError>> {
    let config = Config {
        webhook: vec![webhook.clone()],
        ..Config::default()
    };
    let cwd = current_dir()?;
    match ping_webhook(
        &config,
        Some(&webhook.name),
        None,
        Origin {
            env,
            cwd: &cwd,
            run: &System,
        },
    ) {
        Ok(sent) => Ok(Ok(sent.http_status)),
        Err(Error::Webhook(error)) => Ok(Err(error)),
        Err(other) => Err(other),
    }
}

/// Save `webhook` into the config at `path`; returns the new config.
fn save(path: &Path, existing: Option<Config>, webhook: Webhook, make_default: bool) -> Result<Config> {
    let config = upsert_webhook(existing, webhook, make_default);
    write_config(path, &config)?;
    Ok(config)
}

/// What a run without a screen says when a value is missing (`yes`: name the ways to give it).
fn missing(yes: bool, url_missing: bool) -> String {
    match (yes, url_missing) {
        (true, true) => "setup --yes needs the webhook URL: --url or NUDGE_WEBHOOK_URL".into(),
        (true, false) => "setup --yes needs the Authorization header: --authorization, --authorization-file or \
             NUDGE_WEBHOOK_AUTHORIZATION"
            .into(),
        (false, _) => "setup needs the webhook URL and Authorization value: run it in a terminal, pass \
                       --url/--authorization, or pipe them on stdin (one per line)"
            .into(),
    }
}

/// Setup without a screen. Values come from flags, the file and the environment, then stdin (URL, then
/// Authorization, one per line), then the saved webhook. With `--yes` it also does the step-3 choices (plain
/// scripted setup leaves them alone) and says what happens next.
fn scripted(options: &SetupOptions, env: &Env) -> Result<u8> {
    let path = default_config_path(env);
    let existing = read_config(&path)?;
    let name = options.webhook.as_deref().unwrap_or("main");
    let current = existing
        .as_ref()
        .and_then(|config| config.webhook.iter().find(|webhook| webhook.name == name))
        .cloned();
    let (url, authorization) = given(options, env)?;
    let mut url = url.map(|(url, _)| url);
    let mut authorization = authorization.map(|(value, _)| value);
    let stdin_read = options.authorization_file.as_deref() == Some("-");
    if (url.is_none() || authorization.is_none()) && !stdin_read && !std::io::stdin().is_terminal() {
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
    let url = url.or_else(|| current.as_ref().map(|webhook| webhook.url.clone()));
    let authorization = authorization.or_else(|| current.as_ref().map(|webhook| webhook.authorization.clone()));
    let url_missing = url.is_none();
    let (Some(url), Some(authorization)) = (url, authorization) else {
        return Err(Error::Setup(missing(options.yes, url_missing)));
    };
    let signing_secret = options
        .signing_secret
        .clone()
        .or_else(|| current.as_ref().and_then(|webhook| webhook.signing_secret.clone()));
    let description = options
        .description
        .clone()
        .or_else(|| current.and_then(|webhook| webhook.description));
    let webhook = validate_webhook(
        name,
        &url,
        &authorization,
        description.as_deref(),
        signing_secret.as_deref(),
    )?;
    let tested = if options.test {
        match ping(&webhook, env)? {
            Ok(status) => Some(status),
            Err(error) => {
                return Err(Error::Setup(format!(
                    "test ping failed, nothing saved: {}. Use --no-test to save without testing.",
                    explain(&error)
                )));
            }
        }
    } else {
        None
    };
    if options.yes {
        let choices = choices(options, existing.is_none(), env);
        let min_turn = min_turn(existing.as_ref());
        let config = save(&path, existing, webhook, choices.make_default)?;
        return finish(tested, options.test, &config, name, &choices, &min_turn, env);
    }
    if let Some(status) = tested {
        println!("Test ping to \"{name}\": HTTP {status}.");
    }
    let config = save(&path, existing, webhook, options.make_default)?;
    let as_default = if config.default_webhook.as_deref() == Some(name) {
        " (default)"
    } else {
        ""
    };
    println!("Saved webhook \"{name}\"{as_default} to {} (mode 600).", path.display());
    Ok(0)
}

pub(crate) fn setup(options: &SetupOptions, env: &Env) -> Result<u8> {
    if options.yes || !std::io::stdin().is_terminal() {
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
    use std::path::{Path, PathBuf};

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
        let webhook = &read_config(&path).unwrap().unwrap().webhook[0];
        assert_eq!(
            (
                webhook.name.as_str(),
                webhook.url.as_str(),
                webhook.authorization.as_str()
            ),
            ("main", url.as_str(), GOOD)
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
                .webhook
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

    /// A machine with no herdr and no `nudge` on PATH, home, state and config in `dir`; `url` and `authorization`
    /// go into the setup environment variables (empty: unset).
    fn bare_env(dir: &Path, url: &str, authorization: &str) -> Env {
        let empty = dir.join("empty-path");
        fs::create_dir_all(&empty).unwrap();
        let path = |name: &str| dir.join(name).to_str().unwrap().to_owned();
        Env::from([
            ("NUDGE_CONFIG", path("config.toml").as_str()),
            ("NUDGE_STATE_DIR", dir.to_str().unwrap()),
            ("NUDGE_AGENT", "setup-test"),
            ("NUDGE_ROOT", "/opt/nudge"),
            ("HOME", path("home").as_str()),
            ("PATH", empty.to_str().unwrap()),
            ("HERDR_BIN_PATH", path("no-herdr").as_str()),
            ("NUDGE_WEBHOOK_URL", url),
            ("NUDGE_WEBHOOK_AUTHORIZATION", authorization),
        ])
    }

    fn saved(dir: &Path) -> (String, String) {
        let webhook = read_config(&dir.join("config.toml"))
            .unwrap()
            .unwrap()
            .webhook
            .remove(0);
        (webhook.url, webhook.authorization)
    }

    fn command_link(dir: &Path) -> Option<PathBuf> {
        fs::read_link(dir.join("home/.local/bin/nudge")).ok()
    }

    #[test]
    fn yes_takes_values_from_the_environment_and_does_the_step_three_choices() {
        let (hook, dir, _) = fixture();
        let url = format!("{}/hook", hook.base);
        let yes = SetupOptions {
            yes: true,
            test: true,
            ..SetupOptions::default()
        };
        assert_eq!(scripted(&yes, &bare_env(dir.path(), &url, GOOD)).unwrap(), 0);
        assert_eq!(saved(dir.path()), (url.clone(), GOOD.to_owned()));
        assert_eq!(command_link(dir.path()), Some(PathBuf::from("/opt/nudge/bin/nudge")));

        let declined = tempfile::tempdir().unwrap();
        let options = SetupOptions {
            no_install_command: true,
            ..yes
        };
        assert_eq!(scripted(&options, &bare_env(declined.path(), &url, GOOD)).unwrap(), 0);
        assert_eq!(command_link(declined.path()), None);

        let plain = tempfile::tempdir().unwrap();
        assert_eq!(
            scripted(&options_test(), &bare_env(plain.path(), &url, GOOD)).unwrap(),
            0
        );
        assert_eq!(
            command_link(plain.path()),
            None,
            "plain scripted setup leaves the command alone"
        );
    }

    fn options_test() -> SetupOptions {
        SetupOptions {
            test: true,
            ..SetupOptions::default()
        }
    }

    #[test]
    fn the_authorization_file_and_flags_win_over_the_environment() {
        let (hook, dir, _) = fixture();
        let url = format!("{}/hook", hook.base);
        let file = dir.path().join("header");
        fs::write(&file, format!("  Authorization: {GOOD}\n")).unwrap();
        let options = SetupOptions {
            url: Some(url.clone()),
            authorization_file: Some(file.to_str().unwrap().to_owned()),
            ..options_test()
        };
        let env = bare_env(dir.path(), "https://wrong.example/hook", "Bearer env-token-0123456789");
        assert_eq!(scripted(&options, &env).unwrap(), 0);
        assert_eq!(saved(dir.path()), (url, GOOD.to_owned()));

        let both = SetupOptions {
            authorization: Some(GOOD.into()),
            ..options
        };
        assert!(matches!(scripted(&both, &env), Err(Error::Usage(_))));
        let missing = SetupOptions {
            authorization_file: Some(dir.path().join("nope").to_str().unwrap().to_owned()),
            ..options_test()
        };
        let message = setup_error(scripted(&missing, &env));
        assert!(message.contains("can't read --authorization-file"), "{message}");
    }
}
