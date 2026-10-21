#!/usr/bin/env bash
# Regression checks for the P1-13 native launch boundary:
# scripts/native-launch-probe.sh and scripts/native-launch-lib.sh.
#
# Two failures motivated this suite, and both were reproduced before it existed:
#
#   1. The probe accepted any readable, non-empty store as a seeded one, so a
#      disposable test double answering `0|0|0|0|0|0|zh-CN/UTC/USD` still
#      reached `PROBE: PASS` with exit 0. Successful SQL execution is not
#      successful fixture initialisation. The validator cases below pin the
#      deterministic fixture: identities, counts, relationships, revision,
#      settings, and evidence that the post-seed refresh actually committed.
#   2. A fresh `TMPDIR` isolates the prototype's state directory but not its
#      single-instance channel, which the pinned plugin names from the
#      compile-time identifier. The isolation cases below pin the fail-closed
#      gate against a synthetic incumbent - a real listener on a real socket -
#      and pin the rule that only the validation identity's socket is ever
#      removed.
#
# Everything here is synthetic. No QuotaVista process is launched, no real
# single-instance socket is created, the ordinary application identifier is
# temporarily replaced with a `selftest` name for the incumbent cases, and no
# credential store of any kind is touched.
#
# Usage: scripts/test-native-launch-probe.sh

# Not `set -e`: this suite deliberately runs commands that must fail.
set -uo pipefail

root=$(unset CDPATH; cd -- "$(dirname -- "$0")/.." && pwd)
tmp=$(mktemp -d "${TMPDIR:-/tmp}/qv-probe-selftest.XXXXXX")
cleanup_pids=()
cleanup_paths=()

# Deliberately not called `cleanup`: scripts/native-launch-probe.sh defines a
# function of that name for its own EXIT handler, and sourcing it below would
# silently replace this one — the trap would then run the probe's teardown, which
# reads variables only the probe sets. The collision is real and was hit once.
selftest_cleanup() {
    local pid path
    for pid in "${cleanup_pids[@]:-}"; do
        [ -n "$pid" ] || continue
        kill "$pid" 2>/dev/null
        wait "$pid" 2>/dev/null
    done
    for path in "${cleanup_paths[@]:-}"; do
        [ -n "$path" ] || continue
        rm -rf "$path"
    done
    rm -rf "$tmp"
}
trap selftest_cleanup EXIT

failures=0
skips=0

tok() { echo "ok   $1"; }

tfail() {
    echo "FAIL $1: $2" >&2
    [ -n "${3:-}" ] && printf '%s\n' "$3" | sed 's/^/     | /' >&2
    failures=$((failures + 1))
}

tskip() {
    echo "SKIP $1: $2" >&2
    skips=$((skips + 1))
}

# ------------------------------------------------------- code under test

# Sourcing (not executing) the probe defines its functions and runs nothing:
# the library-mode guard at the bottom of the script is itself under test here,
# because a script that launched on source would start a process right now.
# The probe sources this library from inside main(), so sourcing the probe alone
# would leave every qv_* helper undefined. Load it explicitly first.
# shellcheck source=scripts/native-launch-lib.sh
. "$root/scripts/native-launch-lib.sh"
# shellcheck source=scripts/native-launch-probe.sh
. "$root/scripts/native-launch-probe.sh"

# ---------------------------------------------------------------- fixtures

MIGRATIONS="$root/crates/infrastructure/migrations"

# new_store <db-path>
# Applies the prototype's own migrations, so every fixture below is built on the
# real schema rather than a hand-copied approximation of it.
new_store() {
    cat "$MIGRATIONS/0001_persistence.sql" "$MIGRATIONS/0002_settings_thresholds.sql" |
        sqlite3 "$1"
}

# sql <db-path> <statements>
sql() {
    sqlite3 "$1" "$2"
}

FIXTURE_CONNECTIONS="INSERT INTO connections (id, provider, alias, credential_ref, generation, lifecycle) VALUES
 ('synthetic-kimi-demo', 'synthetic-kimi', 'Kimi (synthetic)', NULL, 0, 'active'),
 ('synthetic-openrouter-demo', 'synthetic-openrouter', 'OpenRouter (synthetic)', NULL, 0, 'active');"

