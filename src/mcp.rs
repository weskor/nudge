//! `nudge mcp`: a stdio MCP server (newline-delimited JSON-RPC 2.0) with two tools, `notify_desk` and
//! `list_desks`.
use std::io::{BufRead as _, Write as _};

use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::VERSION;
use crate::config::{default_config_path, default_desk_name, load_config};
use crate::context::{Env, System};
use crate::error::Result;
use crate::notify::{Notification, NotifyStatus, Origin, current_dir, notify_desk};

const LATEST_PROTOCOL_VERSION: &str = "2025-11-25";
const SUPPORTED_PROTOCOL_VERSIONS: [&str; 5] = [
    LATEST_PROTOCOL_VERSION,
    "2025-06-18",
    "2025-03-26",
    "2024-11-05",
    "2024-10-07",
];
const METHOD_NOT_FOUND: i32 = -32601;
const INVALID_PARAMS: i32 = -32602;

fn tools() -> Value {
    let schema = "http://json-schema.org/draft-07/schema#";
    json!([
        {
            "name": "list_desks",
            "description": "List the desks notify_desk can reach, and which one is the default.",
            "inputSchema": { "$schema": schema, "type": "object", "properties": {} },
            "execution": { "taskSupport": "forbidden" },
        },
        {
            "name": "notify_desk",
            "description": "Notify your desk (the webhook automation that coordinates your agents) that you finished, \
                are blocked, or have a question. Sends your herdr agent name, cwd, repo and branch with the message; \
                the desk replies later by prompting you through herdr, so do not wait or poll for an answer.",
            "inputSchema": {
                "$schema": schema,
                "type": "object",
                "properties": {
                    "message": {
                        "type": "string",
                        "minLength": 1,
                        "description": "What you did, or exactly what you need. Self-contained.",
                    },
                    "status": {
                        "default": "done",
                        "description": "done, blocked, or question.",
                        "type": "string",
                        "enum": ["done", "blocked", "question"],
                    },
                    "desk": { "description": "Desk name from list_desks; omit for the default desk.", "type": "string" },
                    "agent": {
                        "description": "Your herdr agent name, if auto-detection would get it wrong.",
                        "type": "string",
                    },
                },
                "required": ["message"],
            },
            "execution": { "taskSupport": "forbidden" },
        },
    ])
}

fn text_result(text: &str, is_error: bool) -> Value {
    let content = json!([{ "type": "text", "text": text }]);
    if is_error {
        json!({ "content": content, "isError": true })
    } else {
        json!({ "content": content })
    }
}

fn json_type(value: Option<&Value>) -> &'static str {
    match value {
        None => "undefined",
        Some(Value::Null) => "null",
        Some(Value::Bool(_)) => "boolean",
        Some(Value::Number(_)) => "number",
        Some(Value::String(_)) => "string",
        Some(Value::Array(_)) => "array",
        Some(Value::Object(_)) => "object",
    }
}

#[derive(Debug)]
struct NotifyArgs {
    message: String,
    status: NotifyStatus,
    desk: Option<String>,
    agent: Option<String>,
}

/// Validate `notify_desk` arguments; problems are listed as "<message> at <field>".
fn notify_args(arguments: &Map<String, Value>) -> Result<NotifyArgs, Vec<String>> {
    let mut problems = Vec::new();
    let expected_string = |field: &str, value: Option<&Value>| {
        format!(
            "Invalid input: expected string, received {} at {field}",
            json_type(value)
        )
    };
    let message = match arguments.get("message") {
        Some(Value::String(text)) if !text.is_empty() => Some(text.clone()),
        Some(Value::String(_)) => {
            problems.push("Too small: expected string to have >=1 characters at message".to_owned());
            None
        }
        other => {
            problems.push(expected_string("message", other));
            None
        }
    };
    let status = match arguments.get("status") {
        None => Some(NotifyStatus::Done),
        Some(value) => {
            let status = value.as_str().and_then(NotifyStatus::parse);
            if status.is_none() {
                problems.push(r#"Invalid option: expected one of "done"|"blocked"|"question" at status"#.to_owned());
            }
            status
        }
    };
    let mut optional = |field: &str| match arguments.get(field) {
        None => None,
        Some(Value::String(text)) => Some(text.clone()),
        other => {
            problems.push(expected_string(field, other));
            None
        }
    };
    let desk = optional("desk");
    let agent = optional("agent");
    match (message, status) {
        (Some(message), Some(status)) if problems.is_empty() => Ok(NotifyArgs {
            message,
            status,
            desk,
            agent,
        }),
        _ => Err(problems),
    }
}

#[derive(Serialize)]
struct DeskSummary<'a> {
    name: &'a str,
    default: bool,
    description: &'a str,
}

#[derive(Serialize)]
struct Notified {
    ok: bool,
    desk: String,
    http_status: u16,
    #[serde(flatten)]
    notification: Notification,
}

