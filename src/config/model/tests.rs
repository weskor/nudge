#[test]
fn durations_in_words() {
    assert_eq!(
        [120, 60, 90, 1, 0].map(super::seconds_in_words),
        ["2 minutes", "1 minute", "90 seconds", "1 second", "0 seconds"]
    );
}
