use crate::webhook::status::NotifyStatus;
use crate::webhook::tests::{FakeWebhook, config, env, herdr_and_git};
use crate::webhook::{Origin, notify_webhook};

#[test]
fn every_notification_gets_its_own_id() {
    let hook = FakeWebhook::start(|_| (200, String::new()));
    let state = tempfile::tempdir().unwrap();
    let (env, run) = (env(&state, true), herdr_and_git());
    let origin = Origin {
        env: &env,
        cwd: "/home/user/src/api",
        run: &run,
    };
    let config = config(&hook.base);
    let ids: std::collections::BTreeSet<String> = (0..20)
        .map(|_| {
            notify_webhook(&config, None, NotifyStatus::Done, "x", None, origin)
                .unwrap()
                .notification
                .id
        })
        .collect();
    assert_eq!(ids.len(), 20);
}