fn pretty(value: &impl Serialize) -> String {
    serde_json::to_string_pretty(value).unwrap_or_default()
}

fn list_desks(env: &Env) -> Result<String> {
    let config = load_config(&default_config_path(env))?;
    let default_name = default_desk_name(&config);
    let desks: Vec<DeskSummary<'_>> = config
        .desk
        .iter()
        .map(|desk| DeskSummary {
            name: &desk.name,
            default: Some(desk.name.as_str()) == default_name,
            description: desk.description.as_deref().unwrap_or_default(),
        })
        .collect();
    Ok(pretty(&desks))
}

fn notify(args: NotifyArgs, env: &Env) -> Result<String> {
    let config = load_config(&default_config_path(env))?;
    let cwd = current_dir()?;
    let origin = Origin {
        env,
        cwd: &cwd,
        run: &System,
    };
    let sent = notify_desk(
        &config,
        args.desk.as_deref(),
        args.status,
        &args.message,
        args.agent,
        origin,
    )?;
    Ok(pretty(&Notified {
        ok: true,
        desk: sent.desk,
        http_status: sent.http_status,
        notification: sent.notification,
    }))
}

fn call_tool(params: Option<&Value>, env: &Env) -> Value {
    let name = params
        .and_then(|params| params.get("name"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let empty = Map::new();
    let arguments = match params.and_then(|params| params.get("arguments")) {
        None => Ok(&empty),
        Some(Value::Object(arguments)) => Ok(arguments),
        Some(other) => Err(format!(
            "Invalid input: expected object, received {}",
            json_type(Some(other))
        )),
    };
    let invalid = |problems: &str| {
        text_result(
            &format!(
                "MCP error {INVALID_PARAMS}: Input validation error: Invalid arguments for tool {name}: {problems}"
            ),
            true,
        )
    };
    let outcome = match (name, arguments) {
        ("list_desks", Ok(_)) => list_desks(env),
        ("notify_desk", Ok(arguments)) => match notify_args(arguments) {
            Ok(args) => notify(args, env),
            Err(problems) => return invalid(&problems.join("\n")),
        },
        ("list_desks" | "notify_desk", Err(problem)) => return invalid(&problem),
        (other, _) => return text_result(&format!("MCP error {INVALID_PARAMS}: Tool {other} not found"), true),
    };
    match outcome {
        Ok(text) => text_result(&text, false),
        Err(error) => text_result(&error.to_string(), true),
    }
}

/// The JSON-RPC response to one message; `None` for notifications and anything that isn't a request.
fn respond(message: &Value, env: &Env) -> Option<Value> {
    let id = message.get("id")?;
    let method = message.get("method")?.as_str()?;
    let params = message.get("params");
    let result = match method {
        "initialize" => {
            let requested = params
                .and_then(|params| params.get("protocolVersion"))
                .and_then(Value::as_str);
            let version = requested
                .filter(|v| SUPPORTED_PROTOCOL_VERSIONS.contains(v))
                .unwrap_or(LATEST_PROTOCOL_VERSION);
            json!({
                "protocolVersion": version,
                "capabilities": { "tools": { "listChanged": true } },
                "serverInfo": { "name": "nudge", "version": VERSION },
            })
        }
        "ping" => json!({}),
        "tools/list" => json!({ "tools": tools() }),
        "tools/call" => call_tool(params, env),
        _ => {
            let error = json!({ "code": METHOD_NOT_FOUND, "message": "Method not found" });
            return Some(json!({ "jsonrpc": "2.0", "id": id, "error": error }));
        }
    };
    Some(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
}

/// Serve over stdio until the client disconnects. Lines that aren't JSON are ignored.
pub(crate) fn run_mcp(env: &Env) -> Result<()> {
    let mut stdout = std::io::stdout().lock();
    for line in std::io::stdin().lock().lines() {
        let line = line?;
        let Some(response) = serde_json::from_str(&line)
            .ok()
            .and_then(|message| respond(&message, env))
        else {
            continue;
        };
        writeln!(stdout, "{response}")?;
        stdout.flush()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::respond;
    use crate::context::Env;

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
            "notify_desk",
            &json!({ "message": "", "status": "nah", "desk": 1 }),
        );
        assert_eq!(result["isError"], true);
        let text = result["content"][0]["text"].as_str().unwrap();
        for field in ["at message", "at status", "at desk"] {
            assert!(text.contains(field), "{text}");
        }
    }

    #[test]
    fn config_problems_are_tool_errors_not_protocol_errors() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("none.toml");
        let env = Env::from([("NUDGE_CONFIG", missing.to_str().unwrap())]);
        let result = call(&env, "list_desks", &json!({}));
        assert_eq!(result["isError"], true);
        assert!(result["content"][0]["text"].as_str().unwrap().contains("nudge setup"));
    }
}
