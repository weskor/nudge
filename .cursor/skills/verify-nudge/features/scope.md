# Scope

The config's optional `[scope]` table limits which herdr agents may notify the webhook: `workspaces = [...]` (herdr
workspace labels or ids) and `agents = [...]` (agent names or pane ids). Both empty or absent means every agent may
send; otherwise an agent may send only if its name/pane id is in `agents` or its workspace's label/id is in
`workspaces`. Out-of-scope plugin events are skipped and logged, `nudge notify` and MCP `notify_webhook` refuse with an
"outside nudge's scope" error, and nothing reaches the webhook. The `send-pane` action and `nudge test` ignore scope. The
herdr actions `only-workspace`, `only-agent` and `all-agents` rewrite `[scope]` from the focused pane.

## Sub-features

- `scope-config` `[scope]` keys `workspaces`, `agents` (arrays of non-empty strings); any other key is a config error
  `scope.<key>: unknown setting`; wrong types are config errors too.
- `scope-lookup` the workspace of a pane comes from `herdr pane get <pane>` (`result.pane.workspace_id`) then
  `herdr workspace get <id>` (`result.workspace.label`, `result.workspace.workspace_id`); the agent from `herdr agent get <pane>`.
  (The 0.6.0 build takes `workspace_id` from `agent get` when the pane has an agent and skips `pane get`.)
- `scope-event` an out-of-scope `blocked`/`done` that would otherwise notify logs `[nudge] <pane> <status>: skipped
  (outside nudge's scope)`, exit 0, no request, no toast. Scope is checked only after the auto rules decide to notify,
  so auto-off / focused / short-turn / cooldown skips keep their own reasons.
- `scope-notify` `nudge notify` uses `HERDR_PANE_ID` (or the cwd-matched agent's pane); without a herdr pane it uses
  `$HERDR_WORKSPACE_ID`. Out of scope: exit 1, stderr `nudge: agent "<name>" in workspace "<id>" is outside nudge's
  scope, so nothing was sent. …`, no request. No herdr and no workspace with a non-empty scope is out of scope.
- `scope-mcp` `notify_webhook` follows the same rule; out of scope is an `isError: true` tool result.
- `scope-exempt` `plugin action send-pane` and `nudge test` send regardless of scope.
- `scope-actions` `only-workspace` writes `workspaces = ["<label>"]`, `agents = []` and toasts `Only workspace "<label>"
  can notify the webhook` (`No workspace for this pane` when the focused pane has none); `only-agent` writes
  `agents = ["<name>"]` and toasts `Only agent "<name>" can notify the webhook` (`No agent in this pane`); `all-agents`
  clears `[scope]` and toasts `All agents can notify the webhook`. Focused pane: `HERDR_PLUGIN_CONTEXT_JSON.focused_pane_id`,
  else `HERDR_PANE_ID`. The config must exist (else exit 1).
- `scope-doctor` doctor prints `Scope: all agents` or `Scope: workspaces a, b; agents x` (with a fix pointing at the
  `all-agents` action).

## How to get to it (user POV)

- Edit `~/.config/nudge/config.toml`: `[scope]` then `workspaces = ["api-work"]` and/or `agents = ["claude-1"]`.
- herdr actions `nudge.only-workspace`, `nudge.only-agent`, `nudge.all-agents` (key bindings or
  `herdr plugin action invoke`), run from the pane whose workspace/agent should be the only sender.
- `nudge doctor` shows the current scope.

## Driving it with verify-nudge

Preconditions:

- `$VN herdr workspaces` (workspace `wD` = `api-work` with `claude-1` in `wD:p1` and a shell in `wD:p3`; `wE` =
  `web-work` with `reviewer` in `wE:p2`; real herdr JSON shapes).
- `$VN seed-config --append '[scope]\nworkspaces = ["api-work"]'` (or `agents = [...]`); `$VN seed-state '{"auto": true}'` for events.
- `E() { printf '{"event":"pane.agent_status_changed","data":{"pane_id":"%s","agent_status":"%s"}}' "$1" "$2"; }`

- **Event in scope.** Run `$VN run --cwd home --env "HERDR_PLUGIN_EVENT_JSON=$(E wD:p1 blocked)" -- plugin event`. One request (`from_agent: "claude-1"`); `herdr_calls` include the workspace lookup (`workspace get wD`, plus `pane get wD:p1` unless the agent's `workspace_id` was used) and the toast `Told the webhook: claude-1 is blocked`.
- **Event out of scope.** Same with `$(E wE:p2 blocked)`. stderr `[nudge] wE:p2 blocked: skipped (outside nudge's scope)`, `webhook: []`, no `Told the webhook` toast.
- **Notify.** `$VN run --env HERDR_PANE_ID=wD:p1 --expect 0 -- notify in scope` sends; `--env HERDR_PANE_ID=wE:p2 --expect 1` prints the `outside nudge's scope` error and `webhook` is `[]`. Without a pane: `--env HERDR_WORKSPACE_ID=wD` (needs a fixture whose `agent list` matches no agent on the cwd, or run in another cwd).
- **MCP.** `$VN mcp --env HERDR_PANE_ID=wE:p2 --call notify_webhook '{"message":"hi"}'`: `["3"].result.isError` true with the scope text.
- **Exempt.** `$VN run --cwd home --env 'HERDR_PLUGIN_CONTEXT_JSON={"focused_pane_id":"wE:p2"}' -- plugin action send-pane` and `$VN run --env HERDR_PANE_ID=wE:p2 -- test` both send.
- **Actions.** `$VN run --cwd home --env 'HERDR_PLUGIN_CONTEXT_JSON={"focused_pane_id":"wD:p1"}' -- plugin action only-workspace`: `files.config.text` ends with `[scope]` / `workspaces = ["api-work"]`; toast `Only workspace "api-work" can notify the webhook`. Then `only-agent` (→ `agents = ["claude-1"]`), `only-agent` with `focused_pane_id` `wD:p3` (toast `No agent in this pane`, config unchanged), `all-agents` (scope cleared). Follow each with `$VN run -- list` (config still valid).
- **Doctor.** `$VN run -- doctor`: `✓ Scope: workspaces api-work` (or `Scope: all agents`).
- **Proof.** `webhook` (sent or `[]`), `stderr` (skip/refusal text), `herdr_calls` (lookups and toasts), `files.config.text` (rewritten scope).

## Gotchas

- Scope lookups are herdr calls; without matching `pane get` / `workspace get` rules the workspace is unknown and only
  `agents` can match. The `workspaces` preset has rules for `wD:p1`, `wD:p3`, `wE:p2`, `wD`, `wE` only.
- `--agent NAME` / `NUDGE_AGENT` are not covered by the contract; the 0.6.0 build lets `notify --agent claude-1` through
  `agents = ["claude-1"]` with no herdr agent (`scope-notify-agent-flag-only` records this without a check).
- Action toasts and lookups never touch a real herdr; prove them via `herdr_calls`.
