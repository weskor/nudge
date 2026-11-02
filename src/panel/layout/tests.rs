use super::parse;

#[test]
fn a_copy_made_by_layout_clone_is_your_own_and_a_panel_anywhere_counts_as_shown() {
    let shipped = r#"{"id":"root","type":"row","children":[{"id":"work","type":"block","content":{"tab":"active"}}]}"#;
    let shipped = parse(shipped, "main").unwrap();
    assert!(!shipped.is_own(), "Herdr's own layouts have no cloned_from");
    assert!(!shipped.shows_panel());
    let own = parse(
        r#"{"id":"root","type":"row","cloned_from":{"name":"main","version":"0.1.12"}}"#,
        "x",
    )
    .unwrap();
    assert!(own.is_own());
    let floating = r#"{"id":"root","type":"row","floating":[{"id":"n","content":{"mod":"nudge"},"edge":"right"}]}"#;
    assert!(parse(floating, "x").unwrap().shows_panel());
    let other = r#"{"id":"root","type":"row","children":[{"id":"n","type":"block","content":{"mod":"nudgey"}}]}"#;
    assert!(!parse(other, "x").unwrap().shows_panel());
}

#[test]
fn a_layout_nudge_cant_read_is_named_in_the_error() {
    assert!(
        parse("not json", "broken")
            .unwrap_err()
            .to_string()
            .contains("\"broken\"")
    );
}
