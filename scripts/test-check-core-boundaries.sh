#!/bin/sh
# Regression checks for scripts/check-core-boundaries.sh: the gate must pass a
# clean core, fail on a forbidden dependency, and fail (not false-green) when
# Cargo itself errors.
set -eu

root=$(unset CDPATH; cd -- "$(dirname -- "$0")/.." && pwd)
tmp=$(mktemp -d "${TMPDIR:-/tmp}/qv-boundary-test.XXXXXX")
trap 'rm -rf "$tmp"' EXIT

failures=0

# run_case <name> <expected: 0=gate passes, 1=gate fails> <workspace root>
run_case() {
    name=$1
    expected=$2
    wroot=$3
    if output=$(sh "$wroot/scripts/check-core-boundaries.sh" 2>&1); then
        rc=0
    else
        rc=1
    fi
    if [ "$rc" -ne "$expected" ]; then
        echo "FAIL $name: gate exit mismatch (output: $output)" >&2
        failures=$((failures + 1))
        return
    fi
    if [ "$expected" -ne 0 ] && printf '%s' "$output" | grep -q 'boundary OK'; then
        echo "FAIL $name: gate printed false-green success" >&2
        failures=$((failures + 1))
        return
    fi
    echo "ok   $name"
}

# Case 1: the current tree has a calendar-only core.
run_case "clean core passes" 0 "$root"

# Case 2: a core with a forbidden workspace dependency must fail.
ws="$tmp/forbidden"
mkdir -p "$ws/scripts" "$ws/crates/core/src" "$ws/crates/providers/src"
cp "$root/scripts/check-core-boundaries.sh" "$ws/scripts/"
cat > "$ws/Cargo.toml" <<'EOF'
[workspace]
resolver = "2"
members = ["crates/core", "crates/providers"]
EOF
cat > "$ws/crates/providers/Cargo.toml" <<'EOF'
[package]
name = "quotavista-providers"
version = "0.0.0"
edition = "2021"
EOF
: > "$ws/crates/providers/src/lib.rs"
cat > "$ws/crates/core/Cargo.toml" <<'EOF'
[package]
name = "quotavista-core"
version = "0.0.0"
edition = "2021"

[dependencies]
quotavista-providers = { path = "../providers" }
EOF
: > "$ws/crates/core/src/lib.rs"
run_case "forbidden dependency fails" 1 "$ws"

# Disabled optional and inactive-target dependencies must not bypass the gate.
cat > "$ws/crates/core/Cargo.toml" <<'EOF'
[package]
name = "quotavista-core"
version = "0.0.0"
edition = "2021"
[dependencies]
quotavista-providers = { path = "../providers", optional = true }
EOF
run_case "optional forbidden dependency fails" 1 "$ws"
cat > "$ws/crates/core/Cargo.toml" <<'EOF'
[package]
name = "quotavista-core"
version = "0.0.0"
edition = "2021"
[target.'cfg(target_os = "none")'.dependencies]
quotavista-providers = { path = "../providers" }
EOF
run_case "inactive target forbidden dependency fails" 1 "$ws"

# Case 3: a workspace Cargo cannot parse must fail, preserving Cargo's error.
ws="$tmp/broken"
mkdir -p "$ws/scripts"
cp "$root/scripts/check-core-boundaries.sh" "$ws/scripts/"
printf '[workspace\n' > "$ws/Cargo.toml"
run_case "cargo failure fails" 1 "$ws"

if [ "$failures" -ne 0 ]; then
    echo "$failures boundary-gate regression check(s) failed" >&2
    exit 1
fi
echo "all boundary-gate regression checks passed"
