use serde_json::Value;

use super::{parse, to_text};

/// Herdr's `main` layout, cut down: a root row with the sidebar and the work column, which holds the Files /
/// Changes stack.
const MAIN: &str = r#"{
  "id": "root", "type": "row", "background": "chrome", "divider": { "width": 1, "color": "border" },
  "children": [
    { "id": "sidebar", "type": "column", "size": { "px": 268 }, "children": [
      { "id": "spaces", "type": "block", "content": { "mod": "spaces" } } ] },
    { "id": "work", "type": "column", "children": [
      { "id": "panel-body", "type": "stack", "active": "files", "children": [
        { "id": "files", "type": "block", "content": { "mod": "files" } } ] } ] }
  ]
}"#;

#[test]
fn docking_adds_the_panel_on_the_right_and_keeps_everything_else() {
    let mut layout = parse(MAIN, "main").unwrap();
    assert!(!layout.shows_panel());
    assert!(!layout.is_own(), "Herdr's own layouts have no cloned_from");
    layout.dock_panel().unwrap();
    assert!(layout.shows_panel());

    let saved: Value = serde_json::from_str(&to_text(&layout).unwrap()).unwrap();
    let before: Value = serde_json::from_str(MAIN).unwrap();
    let children = saved["children"].as_array().unwrap();
    assert_eq!(children.len(), 3);
    assert_eq!(
        children[2],
        serde_json::json!({ "id": "nudge-panel", "type": "block", "content": { "mod": "nudge" }, "size": { "px": 320 } })
    );
    assert_eq!(children[..2], before["children"].as_array().unwrap()[..]);
    for key in ["id", "type", "background", "divider"] {
        assert_eq!(saved[key], before[key], "{key}");
    }
}

#[test]
fn a_copy_made_by_layout_clone_is_your_own_and_a_floating_panel_counts_as_shown() {
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
fn a_layout_whose_top_isnt_a_row_is_left_alone() {
    let text = r#"{"id":"root","type":"column","children":[{"id":"a","type":"block","content":{"tab":"active"}}]}"#;
    let mut layout = parse(text, "stacked").unwrap();
    let error = layout.dock_panel().unwrap_err().to_string();
    assert!(error.contains("isn't a row"), "{error}");
    assert!(!layout.shows_panel());
    assert!(
        parse("not json", "broken")
            .unwrap_err()
            .to_string()
            .contains("\"broken\"")
    );
}
