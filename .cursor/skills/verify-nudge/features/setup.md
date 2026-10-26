# Setup

`nudge setup` connects a webhook: it takes the Webhook URL and Authorization header (a 4-step wizard in a terminal,
clipboard-first; flags, a file, env vars or stdin otherwise), never shows the header, test-pings the webhook before
saving, and writes the config with mode 600 in a mode 700 directory. The wizard (and `--yes`) also handles herdr
auto-notify, the `~/.local/bin/nudge` link and making the webhook the default.

## Sub-features

- `setup-flags` saves from `--url`/`--authorization` (+ `--description`, `--webhook`).
- `setup-stdin` reads URL then Authorization from stdin lines (blank lines and surrounding spaces ignored); a flag
  value skips its line.
- `setup-test-gate` pings first: 401/403 → "rejected the Authorization header", 404/405/410 → "no webhook at that
  URL", other failures → reason; nothing saved, exit 1. `--no-test` saves without a ping.
- `setup-default` makes the first webhook default; `--default` moves the default; re-running a webhook name replaces it
  (missing values fall back to the saved ones).
- `setup-auth-prefix` strips a pasted `Authorization:` prefix (any case, spaces trimmed).
- `setup-validation` rejects non-https URLs (http only for localhost), non-printable header, placeholder values,
  bad webhook names.
- `setup-signing-secret` `--signing-secret VALUE` (printable ASCII, 16+ characters) saves `signing_secret` for the
  webhook, and the test ping is already signed; re-running setup without the flag keeps the saved secret; the flag
  again replaces it. The secret never appears in output; invalid values exit 1 with nothing saved.
- `setup-flags-file` `--authorization-file PATH` reads the header from a file (`-` = stdin, which must be a pipe or a
  redirect: with a terminal on stdin it exits 2, `… stdin is a terminal …`, so a typed header is never echoed);
  combining it with `--authorization` is a usage error. `NUDGE_WEBHOOK_URL` / `NUDGE_WEBHOOK_AUTHORIZATION` supply
  values from the environment. Precedence: flag > file > env > (scripted: stdin lines) > saved webhook > (wizard:
  clipboard).
- `setup-yes` `--yes`/`-y` saves with no screen (terminal or not) and applies step 3's defaults: auto-notify on if the
  herdr plugin is installed, links `~/.local/bin/nudge`; `--no-auto-notify`, `--no-install-command` turn them off.
  Missing values exit 1: `setup --yes needs the webhook URL: --url or NUDGE_WEBHOOK_URL` / `setup --yes needs the
  Authorization header: --authorization, --authorization-file or NUDGE_WEBHOOK_AUTHORIZATION`. Plain scripted setup
  (no terminal, no `--yes`) never touches auto-notify or the command link.
- `setup-tty-frame` a 4-step wizard, width min(screen, 96), title `Setup · Step N of 4`, a 22-column step list on the
  left: `✓ Connect webhook` / `▸ Name` / `○ Options` / `○ Test and save` (✓ done, ▸ current, ○ pending). Under the body: a
  rule, a note line, then the key help. Shift+Tab/Esc goes back a step and keeps every answer.
- `setup-tty-connect` step 1: intro `Where should nudge send agent events? Copy the URL and the` / `Authorization header
  from your webhook's settings: an AI routine,` / `a workflow automation or your own service.`; fields `Webhook URL`,
  `Authorization` (labels padded to 15). A dim line under a value names its source: `from the clipboard`, `pasted`,
  `saved`, `from the command line`, `from --authorization-file`, `from the environment`. Empty focused field: `press Enter
  to paste, or type`; empty unfocused: `not set`. Checked values end with `  ✓`; the header shows only `Bearer
  ••••••••cdef  (34 characters)`. Help `enter next  tab field  esc leave  ctrl-c quit`.