FIXTURE_SCOPES="INSERT INTO provider_scopes (id, connection_id, scope_key, scope_type) VALUES
 ('synthetic-kimi-demo-account', 'synthetic-kimi-demo', 'synthetic-account', 'account'),
 ('synthetic-openrouter-demo-account', 'synthetic-openrouter-demo', 'synthetic-account', 'account');"

FIXTURE_PARTITIONS="INSERT INTO partitions (id, scope_id, product_key, dataset, metric, start_at, end_at, grain, dimensions, authority) VALUES
 (1, 'synthetic-kimi-demo-account', 'tokens', 'usage', 'tokens', '2026-10-18T00:00:00Z', '2026-10-19T00:00:00Z', 'day', '{}', 'synthetic'),
 (2, 'synthetic-openrouter-demo-account', 'tokens', 'usage', 'tokens', '2026-10-18T00:00:00Z', '2026-10-19T00:00:00Z', 'day', '{}', 'synthetic');"

FIXTURE_RUNS="INSERT INTO sync_runs (id, partition_id, generation, base_revision, state, cursor, published_revision) VALUES
 ('run-1', 1, 0, 0, 'published', NULL, 7),
 ('run-2', 2, 0, 0, 'published', NULL, 8);"

FIXTURE_PUBLISHED="INSERT INTO published_partitions (partition_id, run_id, data_revision, successful_sync_at, watermark) VALUES
 (1, 'run-1', 7, '2026-10-21T00:00:00Z', '2026-10-21T00:00:00Z'),
 (2, 'run-2', 8, '2026-10-21T00:00:00Z', '2026-10-21T00:00:00Z');"

FIXTURE_REVISION="UPDATE store_state SET data_revision = 8 WHERE id = 1;"

# fix_facts <db-path>
# Three UTC day facts per connection, matching HISTORY_DAYS = 3 in
# crates/providers/src/simulated.rs. Bucket rows must satisfy the table's CHECK:
# event_id empty, and key_start/key_end equal to start_at/end_at.
fix_facts() {
    local db="$1"
    local partition scope day start end
    for partition in 1 2; do
        if [ "$partition" -eq 1 ]; then
            scope="synthetic-kimi-demo-account"
        else
            scope="synthetic-openrouter-demo-account"
        fi
        for day in 18 19 20; do
            start="2026-10-${day}T00:00:00Z"
            end="2026-10-$((day + 1))T00:00:00Z"
            sql "$db" "INSERT INTO usage_facts
                (partition_id, scope_id, dataset, metric, kind, event_id, key_start, key_end,
                 start_at, end_at, value_kind, amount, unit_detail, source_revision)
                VALUES (${partition}, '${scope}', 'usage', 'tokens', 'bucket', '',
                        '${start}', '${end}', '${start}', '${end}', 'tokens', '100', 'total', 'synthetic-revision');"
        done
    done
}

