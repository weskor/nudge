#[test]
fn utc_timestamps_match_the_calendar() {
    for (seconds, expected) in [
        (0, "1970-01-01T00:00:00Z"),
        (951_782_400, "2000-02-29T00:00:00Z"),
        (1_800_000_000, "2027-01-15T08:00:00Z"),
        (4_102_444_799, "2099-12-31T23:59:59Z"),
    ] {
        assert_eq!(super::utc_timestamp(seconds), expected);
    }
}
