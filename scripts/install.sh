#!/bin/sh
# herdr [[build]] step (herdr plugin install / update): put the prebuilt nudge binary for this
# platform in dist/nudge, verified against the release checksum. Without a release binary for this
# platform, build it with cargo instead. `herdr plugin link` skips build steps; linked checkouts run
# target/release/nudge (see bin/nudge) after `cargo build --release`.
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
version=$(sed -n 's/^version = "\(.*\)"$/\1/p' "$root/herdr-plugin.toml" | head -n 1)
base=${NUDGE_RELEASE_BASE:-https://github.com/weskor/nudge/releases/download/v$version}

case "$(uname -s)-$(uname -m)" in
  Linux-x86_64) target=linux-x64 ;;
  Linux-aarch64 | Linux-arm64) target=linux-arm64 ;;
  Darwin-x86_64) target=darwin-x64 ;;
  Darwin-arm64) target=darwin-arm64 ;;
  *) target= ;;
esac

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$root/dist"

download() {
  if command -v curl >/dev/null 2>&1; then curl -fsSL "$1" -o "$2"; else wget -q "$1" -O "$2"; fi
}
sha256() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | cut -d ' ' -f 1; else shasum -a 256 "$1" | cut -d ' ' -f 1; fi
}

if [ -n "$target" ]; then
  archive="nudge-$target.tar.gz"
  echo "nudge: downloading $archive (v$version)"
  if download "$base/$archive" "$tmp/$archive" && download "$base/$archive.sha256" "$tmp/$archive.sha256"; then
    if [ "$(cut -d ' ' -f 1 "$tmp/$archive.sha256")" != "$(sha256 "$tmp/$archive")" ]; then
      echo "nudge: checksum mismatch for $archive; refusing to install it" >&2
      exit 1
    fi
    tar -xzf "$tmp/$archive" -C "$tmp"
    mv "$tmp/nudge" "$root/dist/nudge"
    chmod +x "$root/dist/nudge"
    echo "nudge: installed $root/dist/nudge"
    exit 0
  fi
  echo "nudge: no release binary at $base/$archive"
else
  echo "nudge: no prebuilt binary for $(uname -s)-$(uname -m)"
fi

for cargo in "$(command -v cargo 2>/dev/null || true)" "$HOME/.cargo/bin/cargo"; do
  if [ -n "$cargo" ] && [ -x "$cargo" ]; then
    echo "nudge: building with $cargo"
    cd "$root"
    "$cargo" build --release --locked
    cp target/release/nudge dist/nudge
    echo "nudge: installed $root/dist/nudge"
    exit 0
  fi
done
echo "nudge: no prebuilt binary and cargo isn't installed (https://rustup.rs)" >&2
exit 1
