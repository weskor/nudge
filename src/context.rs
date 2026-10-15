//! Where nudge runs: the environment, external commands (herdr, git, clipboard tools), the herdr agent to
//! reply to, and the git repo/branch.
use std::collections::HashMap;
use std::io::Read as _;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::LazyLock;
use std::thread;
use std::time::{Duration, Instant};

use regex::Regex;
use serde::Deserialize;
use serde::de::DeserializeOwned;

/// A snapshot of environment variables. Empty values count as unset.
#[derive(Clone, Debug, Default)]
pub(crate) struct Env(HashMap<String, String>);

impl Env {
    pub(crate) fn from_process() -> Self {
        Self(
            std::env::vars_os()
                .filter_map(|(key, value)| Some((key.into_string().ok()?, value.into_string().ok()?)))
                .collect(),
        )
    }

    pub(crate) fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(String::as_str).filter(|value| !value.is_empty())
    }

    /// `$HOME`, else the account's home directory, else `/`.
    pub(crate) fn home(&self) -> PathBuf {
        self.get("HOME")
            .map(PathBuf::from)
            .or_else(std::env::home_dir)
            .unwrap_or_else(|| PathBuf::from("/"))
    }

    /// The herdr CLI: `$HERDR_BIN_PATH` (set by herdr for plugin commands), else `herdr` on PATH.
    pub(crate) fn herdr(&self) -> &str {
        self.get("HERDR_BIN_PATH").unwrap_or("herdr")
    }

    /// The executable `name` on `$PATH`, like `which`.
    pub(crate) fn which(&self, name: &str) -> Option<PathBuf> {
        std::env::split_paths(self.get("PATH")?)
            .map(|dir| dir.join(name))
            .find(|path| {
                path.metadata()
                    .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
            })
    }
}

impl<const N: usize> From<[(&str, &str); N]> for Env {
    fn from(pairs: [(&str, &str); N]) -> Self {
        Self(
            pairs
                .into_iter()
                .map(|(key, value)| (key.to_owned(), value.to_owned()))
                .collect(),
        )
    }
}

/// Runs external commands. Tests substitute a fake keyed by the command line.
pub(crate) trait Run: Sync {
    /// Run `cmd` in `cwd`: its trimmed stdout on exit 0; `None` on failure, a missing binary or a timeout.
    fn run(&self, cmd: &[&str], cwd: &Path) -> Option<String>;
}

/// Runs real processes, giving each at most [`COMMAND_TIMEOUT`].
#[derive(Debug)]
pub(crate) struct System;

const COMMAND_TIMEOUT: Duration = Duration::from_secs(3);

impl Run for System {
    fn run(&self, cmd: &[&str], cwd: &Path) -> Option<String> {
        let (program, args) = cmd.split_first()?;
        let mut child = Command::new(program)
            .args(args)
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let mut stdout = child.stdout.take()?;
        let reader = thread::spawn(move || {
            let mut out = Vec::new();
            stdout.read_to_end(&mut out).map(|_| out)
        });
        let deadline = Instant::now() + COMMAND_TIMEOUT;
        let status = loop {
            if let Some(status) = child.try_wait().ok()? {
                break status;
            }
            if Instant::now() >= deadline {
                child.kill().ok();
                child.wait().ok();
                return None;
            }
            thread::sleep(Duration::from_millis(2));
        };
        let out = reader.join().ok()?.ok()?;
        status
            .success()
            .then(|| String::from_utf8_lossy(&out).trim().to_owned())
    }
}

/// Run `herdr <args>` and parse its JSON output; `None` if herdr fails or answers something unexpected.
pub(crate) fn herdr_json<T: DeserializeOwned>(args: &[&str], env: &Env, cwd: &Path, run: &dyn Run) -> Option<T> {
    let cmd: Vec<&str> = std::iter::once(env.herdr()).chain(args.iter().copied()).collect();
    serde_json::from_str(&run.run(&cmd, cwd)?).ok()
}

#[derive(Debug, Deserialize)]
struct HerdrAgent {
    name: Option<String>,
    pane_id: String,
    cwd: Option<String>,
    foreground_cwd: Option<String>,
    focused: Option<bool>,
}