# build_fixture <name> <variant>
# Creates <tmp>/<name>/quotavista.sqlite holding the requested variant.
build_fixture() {
    local name="$1" variant="$2"
    local dir="$tmp/$name"
    mkdir -p "$dir"
    local db="$dir/quotavista.sqlite"
    new_store "$db"

    case "$variant" in
        schema-only)
            # Migrations applied and nothing else: the shape a store has when the
            # composition opened it but seeding never ran.
            ;;
        good)
            sql "$db" "$FIXTURE_CONNECTIONS"
            sql "$db" "$FIXTURE_SCOPES"
            sql "$db" "$FIXTURE_PARTITIONS"
            sql "$db" "$FIXTURE_RUNS"
            sql "$db" "$FIXTURE_PUBLISHED"
            fix_facts "$db"
            sql "$db" "$FIXTURE_REVISION"
            ;;
        no-facts)
            sql "$db" "$FIXTURE_CONNECTIONS"
            sql "$db" "$FIXTURE_SCOPES"
            sql "$db" "$FIXTURE_PARTITIONS"
            sql "$db" "$FIXTURE_RUNS"
            sql "$db" "$FIXTURE_PUBLISHED"
            sql "$db" "$FIXTURE_REVISION"
            ;;
        no-published)
            sql "$db" "$FIXTURE_CONNECTIONS"
            sql "$db" "$FIXTURE_SCOPES"
            sql "$db" "$FIXTURE_PARTITIONS"
            sql "$db" "$FIXTURE_RUNS"
            fix_facts "$db"
            sql "$db" "$FIXTURE_REVISION"
            ;;
        wrong-ids)
            sql "$db" "INSERT INTO connections (id, provider, alias, credential_ref, generation, lifecycle) VALUES
                ('some-other-connection', 'synthetic-kimi', 'Kimi (synthetic)', NULL, 0, 'active'),
                ('synthetic-openrouter-demo', 'synthetic-openrouter', 'OpenRouter (synthetic)', NULL, 0, 'active');"
            sql "$db" "$FIXTURE_SCOPES"
            sql "$db" "$FIXTURE_PARTITIONS"
            sql "$db" "$FIXTURE_RUNS"
            sql "$db" "$FIXTURE_PUBLISHED"
            fix_facts "$db"
            sql "$db" "$FIXTURE_REVISION"
            ;;
        orphan-scope)
            sql "$db" "$FIXTURE_CONNECTIONS"
            sql "$db" "$FIXTURE_SCOPES"
            # Foreign keys are off in the sqlite3 CLI by default, which is what
            # makes this row constructible at all; the validator must still catch
            # it rather than trusting the engine to have refused it.
            sql "$db" "INSERT INTO provider_scopes (id, connection_id, scope_key, scope_type)
                VALUES ('orphan-account', 'connection-that-does-not-exist', 'synthetic-account', 'account');"
            sql "$db" "$FIXTURE_PARTITIONS"
            sql "$db" "$FIXTURE_RUNS"
            sql "$db" "$FIXTURE_PUBLISHED"
            fix_facts "$db"
            sql "$db" "$FIXTURE_REVISION"
            ;;
        credential-ref)
            sql "$db" "INSERT INTO connections (id, provider, alias, credential_ref, generation, lifecycle) VALUES
                ('synthetic-kimi-demo', 'synthetic-kimi', 'Kimi (synthetic)', 'qv1:macos-keychain:some-entry', 0, 'active'),
                ('synthetic-openrouter-demo', 'synthetic-openrouter', 'OpenRouter (synthetic)', NULL, 0, 'active');"
            sql "$db" "$FIXTURE_SCOPES"
            sql "$db" "$FIXTURE_PARTITIONS"
            sql "$db" "$FIXTURE_RUNS"
            sql "$db" "$FIXTURE_PUBLISHED"
            fix_facts "$db"
            sql "$db" "$FIXTURE_REVISION"
            ;;
        bad-scope-key)
            sql "$db" "$FIXTURE_CONNECTIONS"
            sql "$db" "INSERT INTO provider_scopes (id, connection_id, scope_key, scope_type) VALUES
                ('synthetic-kimi-demo-account', 'synthetic-kimi-demo', 'some-other-key', 'account'),
                ('synthetic-openrouter-demo-account', 'synthetic-openrouter-demo', 'synthetic-account', 'account');"
            sql "$db" "$FIXTURE_PARTITIONS"
            sql "$db" "$FIXTURE_RUNS"
            sql "$db" "$FIXTURE_PUBLISHED"
            fix_facts "$db"
            sql "$db" "$FIXTURE_REVISION"
            ;;
        needs-credentials)
            sql "$db" "INSERT INTO connections (id, provider, alias, credential_ref, generation, lifecycle) VALUES
                ('synthetic-kimi-demo', 'synthetic-kimi', 'Kimi (synthetic)', NULL, 0, 'needs_credentials'),
                ('synthetic-openrouter-demo', 'synthetic-openrouter', 'OpenRouter (synthetic)', NULL, 0, 'active');"
            sql "$db" "$FIXTURE_SCOPES"
            sql "$db" "$FIXTURE_PARTITIONS"
            sql "$db" "$FIXTURE_RUNS"
            sql "$db" "$FIXTURE_PUBLISHED"
            fix_facts "$db"
            sql "$db" "$FIXTURE_REVISION"
            ;;
        restarted)
            sql "$db" "$FIXTURE_CONNECTIONS"
            sql "$db" "$FIXTURE_SCOPES"
            sql "$db" "$FIXTURE_PARTITIONS"
            sql "$db" "$FIXTURE_RUNS"
            sql "$db" "$FIXTURE_PUBLISHED"
            fix_facts "$db"
            sql "$db" "$FIXTURE_REVISION"
            # What a real second launch leaves behind: reconcile_connection_credentials
            # (crates/infrastructure/src/secrets.rs:47) marks an Active connection with
            # no retrievable credential as needing one, so both seeded demo connections
            # report needs_credentials after a restart rather than staying Active.
            sql "$db" "UPDATE connections SET lifecycle = 'needs_credentials';"
            ;;
        revision-9)
            sql "$db" "$FIXTURE_CONNECTIONS"
            sql "$db" "$FIXTURE_SCOPES"
            sql "$db" "$FIXTURE_PARTITIONS"
            sql "$db" "$FIXTURE_RUNS"
            sql "$db" "$FIXTURE_PUBLISHED"
            fix_facts "$db"
            sql "$db" "UPDATE store_state SET data_revision = 9 WHERE id = 1;"
            ;;
        zh-cn-settings)
            sql "$db" "$FIXTURE_CONNECTIONS"
            sql "$db" "$FIXTURE_SCOPES"
            sql "$db" "$FIXTURE_PARTITIONS"
            sql "$db" "$FIXTURE_RUNS"
            sql "$db" "$FIXTURE_PUBLISHED"
            fix_facts "$db"
            sql "$db" "$FIXTURE_REVISION"
            sql "$db" "UPDATE settings SET locale = 'zh-CN' WHERE id = 1;"
            ;;
        *)
            echo "unknown fixture variant: $variant" >&2
            return 2
            ;;
    esac
    printf '%s\n' "$dir"
}

