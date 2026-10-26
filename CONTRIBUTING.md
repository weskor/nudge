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
cargo clippy --all-targets -- -D warnings
cargo test
NUDGE_BIN=$PWD/target/release/nudge .cursor/skills/verify-nudge/bin/verify-nudge parity /tmp/parity.json
```

CI runs all four on Linux, and clippy and the tests on macOS too. The lint set in `Cargo.toml` is strict on purpose (no `unwrap`, no panics, no unchecked indexing, no `as` casts, exhaustive matches). An exception is a `#[expect(lint, reason = "...")]` on the item it excuses. [AGENTS.md](AGENTS.md) lists the project rules and what enforces each.

The last command drives the real binary through every user journey in the harness's scenario list, each in a sandbox with a fake webhook, fake herdr and fake clipboard. See [`.cursor/skills/verify-nudge`](.cursor/skills/verify-nudge/SKILL.md). To check that a change keeps behavior, run it before and after and `diff -u` the two transcripts.

## Layout

| Path | What |
| --- | --- |
| `src/main.rs` | Command-line entry point |
| `src/plugin.rs` | herdr hook and actions (`nudge plugin …`) |
| `src/setup/` | Setup: `wizard.rs` (the wizard's state and keys), `view.rs` (drawing it and the terminal loop), `paste.rs` (finding the URL and header in pasted text), `mod.rs` (flags, the scripted path, saving) |
| `src/settings.rs`, `src/ui.rs` | The settings editor, and the look it shares with the setup wizard |
| `src/doctor.rs`, `src/tty.rs` | `nudge doctor` checks, and terminal output outside the full-screen views |
| `src/mcp.rs` | The stdio MCP server |
| `src/config.rs`, `src/notify.rs`, `src/context.rs`, `src/state.rs`, `src/error.rs` | Config, notifications, herdr/git detection, state, errors |
| `src/watch.rs` | Auto-notify rules and redaction |
| `src/install.rs`, `src/clipboard.rs` | herdr plugin and `~/.local/bin/nudge` link detection, and reading the clipboard (pbpaste, wl-paste, xclip) |
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
