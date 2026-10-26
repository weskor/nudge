# AGENTS.md

nudge is a Rust CLI and herdr plugin that POSTs JSON to a webhook when coding agents finish or are blocked. Read [CONTRIBUTING.md](CONTRIBUTING.md) for layout and commands.

## Before you call a change done

```sh
cargo fmt --check && scripts/check-sizes.sh && cargo clippy --all-targets -- -D warnings && cargo test
cargo build --release && NUDGE_BIN=$PWD/target/release/nudge .cursor/skills/verify-nudge/bin/verify-nudge parity /tmp/after.json
```

For anything user-visible, run the parity command on `main` first (`/tmp/before.json`) and `diff -u` the two transcripts. Every difference in the diff is either the change you meant to make or a bug. To drive one feature by hand, use the [verify-nudge skill](.cursor/skills/verify-nudge/SKILL.md).

## Rules

Each rule names what enforces it. Prefer adding enforcement (a lint, a type, a test, a harness check) over adding prose here.

| Rule | Enforced by |
| --- | --- |
| No panics outside tests: no `unwrap`, `expect`, `panic!`, `unreachable!`, unchecked indexing or string slicing. Return `error::Error`. | clippy lints in `Cargo.toml`; tests are exempt via `clippy.toml` |
| No `as` casts; use `From`/`TryFrom`. | `clippy::as_conversions` |
| Match every enum variant by name; no `_ =>` over enums. | `clippy::wildcard_enum_match_arm` |
| A lint exception is `#[expect(lint, reason = "...")]` on the item, never `#[allow]`. | `clippy::allow_attributes`, `allow_attributes_without_reason` |
| No `unsafe`. | `unsafe_code = "forbid"` |
| Formatting is rustfmt's. | `cargo fmt --check` in CI |
| Parse external data once, at the boundary, into typed structs: config TOML (`config/parse/`), herdr JSON (`herdr/`), plugin env JSON (`plugin/`), MCP arguments (`mcp.rs`). Code past the boundary trusts the types. | serde types; review |
| Read the environment through `env::Env`, passed in from `main`. Logic never calls `std::env::var`. | review; tests build `Env` values |
| Never print or log a secret. Validation messages describe the problem, never the value; setup shows the header masked. | tests `setup_field_checks_never_echo_the_rejected_value`, `errors_never_contain_the_authorization_value`; parity's "no webhook token printed" check |
| Never contact a real webhook, the real herdr server or the real clipboard from tests or verification. | `FakeWebhook` / `FakeRun` in tests; the verify-nudge sandbox (fakes first on PATH) |
| Tests assert what a user or the webhook can observe (requests sent, files written, decisions, messages), not internals. | review |
| Behavior changes show up in the parity diff and are intended. | `verify-nudge parity` in CI, plus the before/after diff above |
| herdr overlay panes open over the active pane; never pass `--target-pane` when opening them. | parity scenarios for the `setup` and `status` actions record the herdr argv |
| `Cargo.lock` is committed and CI builds with `--locked`. | CI |
| `version` in `Cargo.toml` and `herdr-plugin.toml` match the release tag. | release workflow |
| Use `parking_lot::Mutex` where a lock result would be unwrapped. | review |
| No one-caller wrapper functions and no dead code; delete what a change makes obsolete. | `dead_code` (denied by `clippy -D warnings`); review |
| A source file has at most 300 lines; tests don't count (they live in `tests.rs`). | `scripts/check-sizes.sh` in CI |
| A function has at most 50 lines. Split it into named steps, not pass-through wrappers. | `clippy::too_many_lines` with `too-many-lines-threshold = 50` in `clippy.toml` |
| One responsibility per file, named after what it owns, opening with a `//!` line that says so. Areas with several files are folders whose `mod.rs` is the facade: submodules are private and `mod.rs` re-exports what other areas use. | review |
| Unit tests live next to their module: `foo.rs` declares `#[cfg(test)] mod tests;` and the tests are in `foo/tests.rs` (`tests.rs` beside a `mod.rs`). | review; `scripts/check-sizes.sh` skips only `tests.rs` files |