# run_validator <case-name> <expected: 0=validates, 1=rejected> <data-dir> [lifecycles]
# The fourth argument overrides the expected connection lifecycle, which differs
# between a first launch (active) and every later one (needs_credentials).
run_validator() {
    local name="$1" expected="$2" dir="$3"
    local lifecycles="${4:-$EXPECTED_LIFECYCLES_SEEDED}"
    local out rc
    if out="$(store_snapshot "$name" "$dir" 2>&1 && validate_snapshot "$name" "$lifecycles" 2>&1)"; then
        rc=0
    else
        rc=1
    fi
    if [ "$rc" -ne "$expected" ]; then
        tfail "$name" "validator exit mismatch (expected $expected, got $rc)" "$out"
        return
    fi
    if [ "$expected" -ne 0 ] && printf '%s' "$out" | grep -q 'as expected'; then
        # A rejected store must say what was wrong, not quietly disagree.
        if ! printf '%s' "$out" | grep -q "found '"; then
            tfail "$name" "rejected the store without naming a mismatch" "$out"
            return
        fi
    fi
    tok "$name"
}

echo "== store fixture validation =="

good_dir="$(build_fixture good good)"
run_validator "known-good fixture validates" 0 "$good_dir"

# The exact shape the reviewer reproduced as a false pass.
empty_dir="$(build_fixture schema-only schema-only)"
run_validator "schema-only store is rejected" 1 "$empty_dir"
# store_snapshot fills its globals in the calling shell, so a command
# substitution around it would lose them; read the aggregate back out explicitly.
schema_row="$(store_snapshot schema-only "$empty_dir" >/dev/null 2>&1; printf '%s' "$snap_row")"
case "$schema_row" in
    0\|0\|0\|0\|0\|0\|*)
        tok "schema-only store really reads back as an empty aggregate ($schema_row)"
        ;;
    *)
        tfail "schema-only aggregate" "expected 0|0|0|0|0|0|..., got '$schema_row'"
        ;;
esac

run_validator "seed without committed facts is rejected" 1 "$(build_fixture no-facts no-facts)"
run_validator "seed without published partitions is rejected" 1 "$(build_fixture no-published no-published)"
run_validator "wrong connection identity is rejected" 1 "$(build_fixture wrong-ids wrong-ids)"
run_validator "scope with no matching connection is rejected" 1 "$(build_fixture orphan-scope orphan-scope)"
run_validator "a persisted credential reference is rejected" 1 "$(build_fixture credential-ref credential-ref)"
run_validator "off-fixture scope key is rejected" 1 "$(build_fixture bad-scope-key bad-scope-key)"
run_validator "non-active connection lifecycle is rejected" 1 "$(build_fixture needs-credentials needs-credentials)"

