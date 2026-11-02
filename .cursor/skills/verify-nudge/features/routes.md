# Routes and the default webhook

`[[route]]` tables in the config send one pane's or one workspace's notifications to a webhook other than the default.
The most specific wins: the first pane route (agent name or pane id) in file order, else the first workspace route
(workspace label or id), else `default_webhook` (else the only webhook). `backend = "tui" | "core"` narrows a route to
herdr or herdr-core; without it a route matches both. `nudge notify --webhook NAME` and the MCP `webhook` argument
beat every route; `nudge test` pings the default. Plugin auto-notify, `send-pane`, `nudge notify` and MCP
`notify_webhook` resolve routes after the `[scope]` check. Every event body carries `webhook`, the name it went to.
`nudge route ls|set|rm` and `nudge default <webhook>` edit the config; `nudge settings` lists routes read-only.

## Sub-features

- `route-notify-precedence` pane route beats workspace route beats default; `--webhook` and `--agent` behave as on any
  notify (an explicit webhook wins; a named agent is routed by its name); `test` goes to the default.
- `route-notify-by-id` routes match pane ids (`wE:p2`) and workspace ids (`wD`), including a workspace from
  `$HERDR_WORKSPACE_ID` outside any pane.
- `route-notify-backend` a `backend = "core"` route never applies to herdr TUI senders; `backend = "tui"` does.
- `route-notify-with-scope` scope is checked first: an out-of-scope sender is refused even with a route.
- `route-mcp` `notify_webhook` without `webhook` follows the route; with one, it wins.
- `route-event` / `route-send-pane` plugin auto-notify and `send-pane` (which ignores scope) follow routes; a pane
  without an agent is routed by its workspace.
- `route-no-default` a route works with several webhooks and no default; an unrouted sender then gets the usual
  `no default_webhook; pass --webhook` error.
- `route-invalid-config` a route naming a missing webhook, with both or neither of `pane`/`workspace`, with an unknown
  backend or key, or repeating an earlier route's selector and backend is a config error naming `route.<index>`.
- `route-cli` `route ls` prints pane routes, then workspace routes, then `everything else → <default> (default webhook)`;
  `--json` is `{"default_webhook", "routes": [{pane|workspace, webhook, backend?}]}`. `route set` adds (`Added route:
  pane "x" → y`) or re-points the route with the same selector and backend in place (`Changed route: …`); the webhook
  must exist (exit 1). `route rm` needs the same selector and backend (`no route for …; see nudge route ls`, exit 1).
  Wrong option combinations exit 2. Every write re-validates the config.
- `default-cli` `nudge default <webhook>` sets `default_webhook` (exit 1 for an unknown webhook, 2 without exactly one
  name).
- `route-kept-by-writers` / `settings-routes` the scope actions, setup, `nudge default` and settings keep every route
  when they rewrite the config; settings shows a read-only "Routes" section under the webhooks (no cursor stop).

## How to get to it (user POV)

- `nudge route set --workspace client-a --webhook client-a`, `nudge route ls`, `nudge route rm --pane api-refactor`,
  `nudge default main`.
- Or edit `[[route]]` tables in `~/.config/nudge/config.toml` (see config.example.toml).

## Driving it with verify-nudge

Preconditions:

- `$VN seed-config` (main and research both post to `/hook`; tell them apart by the body's `webhook`, signed goes to
  `/signed`), `$VN herdr workspaces` (claude-1 in `wD:p1` / api-work, a shell in `wD:p3`, reviewer in `wE:p2` /
  web-work).

- **Set and list.** `$VN run -- route set --pane claude-1 --webhook research`, then `$VN run -- route ls`; `files.config`
  ends with the `[[route]]` table.
- **Notify.** `$VN run --env HERDR_PANE_ID=wD:p1 -- notify hi`: `webhook[0].body.webhook` is `research`. With
  `--webhook main` it's `main`.
- **Workspace route.** `$VN run -- route set --workspace api-work --webhook signed`, then
  `$VN run --env HERDR_PANE_ID=wD:p3 -- notify hi`: the request goes to `/signed`; `herdr_calls` include `pane get wD:p3`
  and `workspace get wD`.
- **Plugin.** `$VN seed-state '{"auto": true}'`, an event for `wD:p1`, and `plugin action send-pane` with
  `focused_pane_id` `wD:p3`: bodies name the routed webhooks.
- **Proof.** `webhook[].body.webhook` and `path`; `files.config.text`; `herdr_calls` (workspace lookups happen only
  when a route or the scope names a workspace).

## Gotchas

- Without routes nothing changes: no extra herdr calls, the same error order (a bad `--webhook` or missing default is
  reported before any herdr lookup).
- Older nudge versions ignore `[[route]]` and drop it when they rewrite the config.
