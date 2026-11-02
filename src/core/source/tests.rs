use std::path::Path;

use super::{CoreSource, core_reply_hint, core_sender};
use crate::env::Env;
use crate::env::tests::FakeRun;
use crate::source::{AgentRef, AgentSource as _, Backend, WorkspaceRef};

const T7: &str = r#"{"address":"t7","name":"api","labels":{"herdr.space":"s1"},"cwd":"/start","last_cwd":"",
    "state":"live","status":{"state":"blocked"}}"#;

fn api() -> AgentRef {
    AgentRef {
        backend: Backend::Core,
        pane: "t7".into(),
        name: Some("api".into()),
        workspace: Some(WorkspaceRef {
            id: "s1".into(),
            name: None,
        }),
        cwd: Some("/start".into()),
    }
}

#[test]
fn a_terminal_through_hc_get_its_output_and_the_reply_hint() {
    let read = "hc read t7 --lines 200";
    let run = FakeRun::new([
        ("hc get t7 --json", T7),
        (
            read,
            "$ make\nerror: boom\n[hc] only its screen: it is blocked; history is not scrolled in this state",
        ),
    ]);
    let source = CoreSource {
        home: Path::new("/home/me"),
        run: &run,
    };
    assert_eq!(source.agent("t7"), Some(api()));
    assert_eq!(source.agent("t9"), None);
    assert_eq!(
        source.tail(&api(), 200).as_deref(),
        Some("$ make\nerror: boom"),
        "hc's own notes are dropped"
    );
    assert_eq!(source.reply_hint(&api()), "reply with: hc send t7 \"...\"");
    assert_eq!(core_reply_hint("box/t7"), "reply with: hc send 'box/t7' \"...\"");
    assert_eq!(
        source.workspace(&api()),
        api().workspace,
        "core knows the space id only, without a lookup"
    );
}

#[test]
fn nudge_notify_inside_a_core_terminal_is_from_that_terminal() {
    let run = FakeRun::new([("hc get t7 --json", T7)]);
    let in_core = Env::from([("HC_TERMINAL_ID", "t7")]);
    assert_eq!(core_sender(&in_core, &run), Some(api()));
    let unknown = Env::from([("HC_TERMINAL_ID", "t8")]);
    assert_eq!(
        core_sender(&unknown, &run),
        Some(AgentRef::in_pane(Backend::Core, "t8".into())),
        "hc can't describe it: still that terminal"
    );
    let in_herdr = Env::from([("HC_TERMINAL_ID", "t7"), ("HERDR_PANE_ID", "w1:p1")]);
    assert_eq!(core_sender(&in_herdr, &run), None, "a herdr pane is the TUI's");
    assert_eq!(core_sender(&Env::default(), &run), None);
}
