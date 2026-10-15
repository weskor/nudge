# Setup

`nudge setup` connects a desk: it takes the Webhook URL and Authorization header (clipboard-first in a terminal,
flags or stdin otherwise), never shows the header, test-pings the webhook before saving, and writes the config with
mode 600 in a mode 700 directory. In a terminal it then offers herdr auto-notify and the `~/.local/bin/nudge` link.

## Sub-features

- `setup-flags` saves from `--url`/`--authorization` (+ `--description`, `--desk`).
- `setup-stdin` reads URL then Authorization from stdin lines (blank lines and surrounding spaces ignored); a flag
  value skips its line.
- `setup-test-gate` pings first: 401/403 → "rejected the Authorization header", 404/405/410 → "no webhook at that
  URL", other failures → reason; nothing saved, exit 1. `--no-test` saves without a ping.
- `setup-default` makes the first desk default; `--default` moves the default; re-running a desk name replaces it
  (missing values fall back to the saved ones).
- `setup-auth-prefix` strips a pasted `Authorization:` prefix (any case, spaces trimmed).
- `setup-validation` rejects non-https URLs (http only for localhost), non-printable header, placeholder values,
  bad desk names.
- `setup-tty-clipboard` Enter on an empty prompt reads the clipboard (wl-paste, then xclip, then `env DISPLAY=:0 xclip`).
- `setup-tty-manual` with no clipboard text asks to paste/type; Enter keeps the current value on update.
- `setup-tty-hidden` the header is never echoed; only `Bearer ••••••••<last4> (N characters)` is shown.
- `setup-tty-reask` re-asks the blamed value after 401 (header) or 404 (URL); 500 asks `Save anyway? [y/N]`.
- `setup-tty-extras` asks `Turn on auto-notify?` when the herdr plugin is registered (writes `auto` to state) and
  offers to link `~/.local/bin/nudge` when no `nudge` is on PATH or linked.
- `setup-tty-cancel` Ctrl-C at any prompt prints `Cancelled; nothing saved.`, exit 1; `--pane` then waits for a key.

## How to get to it (user POV)

- `nudge setup [--desk NAME] [--url URL] [--authorization VALUE] [--description TEXT] [--default] [--no-test]` in a terminal.
- `printf '%s\n%s\n' "$URL" "Bearer $TOKEN" | nudge setup` (no terminal).
- herdr action `weskor.nudge.setup` opens `nudge setup --pane` in an overlay (see plugin-actions.md for the action itself).

## Driving it with verify-nudge

Preconditions:

- Fresh session (no config). `URL=http://127.0.0.1:<port>/hook` from the `launch` output; `GOOD='Bearer good-token-0123456789abcdef'`.
- For the extras prompts: `$VN herdr running` (plugin registered). Without it setup says herdr isn't running.

- **Flags.** Run `$VN run --expect 0 -- setup --url "$URL" --authorization "$GOOD" --description "Main desk"`. stdout `Test ping to "desk": HTTP 200.` then `Saved desk "desk" (default) to <RUN>/home/.config/nudge/config.toml (mode 600).`; `webhook[0].body.status` is `test`; `files.config.mode` `600`, `dir_mode` `700`.
- **Stdin.** Run `$VN run --stdin "$URL\n$GOOD\n" --expect 0 -- setup`. Same result.
- **Rejected header.** Run `$VN run --expect 1 -- setup --url "$URL" --authorization 'Bearer wrong-token-0123456789abcdef'`. stderr `test ping failed, nothing saved: the webhook rejected the Authorization header (returned HTTP 401: …)`; `files.config` is `null`.
- **Wrong URL.** Run `$VN run --expect 1 -- setup --url "${URL%/hook}/nope" --authorization "$GOOD"`. stderr says `there's no webhook at that URL`.
- **No test.** Run `$VN run --expect 0 -- setup --url "$URL" --authorization 'Bearer wrong-token-0123456789abcdef' --no-test`. `webhook` is `[]`, config saved.
- **Two desks + default.** After the flags step run `$VN run -- setup --desk research --url "$URL" --authorization "$GOOD"`, `$VN run -- list` (`* desk`), `$VN run --stdin '' -- setup --desk research --default`, `$VN run -- list` (`* research`).
- **Prefix stripped.** Run `$VN run -- setup --url "$URL" --authorization "Authorization: $GOOD"`. Saved `authorization` has no prefix; the ping used it.
- **Interactive, clipboard.** `$VN herdr running; $VN clip "$URL"`, then `$VN pty --expect '1/2 Copy the Webhook URL[^\n]*›' --send '\r' --expect '2/2 Copy the Authorization header[^\n]*›' --clip "$GOOD" --send '\r' --expect 'Turn on auto-notify\?[^\n]*›' --send '\r' --expect 'Install the[^\n]*›' --send '\r' --expect 'Done\.' --expect-exit 0 -- setup`. Output shows the URL, `Bearer ••••••••cdef (34 characters)`, `Testing… ✓ HTTP 200`, `Saved "desk" as the default desk`; `files.state.json.auto` is `true`; `files.command_link.target` is `<NUDGE_ROOT>/bin/nudge`; `secret_leaks` is `[]`.
- **Interactive, typed + re-ask.** `$VN clip --clear`, then `$VN pty --expect '1/2 Copy the Webhook URL[^\n]*›' --send '\r' --expect 'Paste or type the Webhook URL[^\n]*›' --send "$URL\r" --expect '2/2 Copy the Authorization header[^\n]*›' --send 'Bearer wrong-token-0123456789abcdef\r' --expect 'rejected the Authorization header' --expect '↻ Copy the Authorization header[^\n]*›' --send "$GOOD\r" --expect 'Install the[^\n]*›' --send 'n\r' --expect 'Done\.' -- setup --desk manual`. Two requests (wrong then good token); neither token text appears in `output`.
- **Ctrl-C.** Run `$VN pty --expect '1/2 Copy the Webhook URL[^\n]*›' --send '\x03' --expect 'Cancelled; nothing saved\.' --expect-exit 1 -- setup`. `files.config` is `null`, `webhook` `[]`.
- **Proof.** Pair each setup record with a read-only `$VN run -- list` and `$VN files`.

## Gotchas

- Scripted mode is chosen whenever stdin is not a terminal: `$VN run` always gives a pipe or /dev/null, so an
  interactive journey must use `$VN pty`.
- In a PTY always `--expect` the `›` prompt before `--send`; typing before raw mode lets the kernel echo it and
  makes the hidden-header proof meaningless.
- Clipboard text is read once per Enter, so change it with `--clip` between the URL and header prompts.
- The masked preview reveals the last 4 characters only for secrets of 16+ characters after the scheme.
- `Install the nudge command` is skipped when a `nudge` link already exists in the sandbox home; seed or remove it
  deliberately.