# The lifecycle expectation is per-launch rather than a constant, and that is not
# a convenience: a real second launch leaves both demo connections at
# needs_credentials, so the validator must accept that shape only when the caller
# says a restart is what it is looking at, and reject it otherwise.
restarted_dir="$(build_fixture restarted restarted)"
run_validator "a restarted store is rejected by the first-launch expectation" 1 "$restarted_dir"
run_validator "a restarted store is accepted by the restart expectation" 0 \
    "$restarted_dir" "$EXPECTED_LIFECYCLES_RESTARTED"
run_validator "unexpected data revision is rejected" 1 "$(build_fixture revision-9 revision-9)"
run_validator "unexpected settings are rejected" 1 "$(build_fixture zh-cn zh-cn-settings)"

missing_dir="$tmp/no-store-here"
mkdir -p "$missing_dir"
run_validator "missing database is rejected" 1 "$missing_dir"

# Warm mismatch: step 7 of the probe compares the warm aggregate against the cold
# one. Two stores that both validate must still be compared, and a warm store
# that drifted must not pass just because cold and warm agree with each other.
echo
echo "== warm restart comparison =="
warm_same="$(build_fixture warm-same good)"
cold_row="$(store_snapshot cold-compare "$good_dir" >/dev/null 2>&1; printf '%s' "$snap_row")"
store_snapshot warm-compare "$warm_same" >/dev/null 2>&1
warm_row="$snap_row"
if [ -n "$cold_row" ] && [ "$cold_row" = "$warm_row" ]; then
    tok "an unchanged restart keeps the same aggregate ($cold_row)"
else
    tfail "warm restart aggregate" "cold '$cold_row' differs from warm '$warm_row'"
fi

store_snapshot warm-drift "$(build_fixture warm-drift revision-9)" >/dev/null 2>&1
if [ "$snap_row" != "$cold_row" ]; then
    tok "a drifted restart changes the aggregate, so the comparison can see it"
else
    tfail "warm drift detection" "a revision-9 store read back identical to the cold store"
fi

echo
echo "== launch diagnostics =="

err_clean="$tmp/clean.err"
printf '%s\n' \
    'quotavista: capabilities tray=Available notification=Unconfirmed' \
    'quotavista: wake WindowFocused mode Off permitted false merged 0 not-due 0' >"$err_clean"
if out="$(require_clean_diagnostics clean "$err_clean" 'system tray unavailable' 2>&1)"; then
    tok "clean stderr passes"
else
    tfail "clean stderr passes" "expected exit 0" "$out"
fi

err_seed="$tmp/seed-failed.err"
printf '%s\n' \
    'quotavista: capabilities tray=Available notification=Unconfirmed' \
    'quotavista: demo seeding failed: storage unavailable' >"$err_seed"
if out="$(require_clean_diagnostics seed-failed "$err_seed" 'system tray unavailable' 2>&1)"; then
    tfail "demo seeding failed is fatal" "expected exit 1, the probe must not pass a failed seed" "$out"
else
    tok "demo seeding failed is fatal"
fi

err_secret="$tmp/secret-failed.err"
printf '%s\n' \
    'quotavista: capabilities tray=Available notification=Unconfirmed' \
    'quotavista: credential store unavailable: no session bus' >"$err_secret"
if out="$(require_clean_diagnostics secret-failed "$err_secret" 2>&1)"; then
    tfail "credential store unavailable is fatal" "expected exit 1" "$out"
else
    tok "credential store unavailable is fatal"
fi

err_tray="$tmp/tray-failed.err"
printf '%s\n' \
    'quotavista: system tray unavailable: no indicator host' \
    'quotavista: capabilities tray=Unavailable notification=Unconfirmed' >"$err_tray"
if out="$(require_clean_diagnostics tray-asked "$err_tray" 'system tray unavailable' 2>&1)"; then
    tfail "tray failure is fatal when the caller asks for it" "expected exit 1" "$out"
else
    tok "tray failure is fatal when the caller asks for it"
fi
# The no-tray launch must not be failed by its own expected tray message.
if out="$(require_clean_diagnostics tray-not-asked "$err_tray" 2>&1)"; then
    tok "tray failure is not fatal when the caller did not ask for it"
else
    tfail "tray failure is not fatal when the caller did not ask for it" "expected exit 0" "$out"
fi

echo
echo "== single-instance isolation =="

