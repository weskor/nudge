use serde_json::Value;

use super::{block_line, dock, undock};
use crate::panel::layout::parse;

/// A layout of your own exactly as Herdr 0.1.12 wrote it: `layout.clone` of the default `main` layout.
const HERDR_CLONE: &str = include_str!("herdr-clone.json");

/// The block, as Herdr itself would write it as the last child of that layout's root row.
const HERDR_STYLE_BLOCK: &str = r#"    },
    {
      "id": "nudge-panel",
      "type": "block",
      "content": {
        "mod": "nudge"
      },
      "size": {
        "px": 320
      }
    }
  ],
  "background": "canvas","#;

fn value(text: &str) -> Value {
    serde_json::from_str(text).unwrap()
}

#[test]
fn install_then_remove_gives_back_herdr_s_own_file_byte_for_byte() {
    let docked = dock(HERDR_CLONE).unwrap();
    assert!(parse(&docked, "mine").unwrap().shows_panel());
    assert!(
        docked.contains(HERDR_STYLE_BLOCK),
        "formatted like its neighbours:\n{docked}"
    );
    let mut without = value(&docked);
    let children = without["children"].as_array_mut().unwrap();
    assert_eq!(children.pop().unwrap(), value(&block_line().unwrap()));
    assert_eq!(without, value(HERDR_CLONE), "nothing else changed");

    assert_eq!(undock(&docked).unwrap().unwrap(), HERDR_CLONE);
    assert_eq!(undock(HERDR_CLONE).unwrap(), None, "nothing to take out");
}

#[test]
fn any_formatting_round_trips_byte_for_byte() {
    let one_line = r#"{"id":"root","type":"row","cloned_from":{"name":"main"},"children":[{"id":"a","type":"block","content":{"tab":"active"}}]}"#;
    let empty = "{ \"id\": \"root\", \"type\": \"row\", \"children\": [] }\r\n";
    let tabs = "{\n\t\"type\": \"row\",\n\t\"children\": [\n\t\t{ \"id\": \"a\" },\n\t\t{ \"id\": \"b\" }\n\t]\n}";
    for text in [one_line, empty, tabs] {
        let docked = dock(text).unwrap();
        assert_eq!(
            value(&docked)["children"].as_array().unwrap().last().unwrap()["id"],
            "nudge-panel"
        );
        assert_eq!(undock(&docked).unwrap().unwrap(), text);
    }
    assert!(dock(one_line).unwrap().ends_with(
        r#"{"tab":"active"}},{"id":"nudge-panel","type":"block","content":{"mod":"nudge"},"size":{"px":320}}]}"#
    ));
}

#[test]
fn remove_takes_out_install_s_blocks_anywhere_and_keeps_every_other_byte() {
    let ours = r#"{"id":"nudge-panel","type":"block","content":{"mod":"nudge"}}"#;
    let theirs = r#"{"id":"left","type":"block","content":{"mod":"nudge"}}"#;
    let text = format!(
        "{{\"type\": \"row\", \"children\": [ {ours}, {ours},\n  {{\"id\": \"a\", \"floating\": [{ours}]}},  {theirs}, {ours} , {ours}\n],\n \"floating\": [ {ours} ]}}"
    );
    let kept = undock(&text).unwrap().unwrap();
    assert_eq!(
        kept,
        format!(
            "{{\"type\": \"row\", \"children\": [ {{\"id\": \"a\", \"floating\": []}},  {theirs}\n],\n \"floating\": []}}"
        ),
    );
    assert!(
        parse(&kept, "x").unwrap().shows_panel(),
        "a panel install didn't add stays"
    );
    assert_eq!(undock(&kept).unwrap(), None);
}

#[test]
fn a_layout_whose_top_isnt_a_row_with_children_gets_no_block() {
    for text in [
        r#"{"id":"root","type":"column","children":[{"id":"a","type":"block","content":{"tab":"active"}}]}"#,
        r#"{"id":"root","type":"row"}"#,
    ] {
        let error = dock(text).unwrap_err().to_string();
        assert!(error.contains("isn't a row with children"), "{error}");
    }
}
