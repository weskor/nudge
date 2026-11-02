use std::path::Path;

use parking_lot::Mutex;

use super::{Outcome, Sent, handle};
use crate::config::{Config, Route, Scope, Selector, Webhook};
use crate::env::Env;
use crate::env::tests::FakeRun;
use crate::error::Error;
use crate::pipeline::Context;
use crate::source::{AgentRef, AgentSource, AgentStatus, Backend, Transition, WorkspaceRef, host_name};
use crate::state::{now_ms, update_state};
use crate::webhook::tests::FakeWebhook;

/// A backend that serves a fixed tail and workspace, and records what the pipeline asked of it.
#[derive(Default)]
struct FakeSource {
    workspace: Option<WorkspaceRef>,
    tails: Mutex<usize>,
    workspace_lookups: Mutex<usize>,
    toasts: Mutex<Vec<String>>,
}

impl AgentSource for FakeSource {
    fn tail(&self, _agent: &AgentRef, _lines: usize) -> Option<String> {
        *self.tails.lock() += 1;
        Some("step 1: compiling\nAPI_TOKEN=abc123secretvalue\n".into())
    }

    fn reply_hint(&self, agent: &AgentRef) -> String {
        format!("reply to {}", agent.target())
    }

    fn workspace(&self, _agent: &AgentRef) -> Option<WorkspaceRef> {
        *self.workspace_lookups.lock() += 1;
        self.workspace.clone()
    }

    fn toast(&self, text: &str) {
        self.toasts.lock().push(text.to_owned());
    }
}

fn webhook(name: &str, base: &str) -> Webhook {
    Webhook {
        name: name.into(),
        description: None,
        url: format!("{base}/{name}"),
        authorization: "Bearer secret".into(),
        signing_secret: None,
    }
}

fn config(base: &str) -> Config {
    Config {
        default_webhook: Some("bridge".into()),
        webhook: vec![webhook("bridge", base), webhook("research", base)],
        ..Config::default()
    }
}

fn blocked(watched: bool) -> Transition {
    Transition {
        agent: AgentRef {
            name: Some("api-refactor".into()),
            cwd: Some("/srv/api".into()),
            ..AgentRef::in_pane(Backend::Tui, "w1:p3".into())
        },
        status: AgentStatus::Blocked,
        watched,
        at_ms: now_ms(),
    }
}

/// Runs `handle` with auto-notify on, in a fresh state dir; returns the outcome and the pane's `last_auto_at`.
fn run(transition: Transition, source: &FakeSource, config: Config) -> (crate::error::Result<Outcome>, Option<i64>) {
    let state = tempfile::tempdir().unwrap();
    let env = Env::from([("NUDGE_STATE_DIR", state.path().to_str().unwrap())]);
    update_state(&env, |state| state.auto = true).unwrap();
    let run = FakeRun::default();
    let ctx = Context {
        env: &env,
        run: &run,
        home: Path::new("/home/me"),
        config_path: Path::new("/nonexistent/config.toml"),
    };
    let outcome = handle(transition, source, &ctx, Some(config));
    let cooldown = update_state(&env, |state| state.panes["tui:w1:p3"].last_auto_at).unwrap();
    (outcome, cooldown)
}

#[test]
fn a_settled_agent_is_sent_along_its_route_with_its_backends_hint_and_a_redacted_tail() {
    let hook = FakeWebhook::start(|_| (200, String::new()));
    let mut config = config(&hook.base);
    config.route = vec![Route {
        selector: Selector::Pane("api-refactor".into()),
        webhook: "research".into(),
        backend: None,
    }];
    let source = FakeSource::default();
    let (outcome, cooldown) = run(blocked(false), &source, config);
    let sent = Sent {
        http_status: 200,
        target: "api-refactor".into(),
    };
    assert_eq!(outcome.unwrap(), Outcome::Sent(sent));
    assert!(cooldown.is_some(), "the cooldown starts");
    let requests = hook.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].path, "/research");
    let body: serde_json::Value = serde_json::from_str(&requests[0].body).unwrap();
    assert_eq!(
        body["source"],
        serde_json::json!({ "backend": "tui", "host": host_name(), "pane": "w1:p3" })
    );
    assert_eq!(
        (&body["from_agent"], &body["reply_hint"], &body["cwd"], &body["webhook"]),
        (
            &"api-refactor".into(),
            &"reply to api-refactor".into(),
            &"/srv/api".into(),
            &"research".into()
        )
    );
    let message = body["message"].as_str().unwrap();
    assert!(
        message.contains("step 1: compiling") && !message.contains("abc123secretvalue"),
        "{message}"
    );
    assert_eq!(*source.toasts.lock(), ["Told the webhook: api-refactor is blocked"]);
    assert_eq!(
        *source.workspace_lookups.lock(),
        0,
        "nothing matches by workspace, so nothing is looked up"
    );
}

#[test]
fn outside_the_scope_nothing_is_read_or_sent_and_the_cooldown_is_given_back() {
    let hook = FakeWebhook::start(|_| (200, String::new()));
    let mut config = config(&hook.base);
    config.scope = Scope {
        workspaces: vec!["api-work".into()],
        agents: Vec::new(),
    };
    let source = FakeSource {
        workspace: Some(WorkspaceRef {
            id: "wE".into(),
            name: Some("web-work".into()),
        }),
        ..FakeSource::default()
    };
    let (outcome, cooldown) = run(blocked(false), &source, config);
    assert_eq!(outcome.unwrap(), Outcome::Skipped("outside nudge's scope".into()));
    assert_eq!(cooldown, None);
    assert_eq!(*source.workspace_lookups.lock(), 1);
    assert_eq!(*source.tails.lock(), 0);
    assert!(hook.requests().is_empty());
}

#[test]
fn a_watched_pane_is_skipped_before_anything_is_read() {
    let hook = FakeWebhook::start(|_| (200, String::new()));
    let source = FakeSource::default();
    let (outcome, cooldown) = run(blocked(true), &source, config(&hook.base));
    assert_eq!(
        outcome.unwrap(),
        Outcome::Skipped("you are looking at this pane".into())
    );
    assert_eq!(cooldown, None);
    assert_eq!((*source.tails.lock(), source.toasts.lock().len()), (0, 0));
    assert!(hook.requests().is_empty());
}

#[test]
fn a_failed_send_is_an_error_and_gives_back_the_cooldown() {
    let hook = FakeWebhook::start(|_| (500, "down".into()));
    let source = FakeSource::default();
    let (outcome, cooldown) = run(blocked(false), &source, config(&hook.base));
    assert!(matches!(outcome, Err(Error::Webhook(_))), "{outcome:?}");
    assert_eq!(cooldown, None, "the next event may notify");
    assert!(source.toasts.lock().is_empty());
}
