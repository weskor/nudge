# Contributing

Thanks for helping. Bug reports, ideas and pull requests are all welcome.

## Development setup

You need [Bun](https://bun.sh) and [herdr](https://herdr.dev) 0.9 or later.

```sh
git clone https://github.com/weskor/nudge && cd nudge
bun install
herdr plugin link "$PWD"      # herdr runs your checkout from source with Bun
bun test && bun run typecheck
```

`herdr plugin link` skips the build step, so a linked checkout runs `src/cli.ts` directly through `bin/nudge`. Run `bun run build` to try the compiled binary instead (it goes to `dist/nudge`, which `bin/nudge` prefers); delete `dist/` to go back to source.

## Layout

| Path | What |
| --- | --- |
| `src/cli.ts` | Command-line entry point |
| `src/plugin.ts` | herdr hook and actions (`nudge plugin …`) |
| `src/setup.ts`, `src/doctor.ts` | Interactive setup and checks |
| `src/core/` | Config, notifications, herdr/git detection, auto-notify rules (`watch.ts`), state |
| `herdr-plugin.toml` | Plugin manifest |
| `scripts/install.sh` | herdr build step: download and verify the release binary |

## Tests

Tests must never call a real webhook. Use the fake `fetch` and command runner patterns in `test/`, a temporary `NUDGE_CONFIG` / `NUDGE_STATE_DIR`, or a local `Bun.serve`. Cover behavior, such as which events notify, what gets redacted and what setup saves, rather than implementation details.

## Pull requests

- Keep changes focused, and explain the user-visible effect.
- Run `bun test` and `bun run typecheck` before pushing; CI runs both.
- Add a line to `CHANGELOG.md` for user-visible changes.

## Releases (maintainers)

Bump `version` in both `package.json` and `herdr-plugin.toml`, update `CHANGELOG.md`, then push a `v<version>` tag. The release workflow checks that the versions match the tag, runs the tests and publishes binaries with checksums.
