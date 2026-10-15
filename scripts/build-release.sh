#!/bin/sh
# Build release archives: dist/release/nudge-<target>.tar.gz plus a .sha256 for each.
# Usage: scripts/build-release.sh [target...]   targets: linux-x64 linux-arm64 darwin-x64 darwin-arm64
# Build darwin targets on macOS so the binaries get signed.
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
[ "$#" -gt 0 ] || set -- linux-x64 linux-arm64 darwin-x64 darwin-arm64
mkdir -p dist/release
for target in "$@"; do
  out=$(mktemp -d)
  bun build --compile --minify --target="bun-$target" src/cli.ts --outfile "$out/nudge"
  tar -czf "dist/release/nudge-$target.tar.gz" -C "$out" nudge
  (cd dist/release && if command -v sha256sum >/dev/null 2>&1; then sha256sum "nudge-$target.tar.gz"; else shasum -a 256 "nudge-$target.tar.gz"; fi) >"dist/release/nudge-$target.tar.gz.sha256"
  rm -rf "$out"
  echo "built dist/release/nudge-$target.tar.gz"
done
