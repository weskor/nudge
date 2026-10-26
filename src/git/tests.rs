use super::{GitInfo, git_info, repo_from_remote};
use crate::env::tests::FakeRun;

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
