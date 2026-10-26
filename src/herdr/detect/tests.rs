use super::{Env, Run, Sender, detect_sender, in_scope};
use crate::config::Scope;
use crate::env::tests::FakeRun;

fn detect_agent(env: &Env, cwd: &str, run: &dyn Run) -> Option<String> {
    detect_sender(env, cwd, run).agent
}

fn agent_json(agent: &str) -> String {
    format!(r#"{{"id":"cli:agent:get","result":{{"agent":{agent}}}}}"#)
}

fn list_json(agents: &str) -> String {
    format!(r#"{{"id":"cli:agent:list","result":{{"agents":[{agents}]}}}}"#)
}

#[test]
fn sender_knows_the_agents_pane_and_workspace() {
    let agent = agent_json(r#"{"name":"api-refactor","pane_id":"wD:p1","workspace_id":"wD"}"#);
    let run = FakeRun::new([("herdr agent get wD:p1", agent.as_str())]);
    let sender = detect_sender(&Env::from([("HERDR_PANE_ID", "wD:p1")]), "/repo", &run);
    let expected = Sender {
        agent: Some("api-refactor".into()),
        pane_id: Some("wD:p1".into()),
        workspace_id: Some("wD".into()),
    };
    assert_eq!(sender, expected);
}

#[test]
fn scope_allows_listed_agents_and_workspaces_by_label_or_id() {
    let label = r#"{"result":{"workspace":{"workspace_id":"wD","label":"team-web"}}}"#;
    let run = FakeRun::new([("herdr workspace get wD", label)]);
    let sender = Sender {
        agent: Some("api-refactor".into()),
        pane_id: Some("wD:p1".into()),
        workspace_id: Some("wD".into()),
    };
    let allows = |workspaces: &[&str], agents: &[&str]| {
        let scope = Scope {
            workspaces: workspaces.iter().map(|&w| w.to_owned()).collect(),
            agents: agents.iter().map(|&a| a.to_owned()).collect(),
        };
        in_scope(&scope, &sender, &Env::default(), &run)
    };
    assert!(allows(&[], &[]), "an empty scope allows everyone");
    assert!(allows(&["team-web"], &[]));
    assert!(allows(&["wD"], &[]));
    assert!(allows(&[], &["api-refactor"]));
    assert!(allows(&[], &["wD:p1"]));
    assert!(allows(&["other"], &["api-refactor"]), "either list is enough");
    assert!(!allows(&["other"], &["reviewer"]));
    assert!(!in_scope(
        &Scope {
            workspaces: vec!["team-web".into()],
            agents: Vec::new()
        },
        &Sender::default(),
        &Env::default(),
        &run
    ));
}

#[test]
fn a_pane_without_an_agent_is_placed_in_its_workspace_through_herdr() {
    let pane = r#"{"result":{"pane":{"pane_id":"wD:p3","workspace_id":"wD"}}}"#;
    let label = r#"{"result":{"workspace":{"workspace_id":"wD","label":"api-work"}}}"#;
    let run = FakeRun::new([("herdr pane get wD:p3", pane), ("herdr workspace get wD", label)]);
    let shell = Sender {
        agent: None,
        pane_id: Some("wD:p3".into()),
        workspace_id: None,
    };
    let scope = Scope {
        workspaces: vec!["api-work".into()],
        agents: Vec::new(),
    };
    assert!(in_scope(&scope, &shell, &Env::default(), &run));
}

#[test]
fn scope_with_only_agents_never_asks_herdr_for_the_workspace() {
    let run = FakeRun::default();
    let sender = Sender {
        agent: Some("a".into()),
        pane_id: None,
        workspace_id: Some("wD".into()),
    };
    let scope = Scope {
        workspaces: Vec::new(),
        agents: vec!["a".into()],
    };
    assert!(in_scope(&scope, &sender, &Env::default(), &run));
    assert_eq!(run.calls(), Vec::<String>::new());
}

#[test]
fn nudge_agent_wins_without_calling_herdr() {
    let run = FakeRun::default();
    let env = Env::from([("NUDGE_AGENT", "me"), ("HERDR_PANE_ID", "w1:p1")]);
    assert_eq!(detect_agent(&env, "/repo", &run).as_deref(), Some("me"));
    assert!(run.calls().is_empty());
}

#[test]
fn agent_in_this_pane_by_name_else_pane_id() {
    let named = agent_json(r#"{"name":"api-refactor","pane_id":"w1:p1"}"#);
    let run = FakeRun::new([("herdr agent get w1:p1", named.as_str())]);
    assert_eq!(
        detect_agent(&Env::from([("HERDR_PANE_ID", "w1:p1")]), "/repo", &run).as_deref(),
        Some("api-refactor")
    );

    let unnamed = agent_json(r#"{"pane_id":"w1:p5","cwd":"/repo"}"#);
    let run = FakeRun::new([("herdr agent get w1:p5", unnamed.as_str())]);
    assert_eq!(
        detect_agent(&Env::from([("HERDR_PANE_ID", "w1:p5")]), "/repo", &run).as_deref(),
        Some("w1:p5")
    );
}

#[test]
fn a_pane_without_an_agent_or_an_unreachable_herdr_gives_none_not_the_pane_id() {
    let shell_pane = FakeRun::default();
    assert_eq!(
        detect_agent(&Env::from([("HERDR_PANE_ID", "w1:p2")]), "/repo", &shell_pane),
        None
    );
    assert_eq!(
        shell_pane.calls(),
        ["herdr agent get w1:p2"],
        "no cwd guessing inside a herdr pane"
    );

    let junk = FakeRun::new([("herdr agent get w1:p5", "not json")]);
    assert_eq!(
        detect_agent(&Env::from([("HERDR_PANE_ID", "w1:p5")]), "/repo", &junk),
        None
    );
}

#[test]
fn uses_herdr_bin_path_when_set() {
    let agent = agent_json(r#"{"name":"x","pane_id":"p"}"#);
    let run = FakeRun::new([("/opt/herdr agent get p", agent.as_str())]);
    let env = Env::from([("HERDR_PANE_ID", "p"), ("HERDR_BIN_PATH", "/opt/herdr")]);
    assert_eq!(detect_agent(&env, "/repo", &run).as_deref(), Some("x"));
}

#[test]
fn outside_a_pane_the_single_agent_whose_cwd_matches() {
    let list = list_json(
        r#"{"name":"other","pane_id":"w1:p1","cwd":"/elsewhere"},
           {"name":"builder","pane_id":"w2:p2","cwd":"/home/x","foreground_cwd":"/repo"}"#,
    );
    let run = FakeRun::new([("herdr agent list", list.as_str())]);
    assert_eq!(detect_agent(&Env::default(), "/repo", &run).as_deref(), Some("builder"));
}

#[test]
fn outside_a_pane_an_ambiguous_or_missing_cwd_match_gives_none() {
    let two = list_json(r#"{"name":"a","pane_id":"p1","cwd":"/repo"},{"name":"b","pane_id":"p2","cwd":"/repo"}"#);
    let none = list_json("");
    for outputs in [two.as_str(), none.as_str()] {
        assert_eq!(
            detect_agent(&Env::default(), "/repo", &FakeRun::new([("herdr agent list", outputs)])),
            None
        );
    }
    assert_eq!(detect_agent(&Env::default(), "/repo", &FakeRun::default()), None);
}
