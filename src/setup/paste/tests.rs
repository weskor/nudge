use super::{Found, prefill, split_paste};

const URL: &str = "https://hooks.example.com/h/7f3c2a";
const HEADER: &str = "Bearer token-0123456789abcdef";

fn found(url: Option<&str>, authorization: Option<&str>) -> Found {
    Found {
        url: url.map(str::to_owned),
        authorization: authorization.map(str::to_owned),
    }
}

#[test]
fn a_combined_paste_yields_both_values_in_its_usual_shapes() {
    let both = found(Some(URL), Some(HEADER));
    for paste in [
        format!("{URL}\n{HEADER}\n"),
        format!("  {URL}  \n\n  Authorization: {HEADER}  "),
        format!("curl -X POST -H 'Authorization: {HEADER}' -H 'Content-Type: application/json' {URL}"),
        format!("curl \"{URL}\" -H \"authorization: {HEADER}\" -d '{{}}'"),
        format!("Webhook: {URL}\nSend it with the header Authorization: {HEADER}"),
        format!("POST to <{URL}> with {HEADER}."),
    ] {
        assert_eq!(split_paste(&paste), both, "{paste}");
    }
}

#[test]
fn a_paste_with_one_value_or_none_yields_only_what_checks_out() {
    assert_eq!(split_paste(URL), found(Some(URL), None));
    assert_eq!(
        split_paste(&format!("Authorization: {HEADER}")),
        found(None, Some(HEADER))
    );
    assert_eq!(split_paste("hello there, nothing to see"), found(None, None));
    assert_eq!(
        split_paste(&format!("http://example.com/hook\n{HEADER}")),
        found(None, Some(HEADER)),
        "plain http is only allowed for localhost"
    );
    assert_eq!(
        split_paste(&format!("Authorization: {URL}")),
        found(Some(URL), None),
        "a URL is never taken for the header"
    );
}

#[test]
fn the_clipboard_prefills_only_a_lone_url_a_lone_header_or_both() {
    assert_eq!(prefill(&format!(" {URL}\n")), found(Some(URL), None));
    assert_eq!(prefill(HEADER), found(None, Some(HEADER)));
    assert_eq!(prefill(&format!("authorization: {HEADER}")), found(None, Some(HEADER)));
    assert_eq!(prefill(&format!("{URL}\n{HEADER}")), found(Some(URL), Some(HEADER)));
    assert_eq!(
        prefill(&format!("see {URL} for details")),
        found(None, None),
        "a link in prose"
    );
    assert_eq!(prefill("sk-live-0123456789abcdef"), found(None, None), "a bare secret");
    assert_eq!(prefill("Bearer is a word in this sentence"), found(None, None));
}
