use super::serialize_config;
use crate::config::tests::{config_of, webhook};

#[test]
fn serialized_output_omits_absent_and_default_settings() {
    let mut config = config_of(None, vec![webhook("bridge", "Bearer a")]);
    let text = serialize_config(&config);
    for absent in ["default_webhook", "description", "signing_secret", "[auto]", "[scope]"] {
        assert!(!text.contains(absent), "{absent}");
    }
    config.auto.cooldown_seconds = 60;
    let text = serialize_config(&config);
    assert!(text.contains("[auto]\ncooldown_seconds = 60\n"), "{text}");
    assert!(!text.contains("min_turn_seconds"), "{text}");
}