- `setup-tty-clipboard` on opening, the clipboard (macOS: `pbpaste`; elsewhere wl-paste, xclip, `env DISPLAY=:0
  xclip`) prefills the URL or header only if it holds a value that checks out; otherwise it is never shown. Enter on an
  empty field pastes the clipboard (note `Reading the clipboard…` meanwhile). Reads run in the background, so a slow
  tool never holds up the screen, and a read fills only fields that are still empty, never what was typed meanwhile,
  and nothing once step 1 is left (`setup-tty-slow-clipboard`: `clipboard-delay` in the sandbox makes the fake tools
  sleep).
- `setup-tty-paste` typed characters and bracketed pastes go into the focused field; Backspace, Ctrl-U empties it. A
  paste holding both the URL and the Authorization header fills both from any step: `✓ Found the URL and the
  Authorization header in what you pasted.` (on step 2: `…; step 1 has them now.`). The header never lands in a visible
  field: a header, or anything not starting with `http`, pasted into an empty URL field goes to Authorization (`That
  isn't a URL, so it went into the Authorization field, where it's never shown.`); a header pasted into Name or
  Description is refused (`✗ That looks like an Authorization header, so it wasn't pasted here. It belongs in step
  1.`). A header pasted into either step-1 field (or read from the clipboard) is stored as the header found in it, not
  the text around it: `-H "Authorization: Bearer …"` saves `Bearer …`. Parity: `setup-tty-paste-routing`,
  `setup-tty-authorization-file-terminal`, `setup-tty-paste-curl-header`.
- `setup-tty-name` step 2: intro `What should nudge call this webhook? You'll use the name with` / `--webhook and in nudge
  list. With one webhook, main is fine.`; `Name` (prefilled `main` or `--webhook`), `Description` (`optional`). Names
  are checked like `--webhook`; an existing one is refused (spaces around the name don't count: `research ` is
  `research`): `✗ "research" is already a webhook; pick another name, or run nudge setup --webhook research to change
  it`. Help `enter next  tab field  shift+tab/esc back  ctrl-c quit`. Parity: `setup-tty-name-with-spaces`.
- `setup-tty-options` step 3: `After saving, nudge will:` then `✓ Turn on auto-notify  herdr plugin enabled`, `✓ Install
  the nudge command into ~/.local/bin`, `✓ Make "main" the default webhook` (off: `○ Leave auto-notify off`, `○ Not install
  the nudge command`, `○ Keep "X" as the default webhook`). Not choices: `! herdr isn't running: for auto-notify, run nudge
  setup in herdr`, `! herdr plugin not installed: herdr plugin install weskor/nudge`, `✓ nudge command installed
  ~/.local/bin/nudge`. Note `Enter accepts these. e changes them.`; help `enter accept  e change  shift+tab/esc back  q
  quit`. After `e`: checkboxes `[x] Turn on auto-notify …`, help `↑↓ move  space tick  enter done  q quit`.
- `setup-tty-test` step 4: spinner `⠹ Sending a test ping to "main"…` (help `ctrl-c stop`). On failure: `The test ping
  failed. What now?` with `Try again` / `Back to step 1 (fix the header)` (or `(fix the URL)`) / `Save anyway`; the red
  reason in the note, e.g. `✗ the webhook rejected the Authorization header (returned HTTP 401: Invalid API key)`; help
  `↑↓ choose  enter go  shift+tab/esc back  q quit`. `--no-test` skips the ping. Saving happens with the screen up:
  if it fails, step 4 says `Your answers are kept. Fix what's in the way, then press Enter to try saving again.`, the
  note `✗ Not saved: <reason>`, help `enter try again  shift+tab/esc back  q quit` (`setup-tty-save-fails`, using
  the pty step `["config-dir", "readonly"|"writable"]`).
- `setup-tty-finish` after saving, the normal screen shows `✓ Test ping: HTTP 200, the webhook received it`, `✓ Saved
  "main" as the default webhook (~/.config/nudge/config.toml, mode 600)`, `✓ auto-notify on`, `✓ nudge command
  ~/.local/bin/nudge`, a blank line, `What happens next` and three bullets (`  • When an agent is blocked, or done after 2
  minutes while you're not watching, your webhook gets an event.`, `  • Agents can send their own: nudge notify "…" (or the
  MCP tool notify_webhook).`, `  • Check everything with nudge doctor; change options with nudge settings.`).
