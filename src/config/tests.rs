//! Fixtures shared by the config tests.
use super::{Config, Webhook};
use crate::error::Error;

pub(super) const VALID: &str = r#"
default_webhook = "bridge"

[[webhook]]
name = "bridge"
description = "Coordinator"
url = "https://webhooks.example/hook/bridge"
authorization = "Bearer webhook-token"

[[webhook]]
name = "research"
url = "https://webhooks.example/hook/research"
authorization = "Token xyz"
"#;

pub(super) fn webhook(name: &str, authorization: &str) -> Webhook {
    Webhook {
        name: name.to_owned(),
        description: None,
        url: format!("https://webhooks.example/hook/{name}"),
        authorization: authorization.to_owned(),
        signing_secret: None,
    }
}

pub(super) fn config_of(default_webhook: Option<&str>, webhook: Vec<Webhook>) -> Config {
    Config {
        default_webhook: default_webhook.map(str::to_owned),
        webhook,
        ..Config::default()
    }
}

pub(super) fn config_error(result: Result<impl std::fmt::Debug, Error>) -> String {
    match result {
        Err(Error::Config(message)) => message,
        other => panic!("expected a config error, got {other:?}"),
    }
}
