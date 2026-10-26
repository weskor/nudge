use super::{find_webhook, upsert_webhook};
use crate::config::parse::parse_config;
use crate::config::tests::{VALID, config_error, config_of, webhook};

#[test]
fn find_webhook_by_name_default_or_only_webhook() {
    let config = parse_config(VALID, "cfg").unwrap();
    assert_eq!(find_webhook(&config, Some("research")).unwrap().name, "research");
    assert_eq!(find_webhook(&config, None).unwrap().name, "bridge");
    let single = config_of(None, vec![webhook("research", "Bearer b")]);
    assert_eq!(find_webhook(&single, None).unwrap().name, "research");
}

#[test]
fn find_webhook_errors_name_the_configured_webhooks() {
    let two = config_of(
        None,
        vec![webhook("bridge", "Bearer a"), webhook("research", "Bearer b")],
    );
    assert!(config_error(find_webhook(&two, None)).contains("no default_webhook; pass --webhook"));
    let config = parse_config(VALID, "cfg").unwrap();
    assert!(config_error(find_webhook(&config, Some("nope"))).contains("Configured webhooks: bridge, research"));
}

#[test]
fn upsert_first_webhook_becomes_default_and_later_ones_only_when_asked() {
    let bridge = webhook("bridge", "Bearer a");
    let research = webhook("research", "Bearer b");
    let one = upsert_webhook(None, bridge.clone(), false);
    assert_eq!(one, config_of(Some("bridge"), vec![bridge.clone()]));
    let two = upsert_webhook(Some(one.clone()), research.clone(), false);
    assert_eq!(two, config_of(Some("bridge"), vec![bridge, research.clone()]));
    assert_eq!(
        upsert_webhook(Some(one), research, true).default_webhook.as_deref(),
        Some("research")
    );
}

#[test]
fn upsert_existing_name_replaces_it_in_place_and_keeps_other_settings() {
    let mut two = config_of(Some("research"), vec![webhook("bridge", "a"), webhook("research", "b")]);
    two.auto.toast = false;
    two.scope.agents = vec!["api".into()];
    let rotated = webhook("bridge", "Bearer rotated");
    let updated = upsert_webhook(Some(two.clone()), rotated.clone(), false);
    assert_eq!(updated.webhook, [rotated, webhook("research", "b")]);
    assert_eq!(
        (updated.default_webhook, updated.auto, updated.scope),
        (two.default_webhook, two.auto, two.scope)
    );
}