- `setup-tty-cancel` Ctrl-C (any step) or Esc on step 1 (`q` on steps 3–4) prints `Cancelled; nothing saved.`, exit 1;
  `--pane` then waits for a key.

## How to get to it (user POV)

- `nudge setup [--webhook NAME] [--url URL] [--authorization VALUE | --authorization-file PATH] [--description TEXT] [--signing-secret VALUE] [--default] [--no-test] [--yes] [--no-auto-notify] [--no-install-command]` in a terminal.
- `printf '%s\n%s\n' "$URL" "Bearer $TOKEN" | nudge setup` (no terminal).
- herdr action `nudge.setup` opens `nudge setup --pane` in an overlay (see plugin-actions.md for the action itself).

## Driving it with verify-nudge

Preconditions:

- Fresh session (no config). `URL=http://127.0.0.1:<port>/hook` from the `launch` output; `GOOD='Bearer good-token-0123456789abcdef'`.
- For the auto-notify checkbox: `$VN herdr running` (plugin registered). Without it the form says herdr isn't running.

- **Flags.** Run `$VN run --expect 0 -- setup --url "$URL" --authorization "$GOOD" --description "Main webhook"`. stdout `Test ping to "main": HTTP 200.` then `Saved webhook "main" (default) to <RUN>/home/.config/nudge/config.toml (mode 600).`; `webhook[0].body.status` is `test`; `files.config.mode` `600`, `dir_mode` `700`.
- **Stdin.** Run `$VN run --stdin "$URL\n$GOOD\n" --expect 0 -- setup`. Same result.
- **Rejected header.** Run `$VN run --expect 1 -- setup --url "$URL" --authorization 'Bearer wrong-token-0123456789abcdef'`. stderr `test ping failed, nothing saved: the webhook rejected the Authorization header (returned HTTP 401: …)`; `files.config` is `null`.
- **Wrong URL.** Run `$VN run --expect 1 -- setup --url "${URL%/hook}/nope" --authorization "$GOOD"`. stderr says `there's no webhook at that URL`.
- **No test.** Run `$VN run --expect 0 -- setup --url "$URL" --authorization 'Bearer wrong-token-0123456789abcdef' --no-test`. `webhook` is `[]`, config saved.
- **Two webhooks + default.** After the flags step run `$VN run -- setup --webhook research --url "$URL" --authorization "$GOOD"`, `$VN run -- list` (`* main`), `$VN run --stdin '' -- setup --webhook research --default`, `$VN run -- list` (`* research`).
- **Prefix stripped.** Run `$VN run -- setup --url "$URL" --authorization "Authorization: $GOOD"`. Saved `authorization` has no prefix; the ping used it.
- **Signing secret.** Run `$VN run --expect 0 -- setup --url "${URL%/hook}/signed" --authorization "$GOOD" --signing-secret setup-secret-fedcba9876543210`. The ping is `POST /signed` with `signature: "valid:setup"`; `files.config.text` has `signing_secret = "setup-secret-fedcba9876543210"`; `secret_leaks` is `[]`. Then `$VN run --stdin '' -- setup --description again`: the secret line is still there and the ping is still `valid:setup`. `--signing-secret too-short` exits 1 with `files.config: null`.
- **Wizard, clipboard.** `$VN herdr running; $VN clip "$URL"`, then `$VN pty --screen '^> .*Authorization +press Enter to paste' --clip "$GOOD" --send '\r' --screen 'Bearer ••••••••cdef  \(34 characters\)  ✓' --send '\r' --screen 'Step 2 of 4' --send '\r' --screen 'Enter accepts these' --send '\r' --expect-exit 0 -- setup`: the URL is prefilled from the clipboard when the wizard opens (`from the clipboard`), so it opens on the header. `output` has `✓ Test ping: HTTP 200, the webhook received it`, `✓ Saved "main" as the default webhook`, `✓ auto-notify on`, `✓ nudge command ~/.local/bin/nudge` and `What happens next`; `files.state.json.auto` is `true`; `files.command_link.target` is `<NUDGE_ROOT>/bin/nudge`; `secret_leaks` is `[]`.
- **Combined paste.** `$VN clip --clear`, then `$VN pty --screen 'Webhook URL' --send "\e[200~$URL\n$GOOD\e[201~" --screen 'Found the URL and the Authorization header' --send '\x03' --expect-exit 1 -- setup`.
- **Wizard, 401 then back.** `$VN clip --clear`, then `$VN pty --screen 'Webhook URL' --send "\e[200~$URL\e[201~\r" --screen 'Authorization +press Enter' --send 'Bearer wrong-token-0123456789abcdef\r' --screen 'Step 2 of 4' --send '\r' --screen 'Enter accepts' --send '\r' --screen 'The test ping failed' --send '\r' --screen '^> .*Authorization' --send "$GOOD\r" --screen 'Step 2 of 4' --send '\r' --screen 'Enter accepts' --send '\r' --expect-exit 0 -- setup --webhook manual`: a 401 preselects `Back to step 1 (fix the header)`, so Enter lands on the Authorization field, and typing replaces the rejected header (no Ctrl-U needed). Two requests (wrong, then good token); neither token appears in the screens or `output`.
- **Duplicate name.** After the flags step, run the wizard with `--screen 'Step 2 of 4' --send '\x15research\r'` once `research` exists: `✗ "research" is already a webhook; pick another name, or run nudge setup --webhook research to change it`.
- **Ctrl-C.** Run `$VN pty --screen 'Webhook URL' --send '\x03' --expect-exit 1 -- setup`: `output` is `Cancelled; nothing saved.`, `files.config` is `null`, `webhook` `[]`.
- **--yes.** `$VN herdr running`, then `$VN run --env NUDGE_WEBHOOK_URL="$URL" --env NUDGE_WEBHOOK_AUTHORIZATION="$GOOD" --expect 0 -- setup --yes`: auto-notify on, command linked. `--yes --no-auto-notify --no-install-command` leaves both alone. `$VN run --stdin '' --expect 1 -- setup --yes` stderr `setup --yes needs the webhook URL: --url or NUDGE_WEBHOOK_URL`.
- **Authorization file.** `$VN run --stdin "$GOOD\n" --expect 0 -- setup --url "$URL" --authorization-file -`; with `--authorization` too it exits 2 (`use --authorization or --authorization-file, not both`); a missing file exits 1 with `can't read --authorization-file`.
- **Proof.** Pair each setup record with a read-only `$VN run -- list` and `$VN files`.

