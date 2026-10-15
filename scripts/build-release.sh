#!/bin/sh
# Build release archives: dist/release/nudge-<target>.tar.gz plus a .sha256 for each.
# Usage: scripts/build-release.sh [target...]   targets: linux-x64 linux-arm64 darwin-x64 darwin-arm64
# Linux binaries are static (musl). Build each target on its own OS and architecture: CI runs linux-x64
# on x86_64 Linux, linux-arm64 on arm64 Linux, and both darwin targets on macOS.
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
[ "$#" -gt 0 ] || set -- linux-x64 linux-arm64 darwin-x64 darwin-arm64
mkdir -p dist/release
for target in "$@"; do
  case "$target" in
    linux-x64) triple=x86_64-unknown-linux-musl ;;
    linux-arm64) triple=aarch64-unknown-linux-musl ;;
    darwin-x64) triple=x86_64-apple-darwin ;;
    darwin-arm64) triple=aarch64-apple-darwin ;;
    *) echo "unknown target: $target" >&2; exit 2 ;;
  esac
  rustup target add "$triple" >/dev/null
  cargo build --release --locked --target "$triple"
  tar -czf "dist/release/nudge-$target.tar.gz" -C "target/$triple/release" nudge
  (cd dist/release && if command -v sha256sum >/dev/null 2>&1; then sha256sum "nudge-$target.tar.gz"; else shasum -a 256 "nudge-$target.tar.gz"; fi) >"dist/release/nudge-$target.tar.gz.sha256"
  echo "built dist/release/nudge-$target.tar.gz"
done
