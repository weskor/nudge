# Setup

`nudge setup` connects a desk: it takes the Webhook URL and Authorization header (a full-screen form in a terminal,
clipboard-first; flags or stdin otherwise), never shows the header, test-pings the webhook before saving, and writes
the config with mode 600 in a mode 700 directory. In a terminal the form also offers herdr auto-notify, the
`~/.local/bin/nudge` link and (for a desk that isn't the default while others exist) making it the default.

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
- `setup-signing-secret` `--signing-secret VALUE` (printable ASCII, 16+ characters) saves `signing_secret` for the
  desk, and the test ping is already signed; re-running setup without the flag keeps the saved secret; the flag
  again replaces it. The secret never appears in output; invalid values exit 1 with nothing saved.
- `setup-tty-form` a rounded box titled ` nudge setup `: two intro lines (`Your desk is the webhook that receives nudge's
  events: …`), then numbered sections. `1  Webhook  copy both from your desk's webhook settings`: `Webhook URL`,
  `Authorization`. `2  This desk`: `Name` (starts as `--desk`, default `desk`; then `will be added`, `already saved:
  saving updates it` or, in yellow, `replaces the saved desk with this name`), `Description`, and `[ ]/[x] Make it the
  default desk` when other desks are saved and this name isn't the default. `3  After saving`: `[x] Turn on auto-notify
  herdr plugin enabled|disabled` (plugin registered; else `!  herdr isn't running  … run nudge setup in herdr for
  auto-notify` or `!  herdr plugin not installed  herdr plugin install weskor/nudge`), `[x] Install the nudge command
  into ~/.local/bin` (no `nudge` on PATH or linked; else ` ✓  nudge command installed  ~/…`, long paths keep their
  end); then `[ Test and save ]` (`[ Save ]` with `--no-test`, `[ Save anyway ]` after an unblamed failure). Rows start
  3 columns in, under the section titles; field labels are padded to 15. Values are prefilled from flags or the saved
  desk; empty ones show `press Enter to paste, or type` (focused URL/Authorization), `not set` or `optional`; `  ✓`
  follows a checked URL/header, `  ✗` a rejected value. The focused text field shows the terminal's own cursor (not
  drawn, so not on the harness screen). Footer: the status or the field's problem (✓ green, ✗ red, other yellow), else
  a dim hint for the row under the cursor; then `↑↓ move  enter paste/next  space tick  ctrl-s test & save  esc quit`
  (`ctrl-s save` with `--no-test`).
- `setup-tty-clipboard` Enter on an empty URL/Authorization field reads the clipboard (wl-paste, then xclip, then
  `env DISPLAY=:0 xclip`); a valid value fills the field and moves on; an invalid one is not shown, only `✗ The clipboard
  doesn't hold the <field>: <problem>`; an empty clipboard says `No clipboard text found. Type or paste the <field> here.`
- `setup-tty-typing` typed characters and bracketed pastes go into the focused field; Backspace, Ctrl-U (empty the
  field); Enter checks the value and moves on only if it passed; ↑↓/Tab move (checking the field left).
- `setup-tty-hidden` the header is never drawn: dots while typed, `Bearer ••••••••<last4>  (N characters)` once checked;
  typing into a checked (or saved) header starts a new one.
- `setup-tty-test` Ctrl-S or Enter on the button checks every field (an empty or rejected one gets the cursor), then
  test-pings: 401/403 puts `✗ the webhook rejected the Authorization header (…)` on the Authorization field, 404/405/410
  `✗ there's no webhook at that URL (…)` on the URL; the same value can't be resent until edited. Other failures:
  `✗ <reason>. Press Enter to save it anyway.` with the cursor on `[ Save anyway ]`; any edit re-arms the test.
- `setup-tty-name` the name is checked like `--desk` (letters, digits, `-`, `_`); a rename saves a new desk and keeps
  the old one; naming it after another saved desk replaces that one.
- `setup-tty-finish` after saving, the normal screen shows `✓ Test ping: HTTP 200, the desk received it` (or `! Saved
  without a successful test ping`), `✓ Saved "<name>"[ as the default desk] (<path>, mode 600)`, `✓ auto-notify on` / `!
  auto-notify off` (writes state `auto`) or the herdr warning, `✓ nudge command ~/…` (linking it when ticked), then
  `Done. Check everything any time with nudge doctor.`
- `setup-tty-cancel` Ctrl-C leaves at once; Esc leaves at once if nothing was entered, else asks `Press Esc again to
  leave without saving.`; both print `Cancelled; nothing saved.`, exit 1; `--pane` then waits for a key.

## How to get to it (user POV)

- `nudge setup [--desk NAME] [--url URL] [--authorization VALUE] [--description TEXT] [--signing-secret VALUE] [--default] [--no-test]` in a terminal.
- `printf '%s\n%s\n' "$URL" "Bearer $TOKEN" | nudge setup` (no terminal).
- herdr action `weskor.nudge.setup` opens `nudge setup --pane` in an overlay (see plugin-actions.md for the action itself).

## Driving it with verify-nudge

Preconditions:

- Fresh session (no config). `URL=http://127.0.0.1:<port>/hook` from the `launch` output; `GOOD='Bearer good-token-0123456789abcdef'`.
- For the auto-notify checkbox: `$VN herdr running` (plugin registered). Without it the form says herdr isn't running.

- **Flags.** Run `$VN run --expect 0 -- setup --url "$URL" --authorization "$GOOD" --description "Main desk"`. stdout `Test ping to "desk": HTTP 200.` then `Saved desk "desk" (default) to <RUN>/home/.config/nudge/config.toml (mode 600).`; `webhook[0].body.status` is `test`; `files.config.mode` `600`, `dir_mode` `700`.
- **Stdin.** Run `$VN run --stdin "$URL\n$GOOD\n" --expect 0 -- setup`. Same result.
- **Rejected header.** Run `$VN run --expect 1 -- setup --url "$URL" --authorization 'Bearer wrong-token-0123456789abcdef'`. stderr `test ping failed, nothing saved: the webhook rejected the Authorization header (returned HTTP 401: …)`; `files.config` is `null`.
- **Wrong URL.** Run `$VN run --expect 1 -- setup --url "${URL%/hook}/nope" --authorization "$GOOD"`. stderr says `there's no webhook at that URL`.
- **No test.** Run `$VN run --expect 0 -- setup --url "$URL" --authorization 'Bearer wrong-token-0123456789abcdef' --no-test`. `webhook` is `[]`, config saved.
- **Two desks + default.** After the flags step run `$VN run -- setup --desk research --url "$URL" --authorization "$GOOD"`, `$VN run -- list` (`* desk`), `$VN run --stdin '' -- setup --desk research --default`, `$VN run -- list` (`* research`).
- **Prefix stripped.** Run `$VN run -- setup --url "$URL" --authorization "Authorization: $GOOD"`. Saved `authorization` has no prefix; the ping used it.
- **Signing secret.** Run `$VN run --expect 0 -- setup --url "${URL%/hook}/signed" --authorization "$GOOD" --signing-secret setup-secret-fedcba9876543210`. The ping is `POST /signed` with `signature: "valid:setup"`; `files.config.text` has `signing_secret = "setup-secret-fedcba9876543210"`; `secret_leaks` is `[]`. Then `$VN run --stdin '' -- setup --description again`: the secret line is still there and the ping is still `valid:setup`. `--signing-secret too-short` exits 1 with `files.config: null`.
- **Interactive, clipboard.** `$VN herdr running; $VN clip "$URL"`, then `$VN pty --screen 'Webhook URL +press Enter to paste' --send '\r' --screen '^> .*Authorization' --clip "$GOOD" --send '\r' --screen 'Bearer ••••••••cdef  \(34 characters\)  ✓' --send '\x13' --expect-exit 0 -- setup`. `output` ends with the `Saved "desk" as the default desk`, `auto-notify on`, `nudge command ~/.local/bin/nudge` and `Done.` lines; `files.state.json.auto` is `true`; `files.command_link.target` is `<NUDGE_ROOT>/bin/nudge`; `secret_leaks` is `[]`.
- **Interactive, typed + 401.** `$VN clip --clear`, then `$VN pty --screen 'Webhook URL' --send '\r' --screen 'No clipboard text found' --send "\e[200~$URL\e[201~\r" --screen '^> .*Authorization' --send 'Bearer wrong-token-0123456789abcdef' --send '\x13' --screen 'rejected the Authorization header' --send "$GOOD" --send '\x13' --expect-exit 0 -- setup --desk manual`. Two requests (wrong then good token); neither token text appears in the screens or `output`.
- **Ctrl-C.** Run `$VN pty --screen 'Webhook URL' --send '\x03' --expect-exit 1 -- setup`: `output` is `Cancelled; nothing saved.`, `files.config` is `null`, `webhook` `[]`.
- **Proof.** Pair each setup record with a read-only `$VN run -- list` and `$VN files`.

## Gotchas

- Scripted mode is chosen whenever stdin is not a terminal: `$VN run` always gives a pipe or /dev/null, so an
  interactive journey must use `$VN pty` with `--screen` steps (the form is full-screen, like settings).
- Send keys only after a `--screen` step, so the form is in raw mode and nothing is echoed. Esc must be its own `--send`.
- Ctrl-S is `\x13`, Ctrl-U `\x15`; a bracketed paste is `\e[200~TEXT\e[201~`.
- Clipboard text is read once per Enter on an empty field, so change it with `--clip` between the URL and header.
- The masked preview reveals the last 4 characters only for secrets of 16+ characters after the scheme.
- `Install the nudge command` is offered only when no `nudge` link exists in the sandbox home and none is on PATH; seed
  or remove it deliberately. The field rows always come first: count ↓ from `Webhook URL` to reach a checkbox.
- The fake `/signed` desk only knows `sign-secret-0123456789abcdef` (label `signed-desk`) and
  `setup-secret-fedcba9876543210` (label `setup`); any other secret is reported `invalid` and answered 401, which
  setup reports as a rejected Authorization header.
