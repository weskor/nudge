//! The git repo and branch a notification comes from.
use std::path::Path;
use std::sync::LazyLock;
use std::thread;

use regex::Regex;

use crate::env::Run;
use crate::error::unreachable_regex;

static REMOTE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(?:[a-z][a-z0-9+.-]*://(?:[^@/]+@)?[^/]+/|[^@/:]+@[^:/]+:)(.+?)(?:\.git)?/?$")
        .unwrap_or_else(|error| unreachable_regex(&error))
});

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
mod tests;
