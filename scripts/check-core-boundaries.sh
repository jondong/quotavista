#!/bin/sh
# Enforces the workspace dependency direction: quotavista-core must not depend
# on Tauri, SQLx, native credential stores, or any other workspace crate.
# See docs/architecture.md "Modules and dependencies".
set -eu

# Resolve the workspace from this script's location so the gate works from any
# caller directory.
root=$(unset CDPATH; cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root"

# Keep Cargo's output in a variable instead of a pipeline so a Cargo failure
# aborts the script (set -e) instead of being masked by the last pipe stage.
# normal,build,dev covers every dependency kind core could declare.
tree=$(cargo tree -p quotavista-core --edges normal,build,dev --depth 1 --target all --all-features --prefix none)

# Only pure calendar libraries are allowed as direct dependencies (all kinds).
# Transitive calendar implementation dependencies are locked in Cargo.lock.
deps=$(printf '%s\n' "$tree" | tail -n +2 | awk '$1 != "chrono" && $1 != "chrono-tz"')
if [ -n "$deps" ]; then
    echo "error: quotavista-core permits only chrono and chrono-tz, found:" >&2
    echo "$deps" >&2
    exit 1
fi
echo "quotavista-core calendar-only dependencies: boundary OK"
