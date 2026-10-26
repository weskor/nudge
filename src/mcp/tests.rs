use serde_json::{Value, json};

use super::respond;
use crate::env::Env;

fn call(env: &Env, name: &str, arguments: &Value) -> Value {
    let request = json!({ "jsonrpc": "2.0", "id": 7, "method": "tools/call", "params": { "name": name, "arguments": arguments } });
    respond(&request, env).unwrap()["result"].clone()
}

#[test]
fn negotiates_a_supported_protocol_version_else_the_latest() {
    let init = |version: &str| {
        let request =
            json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": version } });
        respond(&request, &Env::default()).unwrap()["result"]["protocolVersion"].clone()
    };
    assert_eq!(init("2025-03-26"), "2025-03-26");
    assert_eq!(init("2099-01-01"), "2025-11-25");
}

#[test]
fn notifications_get_no_response_and_unknown_methods_an_error() {
    let env = Env::default();
    assert_eq!(
        respond(
            &json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
            &env
        ),
        None
    );
    let unknown = respond(&json!({ "jsonrpc": "2.0", "id": 5, "method": "bogus" }), &env).unwrap();
    assert_eq!(unknown["error"]["code"], -32601);
}

#[test]
fn invalid_notify_arguments_are_tool_errors_naming_each_field() {
    let result = call(
        &Env::default(),
        "notify_webhook",
        &json!({ "message": "", "status": "nah", "webhook": 1 }),
    );
    assert_eq!(result["isError"], true);
    let text = result["content"][0]["text"].as_str().unwrap();
    for field in ["at message", "at status", "at webhook"] {
        assert!(text.contains(field), "{text}");
    }
}

#[test]
fn config_problems_are_tool_errors_not_protocol_errors() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("none.toml");
    let env = Env::from([("NUDGE_CONFIG", missing.to_str().unwrap())]);
    let result = call(&env, "list_webhooks", &json!({}));
    assert_eq!(result["isError"], true);
    assert!(result["content"][0]["text"].as_str().unwrap().contains("nudge setup"));
}
