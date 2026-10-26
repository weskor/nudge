use super::reply_hint;

#[test]
fn reply_hint_shell_quotes_targets_that_need_it() {
    assert_eq!(
        reply_hint(Some("w1:p1")),
        "reply with: herdr agent prompt w1:p1 \"...\""
    );
    assert_eq!(
        reply_hint(Some("my agent")),
        "reply with: herdr agent prompt 'my agent' \"...\""
    );
    assert_eq!(
        reply_hint(Some("it's")),
        r#"reply with: herdr agent prompt 'it'\''s' "...""#
    );
}