impl HerdrAgent {
    /// What `herdr agent prompt` accepts for this agent: its name, else its pane id.
    fn target(self) -> String {
        self.name.filter(|name| !name.is_empty()).unwrap_or(self.pane_id)
    }
}

#[derive(Debug, Deserialize)]
struct AgentGet {
    result: AgentGetResult,
}

#[derive(Debug, Deserialize)]
struct AgentGetResult {
    agent: HerdrAgent,
}

#[derive(Debug, Deserialize)]
struct AgentList {
    result: AgentListResult,
}

#[derive(Debug, Deserialize)]
struct AgentListResult {
    agents: Vec<HerdrAgent>,
}

fn agent_in_pane(pane_id: &str, env: &Env, cwd: &Path, run: &dyn Run) -> Option<HerdrAgent> {
    herdr_json::<AgentGet>(&["agent", "get", pane_id], env, cwd, run).map(|got| got.result.agent)
}

/// The herdr agent target to reply to (`herdr agent prompt <target>`), in order:
/// 1. `$NUDGE_AGENT`;
/// 2. the agent in this pane (`$HERDR_PANE_ID`): its name, else the pane id;
/// 3. outside herdr panes, the single herdr agent whose cwd is `cwd`: its name, else its pane id.
///
/// `None` when none applies: a herdr pane without an agent (a plain shell), herdr unreachable, or several
/// agents sharing the cwd. Never guess: the desk types its reply into this target.
pub(crate) fn detect_agent(env: &Env, cwd: &str, run: &dyn Run) -> Option<String> {
    if let Some(agent) = env.get("NUDGE_AGENT") {
        return Some(agent.to_owned());
    }
    if let Some(pane_id) = env.get("HERDR_PANE_ID") {
        return agent_in_pane(pane_id, env, Path::new(cwd), run).map(HerdrAgent::target);
    }
    let list = herdr_json::<AgentList>(&["agent", "list"], env, Path::new(cwd), run)?;
    let mut here = list
        .result
        .agents
        .into_iter()
        .filter(|agent| agent.cwd.as_deref() == Some(cwd) || agent.foreground_cwd.as_deref() == Some(cwd));
    match (here.next(), here.next()) {
        (Some(agent), None) => Some(agent.target()),
        (Some(_), Some(_)) | (None, _) => None,
    }
}

/// The herdr agent in a pane, as the plugin needs it.
#[derive(Debug)]
pub(crate) struct AgentInfo {
    /// Name to reply to: the agent's name, else its pane id.
    pub(crate) target: String,
    /// The agent's working directory, when herdr knows it.
    pub(crate) cwd: Option<String>,
    /// Whether the pane is focused in the herdr client.
    pub(crate) focused: bool,
}

/// Look up the herdr agent in `pane_id`; `None` if herdr is unavailable or there is no agent there.
pub(crate) fn herdr_agent(pane_id: &str, env: &Env, cwd: &Path, run: &dyn Run) -> Option<AgentInfo> {
    let mut agent = agent_in_pane(pane_id, env, cwd, run)?;
    let agent_cwd = agent
        .foreground_cwd
        .take()
        .filter(|dir| !dir.is_empty())
        .or_else(|| agent.cwd.take());
    Some(AgentInfo {
        cwd: agent_cwd.filter(|dir| !dir.is_empty()),
        focused: agent.focused == Some(true),
        target: agent.target(),
    })
}

static REMOTE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(?:[a-z][a-z0-9+.-]*://(?:[^@/]+@)?[^/]+/|[^@/:]+@[^:/]+:)(.+?)(?:\.git)?/?$")
        .unwrap_or_else(|error| unreachable_regex(&error))
});

/// The literal patterns in this crate are covered by tests; a broken one is a programming error.
#[expect(clippy::panic, reason = "only reachable if a literal regex in the source is invalid")]
pub(crate) fn unreachable_regex(error: &regex::Error) -> Regex {
    panic!("invalid built-in regex: {error}")
}