# The plugin's own naming rules, which the whole isolation argument rests on.
got="$(qv_socket_path 'com.quotavista.prototype')"
if [ "$got" = "/tmp/com_quotavista_prototype_si.sock" ]; then
    tok "socket path mirrors the plugin's identifier mapping"
else
    tfail "socket path mapping" "got '$got'"
fi
got="$(qv_socket_path 'com.example-app.name')"
if [ "$got" = "/tmp/com_example_app_name_si.sock" ]; then
    tok "socket path mapping replaces dashes as well as dots"
else
    tfail "socket path mapping with a dash" "got '$got'"
fi
got="$(qv_dbus_name 'com.quotavista.prototype')"
if [ "$got" = "com.quotavista.prototype.SingleInstance" ]; then
    tok "Linux session D-Bus name mirrors the plugin"
else
    tfail "dbus name mapping" "got '$got'"
fi

if [ "$(qv_ordinary_socket)" != "$(qv_probe_socket)" ]; then
    tok "the validation identity has a disjoint channel from the ordinary one"
else
    tfail "channel disjointness" "ordinary and validation sockets are the same"
fi

# --- synthetic incumbent: a real listener on a real socket -----------------
# The ordinary identifier is temporarily replaced with a self-test name so this
# case can never touch the real application's socket, and never launch anything.
saved_ordinary="$QV_ORDINARY_IDENTIFIER"
saved_probe="$QV_PROBE_IDENTIFIER"
QV_ORDINARY_IDENTIFIER="com.quotavista.selftest.ordinary"
QV_PROBE_IDENTIFIER="com.quotavista.selftest.probe"
fake_ordinary="$(qv_ordinary_socket)"
fake_probe="$(qv_probe_socket)"
cleanup_paths+=("$fake_ordinary" "$fake_probe")

if ! command -v lsof >/dev/null 2>&1; then
    tskip "synthetic incumbent is detected" "lsof is unavailable, and so is socket-holder detection"
    tskip "a stale socket does not block the gate" "lsof is unavailable"
elif ! command -v python3 >/dev/null 2>&1; then
    tskip "synthetic incumbent is detected" "python3 is unavailable to bind the synthetic socket"
    tskip "a stale socket does not block the gate" "python3 is unavailable"
else
    rm -f "$fake_ordinary"
    python3 -c '
import socket, sys, time
s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.bind(sys.argv[1])
s.listen(1)
time.sleep(60)
' "$fake_ordinary" >/dev/null 2>&1 &
    listener=$!
    cleanup_pids+=("$listener")
    waited=0
    while [ ! -S "$fake_ordinary" ] && [ "$waited" -lt 50 ]; do
        sleep 0.1
        waited=$((waited + 1))
    done
    if [ ! -S "$fake_ordinary" ]; then
        tfail "synthetic incumbent is detected" "the listener never bound $fake_ordinary"
    else
        holders="$(qv_socket_holders "$fake_ordinary" | tr '\n' ' ')"
        if printf '%s' "$holders" | grep -qw "$listener"; then
            tok "a live socket holder is found without connecting to it"
        else
            tfail "a live socket holder is found without connecting to it" \
                "expected pid $listener among holders '$holders'"
        fi
        # The gate must fail closed, and must name what it found.
        if out="$(qv_require_clear_host 2>&1)"; then
            tfail "the fail-closed gate blocks a synthetic incumbent" \
                "qv_require_clear_host returned 0 while pid $listener held $fake_ordinary"
        elif printf '%s' "$out" | grep -q "incumbent:"; then
            tok "the fail-closed gate blocks a synthetic incumbent and names it"
        else
            tfail "the fail-closed gate blocks a synthetic incumbent and names it" \
                "the gate failed but did not print an incumbent line" "$out"
        fi
        if out="$(qv_detect_incumbents 2>&1)" && ! printf '%s' "$out" | grep -q "$fake_ordinary"; then
            tfail "incumbent detection names the socket" "output did not mention $fake_ordinary" "$out"
        else
            tok "incumbent detection names the socket that is held"
        fi
    fi
    kill "$listener" 2>/dev/null
    wait "$listener" 2>/dev/null
    rm -f "$fake_ordinary"

    # A stale socket - the file exists, nothing holds it - must NOT block. This
    # is the state a previous SIGTERM leaves behind, and the plugin recovers
    # from it by removing and rebinding.
    python3 -c '
