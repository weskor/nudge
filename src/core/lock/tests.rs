use super::{Locked, lock_watch};

#[test]
fn a_second_watcher_finds_the_first_and_takes_over_once_it_is_gone() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("watch.lock");
    let Locked::Here(first) = lock_watch(&path).unwrap() else {
        panic!("the first watcher gets the lock");
    };
    let pid = std::process::id();
    assert!(matches!(lock_watch(&path).unwrap(), Locked::Elsewhere(Some(held)) if held == pid));
    drop(first);
    assert!(matches!(lock_watch(&path).unwrap(), Locked::Here(_)));
}
