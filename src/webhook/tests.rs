use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::net::TcpListener;
use std::sync::Arc;
use std::thread;

use parking_lot::Mutex;

use crate::config::{Config, Webhook};
use crate::env::Env;
use crate::env::tests::FakeRun;
use crate::error::Error;

/// A request the fake webhook received.
#[derive(Clone, Debug)]
pub(crate) struct Request {
    pub(crate) path: String,
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) body: String,
}

impl Request {
    pub(crate) fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

/// A local HTTP server answering every request with `respond(request)`; records what it received.
pub(crate) struct FakeWebhook {
    pub(crate) base: String,
    pub(crate) requests: Arc<Mutex<Vec<Request>>>,
}

impl FakeWebhook {
    pub(crate) fn start(respond: impl Fn(&Request) -> (u16, String) + Send + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&requests);
        let _server = thread::spawn(move || {
            for stream in listener.incoming() {
                let mut stream = stream.unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let path = line.split_whitespace().nth(1).unwrap_or_default().to_owned();
                let mut headers = Vec::new();
                loop {
                    line.clear();
                    reader.read_line(&mut line).unwrap();
                    let Some((key, value)) = line.trim_end().split_once(": ") else {
                        break;
                    };
                    headers.push((key.to_owned(), value.to_owned()));
                }
                let length = headers
                    .iter()
                    .find(|(key, _)| key.eq_ignore_ascii_case("content-length"))
                    .map_or(0, |(_, value)| value.parse().unwrap());
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                let request = Request {
                    path,
                    headers,
                    body: String::from_utf8(body).unwrap(),
                };
                let (status, text) = respond(&request);
                seen.lock().push(request);
                write!(
                    stream,
                    "HTTP/1.1 {status} X\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{text}",
                    text.len()
                )
                .unwrap();
            }
        });
        Self { base, requests }
    }

    pub(crate) fn requests(&self) -> Vec<Request> {
        self.requests.lock().clone()
    }
}

pub(super) fn config(base: &str) -> Config {
    Config {
        default_webhook: Some("bridge".into()),
        webhook: vec![
            Webhook {
                name: "bridge".into(),
                description: None,
                url: format!("{base}/bridge"),
                authorization: "Bearer bridge-secret".into(),
                signing_secret: None,
            },
            Webhook {
                name: "research".into(),
                description: None,
                url: format!("{base}/research"),
                authorization: "Token research-secret".into(),
                signing_secret: Some("research-signing-secret-0123".into()),
            },
        ],
        ..Config::default()
    }
}

pub(super) fn herdr_and_git() -> FakeRun {
    FakeRun::new([
        (
            "herdr agent get w1:p1",
            r#"{"result":{"agent":{"name":"api-refactor","pane_id":"w1:p1"}}}"#,
        ),
        ("git rev-parse --show-toplevel", "/home/user/src/api"),
        ("git remote get-url origin", "git@github.com:acme/api.git"),
        ("git branch --show-current", "main"),
    ])
}

/// An environment in a herdr pane (unless `in_pane` is false) with its own state dir.
pub(super) fn env(state: &tempfile::TempDir, in_pane: bool) -> Env {
    let dir = state.path().to_str().unwrap();
    if in_pane {
        Env::from([("HERDR_PANE_ID", "w1:p1"), ("NUDGE_STATE_DIR", dir)])
    } else {
        Env::from([("NUDGE_STATE_DIR", dir)])
    }
}

pub(super) fn webhook_error(result: crate::error::Result<impl std::fmt::Debug>) -> String {
    match result {
        Err(Error::Webhook(error)) => error.message,
        other => panic!("expected a webhook error, got {other:?}"),
    }
}
