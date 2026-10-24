# MCP server

`nudge mcp` is a stdio MCP server (server name `nudge`, version = nudge version) with two tools: `list_webhooks`
returns the webhooks and the default, and `notify_webhook` sends a notification like `nudge notify` and returns the sent
payload. Config and webhook failures come back as tool results with `isError: true`, not protocol errors.

## Sub-features

- `mcp-initialize` answers `initialize` with `serverInfo {name: "nudge", version}` and tools capability.
- `mcp-tools-list` lists `list_webhooks` and `notify_webhook` (input `message` required, `status` default `done`,
  optional `webhook`, `agent`).
- `mcp-list-webhooks` returns pretty JSON text `[{name, default, description}]`.
- `mcp-notify` returns `{ok: true, webhook, http_status, id, sent_at, from_agent, cwd, repo, branch, status, message,
  reply_hint}` and POSTs the same body as `nudge notify` (same `id`/`sent_at`; signed for a webhook with `signing_secret`).
- `mcp-scope` with `HERDR_PANE_ID` (or `HERDR_WORKSPACE_ID`) outside `[scope]`, `notify_webhook` is `isError: true` with
  `… outside nudge's scope …` and sends nothing; `list_webhooks` still works.
- `mcp-errors` webhook failure / unknown webhook / missing config → `isError: true` with the error text.
- `mcp-exit` exits 0 when stdin closes.

## How to get to it (user POV)

- `claude mcp add nudge -- nudge mcp` (or any MCP client) and call the tools.

## Driving it with verify-nudge

Preconditions:

- `$VN seed-config`; optionally `$VN herdr running` (agent detection then finds no agent).

- **Session.** Run `$VN mcp --call list_webhooks '{}' --call notify_webhook '{"message":"Done via MCP","status":"question","agent":"mcp-agent"}' --call notify_webhook '{"message":"goes nowhere","webhook":"broken"}'`. `exit` 0; `responses_by_id["1"].result.serverInfo` is `{name: "nudge", version: <output of nudge version>}`; `["2"]` lists both tools; `["3"]` `text_json` lists seven webhooks with `main` default; `["4"]` `text_json.ok` true, `http_status` 200, `from_agent` `mcp-agent`; `["5"]` `isError: true` text `webhook "broken" returned HTTP 500: webhook exploded`.
- **Webhook received it.** The same record's `webhook` holds exactly one `POST /hook` (status `question`) and one `POST /fail`; `["4"]`'s `text_json.id`/`sent_at` (`<uuid>`/`<sent_at>`) are the request's (the record's `contract_problems` says otherwise if not).
- **Signed / scope.** `--call notify_webhook '{"message":"x","webhook":"signed"}'` gives a `/signed` request with `signature: "valid:signed-webhook"`. With `$VN herdr workspaces` and `$VN seed-config --append '[scope]\nworkspaces = ["web-work"]'`, `$VN mcp --env HERDR_PANE_ID=wD:p1 --call notify_webhook '{"message":"x"}'` is `isError` with the scope text and `webhook` `[]`.
- **No config.** On a fresh session run `$VN mcp --call list_webhooks '{}'`. `["3"].result.isError` is true with `no config at …`.
- **Proof.** `.verify/<id>/NNN-mcp.json` (requests sent, responses by id, webhook, stderr).

## Gotchas

- The helper waits for each response by id (20 s) and then closes stdin; a server that does not exit within 10 s is
  killed and its `exit` reads `still running 10s after stdin closed (killed)`.
- `tools/list` schemas (`$schema` draft-07, `execution.taskSupport: "forbidden"`) and `capabilities`
  (`tools.listChanged`) are part of the compared output; any change to the tool definitions shows up in the parity diff.
- Tool text that is JSON is compared parsed (`text_json`); the raw pretty-printing is not compared.
