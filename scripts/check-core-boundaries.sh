#!/bin/sh
# Enforces the workspace dependency direction: quotavista-core must not depend
# on Tauri, SQLx, native credential stores, or any other workspace crate.
# See docs/architecture.md "Modules and dependencies".
set -eu

deps="$(cargo tree -p quotavista-core --edges normal --depth 1 --prefix none | tail -n +2)"
if [ -n "$deps" ]; then
    echo "error: quotavista-core must stay dependency-free, found:" >&2
    echo "$deps" >&2
    exit 1
fi
echo "quotavista-core has no dependencies: boundary OK"
