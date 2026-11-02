#!/bin/sh
# Load the Nudge panel mod once with `herdr-app check`, in a sandbox: a temp HOME and HERDR_APP_HOME,
# and stub `nudge` / `hc` first on PATH, so a check never touches the real nudge state, real herdr-core
# terminals or the running app. The stubs log every call; the log is printed at the end.
# Usage: scripts/check-mod.sh   (from the repo root; skips with a note when herdr-app isn't installed)
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
app=$(command -v herdr-app || true)
if [ -z "$app" ]; then
    echo "check-mod: herdr-app is not installed; skipping herdr-app check (bun test herdr-app/test still runs in CI)"
    exit 0
fi

sandbox=$(mktemp -d)
trap 'rm -rf "$sandbox"' EXIT
mkdir -p "$sandbox/bin" "$sandbox/home/.local/state/nudge" "$sandbox/app/mods"
log="$sandbox/calls.log"
: > "$log"
fixture="$root/herdr-app/test/fixtures/status-laptop.json"

cat > "$sandbox/bin/nudge" <<EOF
#!/bin/sh
echo "nudge \$*" >> "$log"
if [ "\$*" = "status --json" ]; then cat "$fixture"; fi
exit 0
EOF
cat > "$sandbox/bin/hc" <<EOF
#!/bin/sh
echo "hc \$*" >> "$log"
exit 0
EOF
chmod +x "$sandbox/bin/nudge" "$sandbox/bin/hc"

# A copy, so the files gpui-shell writes beside a mod (jsconfig.json, .d.ts) stay out of the repo.
cp -R "$root/herdr-app/mods/nudge" "$sandbox/app/mods/nudge"

status=0
env HOME="$sandbox/home" HERDR_APP_HOME="$sandbox/app" PATH="$sandbox/bin:$PATH" \
    XDG_CONFIG_HOME="$sandbox/home/.config" XDG_STATE_HOME="$sandbox/home/.local/state" \
    NUDGE_CONFIG="$sandbox/home/.config/nudge/config.toml" NUDGE_STATE_DIR="$sandbox/home/.local/state/nudge" \
    "$app" check "$sandbox/app/mods/nudge" || status=$?

echo "check-mod: stub calls:"
if [ -s "$log" ]; then sed 's/^/  /' "$log"; else echo "  (none)"; fi
exit "$status"
