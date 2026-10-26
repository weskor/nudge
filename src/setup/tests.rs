use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};

use super::{SetupOptions, scripted};
use crate::config::read_config;
use crate::env::Env;
use crate::error::Error;
use crate::webhook::tests::FakeWebhook;

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