## Gotchas

- Scripted mode is chosen whenever stdin is not a terminal: `$VN run` always gives a pipe or /dev/null, so a wizard
  journey must use `$VN pty` with `--screen` steps; drive it by what is on screen (`Step N of 4`, notes), never by timing.
- Send keys only after a `--screen` step, so the wizard is in raw mode and nothing is echoed. Esc must be its own
  `--send` (else it merges with the next bytes into an escape sequence).
- Ctrl-U is `\x15` (empties the field), Shift+Tab `\e[Z`; a bracketed paste is `\e[200~TEXT\e[201~`. There is no Ctrl-S.
- `q` quits only on steps 3 and 4; on steps 1–2 it is typed into the field. Esc on step 1 leaves, on later steps goes back.
- The clipboard prefills step 1 only when the wizard opens and only with values that check out; set it with `$VN clip`
  before launching, or use `--clip` between steps and press Enter on the empty field.
- The masked preview reveals the last 4 characters only for secrets of 16+ characters after the scheme.
- Step 3 offers `Install the nudge command` only when no `nudge` link exists in the sandbox home and none is on PATH;
  seed or remove it deliberately.
- The fake `/signed` webhook only knows `sign-secret-0123456789abcdef` (label `signed-webhook`) and
  `setup-secret-fedcba9876543210` (label `setup`); any other secret is reported `invalid` and answered 401, which
  setup reports as a rejected Authorization header.
