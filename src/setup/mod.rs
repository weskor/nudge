//! `nudge setup`: connect a webhook, test it before saving, then turn on the herdr plugin's auto-notify and put the
//! `nudge` command on PATH. In a terminal it is a full-screen form (clipboard-first, see `form`); otherwise it reads
//! flags and stdin.
mod form;

use std::io::{IsTerminal as _, Read as _};
use std::path::Path;

use crate::config::{
    Config, Webhook, default_config_path, read_config, upsert_webhook, validate_webhook, write_config,
};
use crate::context::{Env, System};
use crate::error::{Blame, Error, Result, WebhookError};
use crate::notify::{Origin, current_dir, ping_webhook};
use crate::tty::{Style, wait_for_key};

#[derive(Debug, Default)]
pub(crate) struct SetupOptions {
    pub(crate) webhook: Option<String>,
    pub(crate) url: Option<String>,
    pub(crate) authorization: Option<String>,
    pub(crate) description: Option<String>,
    /// HMAC signing secret shared with the webhook; kept from the existing webhook when not given.
    pub(crate) signing_secret: Option<String>,
    pub(crate) make_default: bool,
    /// Test-ping the webhook before saving (off with `--no-test`).
    pub(crate) test: bool,
    /// Running inside a herdr plugin pane: wait for a key before exiting.
    pub(crate) pane: bool,
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

/// Non-interactive setup from flags and stdin (URL, then Authorization, one per line).
fn scripted(options: &SetupOptions, env: &Env) -> Result<u8> {
    let path = default_config_path(env);
    let existing = read_config(&path)?;
    let name = options.webhook.as_deref().unwrap_or("main");
    let current = existing
        .as_ref()
        .and_then(|config| config.webhook.iter().find(|webhook| webhook.name == name))
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
    let url = url.or_else(|| current.as_ref().map(|webhook| webhook.url.clone()));
    let authorization = authorization.or_else(|| current.as_ref().map(|webhook| webhook.authorization.clone()));
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
    if options.test {
        match ping(&webhook, env)? {
            Ok(status) => println!("Test ping to \"{}\": HTTP {status}.", webhook.name),
            Err(error) => {
                return Err(Error::Setup(format!(
                    "test ping failed, nothing saved: {}. Use --no-test to save without testing.",
                    explain(&error)
                )));
            }
        }
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
    if !std::io::stdin().is_terminal() {
        return scripted(options, env);
    }
    let result = match form::interactive(options, env) {
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
}