import socket, sys
s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.bind(sys.argv[1])
' "$fake_probe" >/dev/null 2>&1
    if [ -S "$fake_probe" ]; then
        holders="$(qv_socket_holders "$fake_probe" | tr '\n' ' ')"
        if [ -z "${holders// /}" ] && qv_require_clear_host >/dev/null 2>&1; then
            tok "a stale socket with no holder does not block the gate"
        else
            tfail "a stale socket with no holder does not block the gate" \
                "holders='$holders'"
        fi
        removed="$(qv_remove_stale_probe_socket)"
        if [ "$removed" = "$fake_probe" ] && [ ! -e "$fake_probe" ]; then
            tok "the stale validation socket is removed by name"
        else
            tfail "the stale validation socket is removed by name" "returned '$removed'"
        fi
    else
        tskip "a stale socket with no holder does not block the gate" \
            "python3 could not bind $fake_probe"
    fi
fi

# The ordinary socket must survive cleanup even when it is stale: it is not this
# script's to remove.
touch "$fake_ordinary"
qv_remove_stale_probe_socket >/dev/null 2>&1
if [ -e "$fake_ordinary" ]; then
    tok "cleanup never removes the ordinary application's socket"
    rm -f "$fake_ordinary"
else
    tfail "cleanup never removes the ordinary application's socket" \
        "$fake_ordinary was deleted by qv_remove_stale_probe_socket"
fi
QV_ORDINARY_IDENTIFIER="$saved_ordinary"
QV_PROBE_IDENTIFIER="$saved_probe"

# --- process-table matching ------------------------------------------------
# `qv_instances_of` matches argv[0] exactly. A canned process table pins that
# behaviour deterministically, including the negative: a plain
# `pgrep -f quotavista` also matches unrelated processes that merely carry the
# repository path in their arguments or environment, which is exactly the false
# positive this function exists to avoid. Shadowing `ps` with a function is safe
# here because it is unset again before anything else runs.
fake_ps_table="$(cat <<EOF
  101 ${PWD}/${QV_ORDINARY_BIN}
  102 ./${QV_PROBE_BIN}
  103 /Applications/some-agent --project ${PWD} --note quotavista-desktop
  104 /bin/bash ./scripts/test-native-launch-probe.sh
  105 /bin/sh ./scripts/native-launch-probe.sh --keep
EOF
)"
ps() { printf '%s\n' "$fake_ps_table"; }

found="$(qv_instances_of "$QV_ORDINARY_BIN" | tr '\n' ' ')"
if [ "${found// /}" = "101" ]; then
    tok "an absolute invocation of the ordinary binary is found"
else
    tfail "an absolute invocation of the ordinary binary is found" "got pids '$found'"
fi
found="$(qv_instances_of "$QV_PROBE_BIN" | tr '\n' ' ')"
if [ "${found// /}" = "102" ]; then
    tok "a relative invocation of the validation binary is found"
else
    tfail "a relative invocation of the validation binary is found" "got pids '$found'"
fi
found="$(qv_detect_incumbents | tr '\n' ' ')"
if printf '%s' "$found" | grep -q '101' && printf '%s' "$found" | grep -q '102' &&
    ! printf '%s' "$found" | grep -q '103'; then
    tok "incumbent detection names both binaries and ignores an unrelated process carrying the path"
else
    tfail "incumbent detection names both binaries and ignores an unrelated process carrying the path" \
        "got: $found"
fi
found="$(qv_instances_of "$tmp/definitely-not-running" | tr '\n' ' ')"
if [ -z "${found// /}" ]; then
    tok "a binary that is not running matches nothing"
else
    tfail "a binary that is not running matches nothing" "unexpected pids '$found'"
fi
unset -f ps


# --- build-time identity check --------------------------------------------
fake_bin="$tmp/qv-selftest-binary"
printf 'not an elf but grep -a reads it: %s\n' "$QV_PROBE_IDENTIFIER" >"$fake_bin"
chmod +x "$fake_bin"
if qv_probe_binary_is_isolated "$fake_bin"; then
    tok "a binary carrying the validation identifier is accepted"
else
    tfail "a binary carrying the validation identifier is accepted" "returned non-zero"
