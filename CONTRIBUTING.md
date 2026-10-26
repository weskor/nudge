# Contributing

Thanks for helping. Bug reports, ideas and pull requests are all welcome.

## Development setup

You need [rustup](https://rustup.rs) (it installs the Rust version pinned in `rust-toolchain.toml`, with rustfmt and clippy), Python 3 for the verification harness, and [herdr](https://herdr.dev) 0.9 or later.

```sh
git clone https://github.com/weskor/nudge && cd nudge
cargo build --release
herdr plugin link "$PWD"      # herdr runs target/release/nudge through bin/nudge
```

`herdr plugin link` skips the build step, so a linked checkout runs `target/release/nudge` through `bin/nudge`. Rebuild with `cargo build --release` after a change. If `dist/nudge` exists (from `scripts/install.sh`), `bin/nudge` prefers it; delete `dist/` to use your build.

## Checks

```sh
cargo fmt --check
scripts/check-sizes.sh
cargo clippy --all-targets -- -D warnings
cargo test
NUDGE_BIN=$PWD/target/release/nudge .cursor/skills/verify-nudge/bin/verify-nudge parity /tmp/parity.json
```

CI runs all five on Linux, and clippy and the tests on macOS too. The lint set in `Cargo.toml` is strict on purpose (no `unwrap`, no panics, no unchecked indexing, no `as` casts, exhaustive matches). An exception is a `#[expect(lint, reason = "...")]` on the item it excuses. Files stay under 300 lines (`scripts/check-sizes.sh`) and functions under 50 (`too-many-lines-threshold` in `clippy.toml`). [AGENTS.md](AGENTS.md) lists the project rules and what enforces each.

The last command drives the real binary through every user journey in the harness's scenario list, each in a sandbox with a fake webhook, fake herdr and fake clipboard. See [`.cursor/skills/verify-nudge`](.cursor/skills/verify-nudge/SKILL.md). To check that a change keeps behavior, run it before and after and `diff -u` the two transcripts.

## Layout

Each area is a folder whose `mod.rs` is the facade (submodules are private); each file owns one thing and says so in its first `//!` line. Unit tests sit next to their module in `tests.rs` (`foo.rs` → `foo/tests.rs`).

| Path | What |
| --- | --- |
| `src/main.rs`, `src/cli/` | Entry point; usage text, argument parsing and the `nudge` commands |
| `src/setup/` | `nudge setup`: `wizard/` (the wizard's state, keys and steps), `view/` (drawing it and the terminal loop), the scripted path, and saving |
| `src/settings/` | `nudge settings`: rows, keys, rendering, saving and the terminal loop |
| `src/terminal/` | `ui/`, the full-screen look setup and settings share, and `tty.rs`, plain terminal output and prompts |
| `src/doctor/` | `nudge doctor` checks |
| `src/plugin/` | herdr hooks and actions (`nudge plugin …`): status events, actions, send-pane, startup |
| `src/herdr/` | herdr queries: agents, workspaces, sender detection, scope, plugin registration and logs, the `~/.local/bin/nudge` link |
| `src/auto/` | Auto-notify rules, redaction and the pane message |
| `src/webhook/` | The notification payload, sending and signing, and `notify`/`ping` |
| `src/config/` | Config model, value checks, parsing, reading and migrating, writing and serializing |
| `src/mcp.rs` | The stdio MCP server |
| `src/env.rs`, `src/git.rs`, `src/state.rs`, `src/clipboard.rs`, `src/error.rs` | Environment and external commands, git repo/branch, state.json, the clipboard (pbpaste, wl-paste, xclip), errors |
| `herdr-plugin.toml` | Plugin manifest |
| `scripts/install.sh` | herdr build step: download and verify the release binary |
| `.cursor/skills/verify-nudge/` | Verification skill and harness (also at `.claude/skills/`) |

## Tests

Tests must never call a real webhook. Use the `FakeWebhook` (a local HTTP server) and `FakeRun` (canned command output) helpers, and a temporary `NUDGE_CONFIG` / `NUDGE_STATE_DIR` passed through `Env`. Cover behavior, such as which events notify, what gets redacted and what setup saves, rather than implementation details.

## Pull requests

- Keep changes focused, and explain the user-visible effect.
- Run the checks above before pushing.
- Add a line to `CHANGELOG.md` for user-visible changes.

## Releases (maintainers)

Bump `version` in both `Cargo.toml` and `herdr-plugin.toml`, update `CHANGELOG.md`, then push a `v<version>` tag. The release workflow checks that the versions match the tag, runs the tests and publishes binaries with checksums.
