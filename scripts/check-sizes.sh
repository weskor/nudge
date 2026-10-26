#!/bin/sh
# Fail when a source file has more than MAX lines. Test files (`tests.rs`, next to the module they test) are
# exempt; function length is clippy's job (`too-many-lines-threshold` in clippy.toml).
# Usage: scripts/check-sizes.sh   (from the repo root; CI runs it)
set -eu

MAX=300
status=0
for file in $(find src -name '*.rs' ! -name tests.rs | sort); do
    lines=$(wc -l < "$file")
    if [ "$lines" -gt "$MAX" ]; then
        echo "$file: $lines lines (max $MAX); split it by responsibility" >&2
        status=1
    fi
done
exit "$status"