fi
printf 'ordinary identifier only: %s\n' "$QV_ORDINARY_IDENTIFIER" >"$fake_bin"
if qv_probe_binary_is_isolated "$fake_bin"; then
    tfail "a binary without the validation identifier is refused" \
        "returned 0 for a binary carrying only $QV_ORDINARY_IDENTIFIER"
else
    tok "a binary without the validation identifier is refused"
fi
if qv_probe_binary_is_isolated "$tmp/does-not-exist"; then
    tfail "a missing binary is refused" "returned 0"
else
    tok "a missing binary is refused"
fi

echo
echo "== manual session boundary =="

# Two shapes matter. The no-argument form is what `native-checks.sh --interactive`
# prints for a human to copy into their own shell, so its text has to be exactly
# right. The explicit-directory form is what the probe uses internally.
default_block="$(qv_session_setup_block)"
for required in \
    'export QUOTAVISTA_NATIVE_TMPDIR="$(mktemp -d "${TMPDIR:-/tmp}/qv-native-session.XXXXXX")"' \
    'export TMPDIR="$QUOTAVISTA_NATIVE_TMPDIR"' \
    'export QUOTAVISTA_SECRET_STORE=session-only'
do
    if printf '%s\n' "$default_block" | grep -qF "$required"; then
        tok "the copied setup block exports: $required"
    else
        tfail "the copied setup block exports: $required" "missing from:" "$default_block"
    fi
done

# Asserting the text is not enough: this block is pasted into a human's shell, so
# it has to actually run. A mis-nested brace in the mktemp template once produced
# a line that looked plausible and created nothing usable.
session_dir="$(bash -c "$default_block"'
if [ -n "${QUOTAVISTA_NATIVE_TMPDIR:-}" ] && [ -d "$QUOTAVISTA_NATIVE_TMPDIR" ] &&
        [ "$TMPDIR" = "$QUOTAVISTA_NATIVE_TMPDIR" ] &&
        [ "${QUOTAVISTA_SECRET_STORE:-}" = "session-only" ]; then
    printf "%s" "$QUOTAVISTA_NATIVE_TMPDIR"
fi')"
if [ -n "$session_dir" ] && [ -d "$session_dir" ]; then
    tok "the copied setup block really creates the session directory, repoints TMPDIR and selects session-only"
    rm -rf "$session_dir"
else
    tfail "the copied setup block really creates the session directory, repoints TMPDIR and selects session-only" \
        "the block ran but left no usable session" "$default_block"
fi

setup_block="$(qv_session_setup_block "$tmp/manual-session")"
for required in \
    "export QUOTAVISTA_NATIVE_TMPDIR=\"$tmp/manual-session\"" \
    'export TMPDIR="$QUOTAVISTA_NATIVE_TMPDIR"' \
    'export QUOTAVISTA_SECRET_STORE=session-only'
do
    if printf '%s\n' "$setup_block" | grep -qF "$required"; then
        tok "the explicit-directory setup block exports: $required"
    else
        tfail "the explicit-directory setup block exports: $required" "missing from:" "$setup_block"
    fi
done

launch_line="$(qv_launch_command "$QV_PROBE_BIN")"
for required in 'TMPDIR="$QUOTAVISTA_NATIVE_TMPDIR"' 'QUOTAVISTA_SECRET_STORE=session-only'; do
    if printf '%s' "$launch_line" | grep -qF "$required"; then
        tok "a printed launch command preserves: $required"
    else
        tfail "a printed launch command preserves: $required" "got: $launch_line"
    fi
done

launch_line="$(qv_launch_command "$QV_PROBE_BIN" QUOTAVISTA_TRAY=off)"
if printf '%s' "$launch_line" | grep -qF 'QUOTAVISTA_TRAY=off' &&
    printf '%s' "$launch_line" | grep -qF 'QUOTAVISTA_SECRET_STORE=session-only'; then
    tok "a printed launch command keeps the session-only store alongside an extra variable"
else
    tfail "a printed launch command keeps the session-only store alongside an extra variable" \
        "got: $launch_line"
fi

echo
if [ "$failures" -ne 0 ]; then
    echo "$failures native-launch regression check(s) failed" >&2
    exit 1
fi
if [ "$skips" -ne 0 ]; then
    echo "all native-launch regression checks passed ($skips skipped: the host lacks a tool the case needs)"
else
    echo "all native-launch regression checks passed"
fi
