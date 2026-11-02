use std::path::Path;

use super::TuiSource;
use crate::env::Env;
use crate::env::tests::FakeRun;
use crate::source::{AgentRef, AgentSource as _, Backend, WorkspaceRef};

const AGENT: &str = r#"{"result":{"agent":{"name":"","pane_id":"wD:p1","workspace_id":"wD","cwd":"/srv/api",
    "foreground_cwd":"","focused":true}}}"#;

#[test]
fn the_agent_in_a_pane_as_the_pipeline_sees_it() {
    let run = FakeRun::new([("herdr agent get wD:p1", AGENT)]);
    let env = Env::default();
    let source = TuiSource {
        env: &env,
        home: Path::new("/home/me"),
        run: &run,
    };
    let expected = AgentRef {
        backend: Backend::Tui,
        pane: "wD:p1".into(),
        name: None,
        workspace: Some(WorkspaceRef {
            id: "wD".into(),
            name: None,
        }),
        cwd: Some("/srv/api".into()),
    };
    assert_eq!(source.agent("wD:p1"), Some((expected, true)));
    assert_eq!(source.agent("wD:p9"), None, "no agent in the pane");
}

#[test]
fn workspace_is_looked_up_through_the_pane_only_when_the_agent_didnt_say() {
    let pane = r#"{"result":{"pane":{"pane_id":"wD:p3","workspace_id":"wD"}}}"#;
    let label = r#"{"result":{"workspace":{"workspace_id":"wD","label":"api-work"}}}"#;
    let run = FakeRun::new([("herdr pane get wD:p3", pane), ("herdr workspace get wD", label)]);
    let env = Env::default();
    let source = TuiSource {
        env: &env,
        home: Path::new("/home/me"),
        run: &run,
    };
    let api_work = Some(WorkspaceRef {
        id: "wD".into(),
        name: Some("api-work".into()),
    });
    let shell = AgentRef::in_pane(Backend::Tui, "wD:p3".into());
    assert_eq!(source.workspace(&shell), api_work);
    let known = AgentRef {
        workspace: Some(WorkspaceRef {
            id: "wD".into(),
            name: None,
        }),
        ..AgentRef::in_pane(Backend::Tui, "wD:p1".into())
    };
    assert_eq!(source.workspace(&known), api_work);
    assert_eq!(
        run.calls(),
        [
            "herdr pane get wD:p3",
            "herdr workspace get wD",
            "herdr workspace get wD"
        ]
    );
}

#[test]
fn tail_reply_hint_and_toast_go_through_the_herdr_cli() {
    let read = "herdr pane read w1:p3 --source recent-unwrapped --lines 200 --format text";
    let run = FakeRun::new([(read, "step 1\n")]);
    let env = Env::default();
    let source = TuiSource {
        env: &env,
        home: Path::new("/home/me"),
        run: &run,
    };
    let agent = AgentRef {
        name: Some("my agent".into()),
        ..AgentRef::in_pane(Backend::Tui, "w1:p3".into())
    };
    assert_eq!(source.tail(&agent, 200).as_deref(), Some("step 1\n"));
    assert_eq!(
        source.reply_hint(&agent),
        "reply with: herdr agent prompt 'my agent' \"...\""
    );
    source.toast("Nudge on");
    assert_eq!(run.calls(), [read, "herdr notification show nudge --body Nudge on"]);
}