/// "owner/name" from a hosted git remote URL (ssh, scp-style or https), else `None`.
pub(crate) fn repo_from_remote(remote: &str) -> Option<String> {
    REMOTE.captures(remote)?.get(1).map(|repo| repo.as_str().to_owned())
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct GitInfo {
    pub(crate) repo: Option<String>,
    pub(crate) branch: Option<String>,
}

/// Repo ("owner/name" from origin, else the checkout's directory name) and current branch.
pub(crate) fn git_info(cwd: &str, run: &dyn Run) -> GitInfo {
    let dir = Path::new(cwd);
    let Some(toplevel) = run.run(&["git", "rev-parse", "--show-toplevel"], dir) else {
        return GitInfo {
            repo: None,
            branch: None,
        };
    };
    let (remote, branch) = thread::scope(|scope| {
        let remote = scope.spawn(|| run.run(&["git", "remote", "get-url", "origin"], dir));
        let branch = run.run(&["git", "branch", "--show-current"], dir);
        (remote.join().ok().flatten(), branch)
    });
    let repo = remote
        .as_deref()
        .and_then(repo_from_remote)
        .filter(|repo| !repo.is_empty())
        .unwrap_or_else(|| {
            Path::new(&toplevel)
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default()
        });
    GitInfo {
        repo: Some(repo),
        branch: branch.filter(|branch| !branch.is_empty()),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::collections::HashMap;
    use std::path::Path;

    use parking_lot::Mutex;

    use super::{Env, GitInfo, Run, detect_agent, git_info, repo_from_remote};

    /// Fake command runner keyed by the joined argv; unknown commands fail like a missing binary.
    #[derive(Debug, Default)]
    pub(crate) struct FakeRun {
        outputs: HashMap<String, String>,
        pub(crate) calls: Mutex<Vec<String>>,
    }

    impl FakeRun {
        pub(crate) fn new<const N: usize>(outputs: [(&str, &str); N]) -> Self {
            Self {
                outputs: outputs
                    .into_iter()
                    .map(|(cmd, out)| (cmd.to_owned(), out.to_owned()))
                    .collect(),
                calls: Mutex::default(),
            }
        }

        pub(crate) fn calls(&self) -> Vec<String> {
            self.calls.lock().clone()
        }
    }

    impl Run for FakeRun {
        fn run(&self, cmd: &[&str], _cwd: &Path) -> Option<String> {
            let key = cmd.join(" ");
            let out = self.outputs.get(&key).cloned();
            self.calls.lock().push(key);
            out
        }
    }

    fn agent_json(agent: &str) -> String {
        format!(r#"{{"id":"cli:agent:get","result":{{"agent":{agent}}}}}"#)
    }

    fn list_json(agents: &str) -> String {
        format!(r#"{{"id":"cli:agent:list","result":{{"agents":[{agents}]}}}}"#)
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

    #[test]
    fn repo_from_hosted_remotes() {
        for (remote, repo) in [
            ("git@github.com:acme/api.git", Some("acme/api")),
            ("https://github.com/acme/api.git", Some("acme/api")),
            ("https://github.com/acme/api", Some("acme/api")),
            ("https://user:token@github.com/acme/api.git", Some("acme/api")),
            (
                "ssh://git@gitlab.example:2222/group/sub/proj.git",
                Some("group/sub/proj"),
            ),
            ("/srv/git/proj.git", None),
        ] {
            assert_eq!(repo_from_remote(remote).as_deref(), repo, "{remote}");
        }
    }

    #[test]
    fn git_info_from_origin_directory_name_or_nothing() {
        let run = FakeRun::new([
            ("git rev-parse --show-toplevel", "/home/user/src/api"),
            ("git remote get-url origin", "https://github.com/acme/api.git"),
            ("git branch --show-current", "main"),
        ]);
        let info = git_info("/home/user/src/api/src", &run);
        assert_eq!(
            info,
            GitInfo {
                repo: Some("acme/api".into()),
                branch: Some("main".into())
            }
        );

        let scratch = FakeRun::new([
            ("git rev-parse --show-toplevel", "/tmp/scratch"),
            ("git branch --show-current", ""),
        ]);
        assert_eq!(
            git_info("/tmp/scratch", &scratch),
            GitInfo {
                repo: Some("scratch".into()),
                branch: None
            }
        );

        assert_eq!(
            git_info("/tmp", &FakeRun::default()),
            GitInfo {
                repo: None,
                branch: None
            }
        );
    }
}
