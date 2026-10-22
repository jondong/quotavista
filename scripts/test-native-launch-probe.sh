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
# Everything here is synthetic. No QuotaVista process is launched. The
# main-path cases launch a test double only, under a temp lock and temp
# socket paths, never the real /tmp plugin sockets. The ordinary application
# identifier is temporarily replaced with a `selftest` name for the incumbent
# cases, and no credential store of any kind is touched.
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
    local partition scope day start end base amount
    for partition in 1 2; do
        if [ "$partition" -eq 1 ]; then
            scope="synthetic-kimi-demo-account"
            base=1000
        else
            scope="synthetic-openrouter-demo-account"
            base=9000
        fi
        for day in 18 19 20; do
            start="2026-10-${day}T00:00:00Z"
            end="2026-10-$((day + 1))T00:00:00Z"
            # simulated::history_facts() writes token_total + day, so a real store
            # holds three distinct, contiguous amounts per scope. The fixture
            # reproduces that shape (not the providers' own constants) so the
            # validator's distinctness and contiguity checks have something to
            # bite on, and a flattened-value corruption becomes detectable.
            amount=$((base + day - 18))
            sql "$db" "INSERT INTO usage_facts
                (partition_id, scope_id, dataset, metric, kind, event_id, key_start, key_end,
                 start_at, end_at, value_kind, amount, unit_detail, source_revision)
                VALUES (${partition}, '${scope}', 'usage', 'tokens', 'bucket', '',
                        '${start}', '${end}', '${start}', '${end}', 'tokens', '${amount}', 'total', 'synthetic-revision');"
        done
    done
}

# seed_good <db-path>
# The full, valid startup fixture. The count-preserving corruption variants
# below build on it and then change exactly one thing, so a failure names the
# check that bit rather than the whole fixture.
seed_good() {
    local db="$1"
    sql "$db" "$FIXTURE_CONNECTIONS"
    sql "$db" "$FIXTURE_SCOPES"
    sql "$db" "$FIXTURE_PARTITIONS"
    sql "$db" "$FIXTURE_RUNS"
    sql "$db" "$FIXTURE_PUBLISHED"
    fix_facts "$db"
    sql "$db" "$FIXTURE_REVISION"
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
        # ---- count-preserving corruptions --------------------------------
        # Every variant below leaves the aggregate row at
        # 8|2|2|6|2|2|en/UTC/USD. They exist because a validator that only
        # compares counts and identities cannot see them, which is the hole an
        # independent review pointed at.
        fact-scope-mismatch)
            seed_good "$db"
            # One scope's facts now claim the other scope. Total facts still 6.
            sql "$db" "UPDATE usage_facts SET scope_id = 'synthetic-openrouter-demo-account'
                WHERE partition_id = 1;"
            ;;
        partition-scope-moved)
            seed_good "$db"
            # The partition moves to the other scope; product_key changes so the
            # table's UNIQUE tuple does not reject the row. Counts unchanged.
            sql "$db" "UPDATE partitions
                SET scope_id = 'synthetic-openrouter-demo-account', product_key = 'tokens-moved'
                WHERE id = 1;"
            ;;
        chain-break)
            seed_good "$db"
            # Both published rows still exist and still name a real run, but one
            # now names the run that published a different partition.
            sql "$db" "UPDATE published_partitions SET run_id = 'run-2' WHERE partition_id = 1;"
            ;;
        flattened-amounts)
            seed_good "$db"
            # Same six facts, same scopes, same partitions: only the values were
            # flattened, so every distinct-value and contiguity check must bite.
            sql "$db" "UPDATE usage_facts SET amount = '1000';"
            ;;
        zero-amount)
            seed_good "$db"
            sql "$db" "UPDATE usage_facts SET amount = '0'
                WHERE partition_id = 1 AND start_at = '2026-10-18T00:00:00Z';"
            ;;
        nonnumeric-amount)
            seed_good "$db"
            sql "$db" "UPDATE usage_facts SET amount = '12ab'
                WHERE partition_id = 1 AND start_at = '2026-10-18T00:00:00Z';"
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

# Count-preserving corruptions. Each of these reads back as the very same
# aggregate row the known-good fixture does, so a validator that compared counts
# and identities only would pass all six. Asserting the aggregate is identical is
# what makes these cases prove that point rather than merely fail for some
# unrelated reason.
echo
echo "== count-preserving corruptions =="
good_row="$(store_snapshot good-compare "$good_dir" >/dev/null 2>&1; printf '%s' "$snap_row")"
for variant in fact-scope-mismatch partition-scope-moved chain-break \
    flattened-amounts zero-amount nonnumeric-amount; do
    corrupt_dir="$(build_fixture "$variant" "$variant")"
    corrupt_row="$(store_snapshot "$variant" "$corrupt_dir" >/dev/null 2>&1; printf '%s' "$snap_row")"
    if [ "$corrupt_row" != "$good_row" ]; then
        tfail "$variant preserves the aggregate" \
            "expected '$good_row', got '$corrupt_row'"
        continue
    fi
    tok "$variant preserves the aggregate row ${corrupt_row}"
    run_validator "$variant is rejected despite the identical aggregate" 1 "$corrupt_dir"
done

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
        # The state vocabulary the probe records, checked while the holder lives.
        state="$(qv_socket_state "$fake_ordinary")"
        if [ "$state" = "live:$listener" ]; then
            tok "qv_socket_state reports live:<pid> for a held socket"
        else
            tfail "qv_socket_state reports live:<pid> for a held socket" "got '$state'"
        fi
        if [ "$(qv_socket_state "$tmp/no-such-socket")" = "absent" ]; then
            tok "qv_socket_state reports absent for a path that does not exist"
        else
            tfail "qv_socket_state reports absent for a path that does not exist" \
                "got '$(qv_socket_state "$tmp/no-such-socket")'"
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
        state="$(qv_socket_state "$fake_probe")"
        if [ "$state" = "stale" ]; then
            tok "qv_socket_state reports stale for an existing socket nothing holds"
        else
            tfail "qv_socket_state reports stale for an existing socket nothing holds" \
                "got '$state'"
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

    # A LIVE validation socket must never be unlinked. The validation identifier
    # is a fixed constant, so two overlapping runs would share this channel, and
    # the preflight gate is a check, not a lock.
    rm -f "$fake_probe"
    python3 -c '
import socket, sys, time
s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.bind(sys.argv[1])
s.listen(1)
time.sleep(30)
' "$fake_probe" >/dev/null 2>&1 &
    live_probe=$!
    cleanup_pids+=("$live_probe")
    waited=0
    while [ ! -S "$fake_probe" ] && [ "$waited" -lt 50 ]; do
        sleep 0.1
        waited=$((waited + 1))
    done
    if [ -S "$fake_probe" ]; then
        removed="$(qv_remove_stale_probe_socket)"
        if [ -z "$removed" ] && [ -e "$fake_probe" ]; then
            tok "cleanup refuses to unlink a validation socket another process holds"
        else
            tfail "cleanup refuses to unlink a validation socket another process holds" \
                "returned '$removed'; still exists: $([ -e "$fake_probe" ] && echo yes || echo no)"
        fi
        if qv_probe_socket_is_clear >/dev/null 2>&1; then
            tfail "the clear-channel check rejects a held validation socket" "it returned 0"
        else
            tok "the clear-channel check rejects a held validation socket"
        fi
        if out="$(qv_detect_incumbents 2>&1)" && ! printf '%s' "$out" | grep -q "$fake_probe"; then
            tfail "incumbent detection names a held validation socket" \
                "output did not mention $fake_probe" "$out"
        else
            tok "incumbent detection names a held validation socket"
        fi
    else
        tskip "cleanup refuses to unlink a validation socket another process holds" \
            "python3 could not bind $fake_probe"
    fi
    kill "$live_probe" 2>/dev/null
    wait "$live_probe" 2>/dev/null
    rm -f "$fake_probe"

    # Fail closed on an inspection we cannot interpret. lsof exits 1 both when
    # nothing matched and when it could not inspect, so "no holders found" and
    # "inspection failed" are indistinguishable from the outside; the state
    # vocabulary makes that explicit and every consumer must treat it as blocked.
    touch "$fake_probe"
    saved_state_fn="$(declare -f qv_socket_state)"
    qv_socket_state() { printf 'unknown:test-failure\n'; }
    if out="$(qv_detect_incumbents 2>&1)" && [ -z "$out" ]; then
        tfail "an uninterpretable socket state is treated as an incumbent" \
            "detection printed nothing"
    elif printf '%s' "$out" | grep -q 'cannot be shown to be clear'; then
        tok "an uninterpretable socket state is treated as an incumbent"
    else
        tfail "an uninterpretable socket state is treated as an incumbent" \
            "unexpected output" "$out"
    fi
    removed="$(qv_remove_stale_probe_socket)"
    if [ -z "$removed" ] && [ -e "$fake_probe" ]; then
        tok "an uninterpretable socket state is never unlinked"
    else
        tfail "an uninterpretable socket state is never unlinked" "returned '$removed'"
    fi
    if qv_probe_socket_is_clear >/dev/null 2>&1; then
        tfail "the clear-channel check fails closed on an uninterpretable state" "it returned 0"
    else
        tok "the clear-channel check fails closed on an uninterpretable state"
    fi
    eval "$saved_state_fn"
    rm -f "$fake_probe"
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

# ---- the run lock ----
# Two agents in different worktrees share `/tmp` but not `target/native-probe`,
# so they would share one single-instance channel. The incumbent gate is a
# check, not a lock; these cases pin the lock's behaviour.
lock_dir="$tmp/qv-selftest.lock"
rm -rf "$lock_dir"
if qv_acquire_lock "$lock_dir" >/dev/null 2>&1; then
    tok "an unheld lock directory is acquired"
else
    tfail "an unheld lock directory is acquired" "qv_acquire_lock returned non-zero"
fi
if [ "$(cat "$lock_dir/owner" 2>/dev/null)" = "$$" ]; then
    tok "the lock records the acquiring shell's pid"
else
    tfail "the lock records the acquiring shell's pid" \
        "owner file holds '$(cat "$lock_dir/owner" 2>/dev/null)'"
fi
lock_reason="$(qv_acquire_lock "$lock_dir" 2>&1)" && lock_rc=0 || lock_rc=1
if [ "$lock_rc" -ne 0 ] && [ "${lock_reason#another native-check run}" != "$lock_reason" ]; then
    tok "a second acquisition is refused while the owner is alive"
else
    tfail "a second acquisition is refused while the owner is alive" \
        "rc=${lock_rc} reason='${lock_reason}'"
fi
# A dead owner makes the lock stale, and stale is reported, never auto-removed:
# deciding that belongs to the operator, not to a script that might be wrong.
sleep 0.2 &
dead_pid=$!
wait "$dead_pid" 2>/dev/null
printf '%s\n' "$dead_pid" >"$lock_dir/owner"
lock_reason="$(qv_acquire_lock "$lock_dir" 2>&1)" && lock_rc=0 || lock_rc=1
if [ "$lock_rc" -ne 0 ] && [ "${lock_reason#stale lock}" != "$lock_reason" ]; then
    tok "a lock whose recorded owner has exited is reported stale, not taken over"
else
    tfail "a lock whose recorded owner has exited is reported stale, not taken over" \
        "rc=${lock_rc} reason='${lock_reason}'"
fi
if [ "$(qv_release_lock "$lock_dir" 2>/dev/null)" = "" ] && [ -d "$lock_dir" ]; then
    tok "release refuses to remove a lock owned by another pid"
else
    tfail "release refuses to remove a lock owned by another pid" \
        "the lock directory was removed or release printed something"
fi
printf '%s\n' "$$" >"$lock_dir/owner"
if [ "$(qv_release_lock "$lock_dir" 2>/dev/null)" = "$lock_dir" ] && [ ! -d "$lock_dir" ]; then
    tok "release removes a lock this shell owns"
else
    tfail "release removes a lock this shell owns" "directory still present or nothing printed"
fi

# ---- the Linux channel is a bus name, not a socket ----
# On Linux the plugin claims `<identifier>.SingleInstance` on the session bus and
# never creates `/tmp/..._si.sock`, so a detector that only looks at sockets
# reports "clear" no matter what is running. These cases pin the bus branch.
host_os="$(qv_host_os)"
case "$host_os" in
    linux | darwin | other) tok "qv_host_os reports a known platform (${host_os})" ;;
    *) tfail "qv_host_os reports a known platform" "printed '${host_os}'" ;;
esac
# Run this in a subshell with an empty PATH so the case is deterministic on a
# host that does have gdbus (this one does, via Homebrew) as well as one that
# does not. `command -v` is a builtin, so the guard still evaluates correctly.
# bash is named by absolute path ($BASH) because `env` cannot resolve a program
# through an empty PATH; naming it lets the subshell start with nothing findable.
mkdir -p "$tmp/emptybin"
no_gdbus="$(env -i PATH="$tmp/emptybin" "$BASH" -c \
    ". '$root/scripts/native-launch-lib.sh'; qv_dbus_state com.quotavista.prototype")"
if [ "$no_gdbus" = "unknown:no-gdbus" ]; then
    tok "a missing gdbus reports unknown rather than free"
else
    tfail "a missing gdbus reports unknown rather than free" "printed '${no_gdbus}'"
fi
saved_dbus_fn="$(declare -f gdbus 2>/dev/null || true)"
gdbus() { printf '(true,)\n'; }
if [ "$(qv_dbus_state com.quotavista.prototype)" = "owned" ]; then
    tok "an owned session-bus name is reported as owned"
else
    tfail "an owned session-bus name is reported as owned" \
        "printed '$(qv_dbus_state com.quotavista.prototype)'"
fi
saved_host_fn="$(declare -f qv_host_os)"
qv_host_os() { printf 'linux\n'; }
incumbents="$(qv_detect_incumbents)"
if [ "${incumbents#*is owned, so an instance may already be running}" != "$incumbents" ]; then
    tok "on Linux an owned bus name is reported as an incumbent"
else
    tfail "on Linux an owned bus name is reported as an incumbent" "detection printed:
${incumbents}"
fi
gdbus() { printf 'gdbus: no such bus\n' >&2; return 1; }
incumbents="$(qv_detect_incumbents)"
if [ "${incumbents#*cannot be shown to be clear}" != "$incumbents" ]; then
    tok "on Linux a bus that will not answer fails closed"
else
    tfail "on Linux a bus that will not answer fails closed" "detection printed:
${incumbents}"
fi
gdbus() { printf '(false,)\n'; }
incumbents="$(qv_detect_incumbents)"
if [ "$incumbents" = "" ]; then
    tok "on Linux a free bus name and no socket reports a clear host"
else
    tfail "on Linux a free bus name and no socket reports a clear host" "detection printed:
${incumbents}"
fi
eval "$saved_host_fn"
if [ -n "$saved_dbus_fn" ]; then
    eval "$saved_dbus_fn"
else
    unset -f gdbus
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
echo "== post-launch channel and no-tray main path =="

# Helper pins. The main-path cases below are what prove main() uses them.
if [ "$(qv_dbus_state_on_bus "$QV_PROBE_IDENTIFIER" "")" = "unknown:no-bus-address" ]; then
    tok "an empty bus address is unknown, never free"
else
    tfail "an empty bus address is unknown, never free" \
        "printed '$(qv_dbus_state_on_bus "$QV_PROBE_IDENTIFIER" "")'"
fi
saved_main_gdbus="$(declare -f gdbus 2>/dev/null || true)"
gdbus() { printf '(false,)\n'; }
if [ "$(qv_dbus_state_on_bus "$QV_PROBE_IDENTIFIER" "unix:path=$tmp/unused-bus")" = "free" ]; then
    tok "NameHasOwner false on a named bus is free"
else
    tfail "NameHasOwner false on a named bus is free" \
        "printed '$(qv_dbus_state_on_bus "$QV_PROBE_IDENTIFIER" "unix:path=$tmp/unused-bus")'"
fi
gdbus() { printf 'gdbus: no such bus\n' >&2; return 1; }
if [ "$(qv_dbus_state_on_bus "$QV_PROBE_IDENTIFIER" "unix:path=$tmp/unused-bus")" = "unknown:query-failed" ]; then
    tok "a named bus that will not answer is unknown"
else
    tfail "a named bus that will not answer is unknown" \
        "printed '$(qv_dbus_state_on_bus "$QV_PROBE_IDENTIFIER" "unix:path=$tmp/unused-bus")'"
fi
if [ -n "$saved_main_gdbus" ]; then
    eval "$saved_main_gdbus"
else
    unset -f gdbus
fi

unit_bus="unix:path=$tmp/unit-child-bus"
# A shell, not /bin/sleep: macOS ps hides the environment of platform binaries.
cat >"$tmp/unit-sleeper" <<'EOF'
#!/usr/bin/env bash
sleep 30
EOF
chmod +x "$tmp/unit-sleeper"
(
    export DBUS_SESSION_BUS_ADDRESS="$unit_bus"
    exec "$tmp/unit-sleeper"
) &
unit_pid=$!
cleanup_pids+=("$unit_pid")
sleep 0.3
unit_got="$(qv_process_bus_address "$unit_pid")"
kill "$unit_pid" 2>/dev/null || true
wait "$unit_pid" 2>/dev/null || true
if [ "$unit_got" = "$unit_bus" ]; then
    tok "the launched process's session bus is read back from the process"
else
    tfail "the launched process's session bus is read back from the process" \
        "got '${unit_got:-<empty>}'"
fi

main_active="$(build_fixture main-active good)"
main_restarted="$(build_fixture main-restarted good)"
sql "$main_restarted/quotavista.sqlite" "UPDATE connections SET lifecycle = 'needs_credentials';"
main_schema="$(build_fixture main-schema schema-only)"
FIXTURE_ACTIVE="$main_active/quotavista.sqlite"
FIXTURE_RESTARTED="$main_restarted/quotavista.sqlite"
FIXTURE_SCHEMA="$main_schema/quotavista.sqlite"

# Re-exec so the process bus differs from the caller's. A probe that queries
# its own bus, or --session, fails the Linux control.
cat >"$tmp/fake-desktop" <<'EOF'
#!/usr/bin/env bash
# com.quotavista.prototype.nativeprobe
# Test double only: no native app, network, keychain, or real data.
set -u
if [ "${QV_FAKE_EXECED:-}" != 1 ]; then
    export QV_FAKE_EXECED=1
    export DBUS_SESSION_BUS_ADDRESS="${CASE_CHILD_BUS:?}"
    exec "$0"
fi
root="${CASE_ROOT:?}"
count_path="$root/count"
count=0
if [ -f "$count_path" ]; then
    count=$(cat "$count_path")
fi
count=$((count + 1))
printf '%s\n' "$count" >"$count_path"
if [ "$count" -eq 2 ]; then
    kill -USR1 "$(cat "$root/first.pid")"
    sleep 0.1
    exit 0
fi
if [ "$count" -eq 1 ]; then
    printf '%s\n' "$$" >"$root/first.pid"
    if [ "${CASE_OS:-}" = "darwin" ]; then
        : >"$root/validation.sock"
    fi
fi
trap 'printf "quotavista: SingleInstanceActivation\n" >&2' USR1
mkdir -p "${TMPDIR:?}/quotavista-prototype"
dest="${TMPDIR}/quotavista-prototype/quotavista.sqlite"
copy_db() {
    if [ -n "${1:-}" ] && [ -f "$1" ]; then
        cp "$1" "$dest"
    fi
}
case "$count" in
    1) copy_db "${FIXTURE_ACTIVE:?}" ;;
    3) copy_db "${FIXTURE_RESTARTED:?}" ;;
    4)
        case "${CASE_NOTRAY:-good}" in
            empty) ;;
            schema) copy_db "${FIXTURE_SCHEMA:?}" ;;
            *) copy_db "${FIXTURE_ACTIVE:?}" ;;
        esac
        ;;
esac
if [ "$count" -eq 4 ] && [ -n "${CASE_FATAL:-}" ]; then
    printf 'quotavista: %s\n' "$CASE_FATAL" >&2
fi
printf 'quotavista: capabilities tray=Unavailable notification=Unconfirmed\n' >&2
# read is a builtin, so SIGUSR1 runs the trap at once. An external sleep
# would defer it until that sleep exited, and the probe would miss the line.
while true; do
    read -t 30 _ || true
done
EOF
chmod +x "$tmp/fake-desktop"

# main re-sources the library, which would wipe overrides. The dot wrapper
# puts them back. Diagnostics, snapshot validation, and the bus helpers stay real.
run_probe_main() {
    local name="$1"
    case_root="$tmp/main-$name"
    case_log="$case_root/probe.log"
    mkdir -p "$case_root"
    case_rc=0
    (
        export CASE_ROOT="$case_root"
        export CASE_OS="${CASE_OS:?}"
        export CASE_DBUS="${CASE_DBUS:-owned}"
        export CASE_NOTRAY="${CASE_NOTRAY:-good}"
        export CASE_FATAL="${CASE_FATAL-}"
        export CASE_CHILD_BUS="unix:path=${case_root}/child-bus"
        export DBUS_SESSION_BUS_ADDRESS="unix:path=${case_root}/parent-bus"
        export FIXTURE_ACTIVE FIXTURE_RESTARTED FIXTURE_SCHEMA
        export QV_FAKE_BIN="$tmp/fake-desktop"
        # shellcheck disable=SC2329
        function .() {
            builtin source "$@"
            QV_PROBE_BIN="$QV_FAKE_BIN"
            QV_LOCK_DIR="$CASE_ROOT/lock"
            qv_ordinary_socket() { printf '%s/ordinary.sock\n' "$CASE_ROOT"; }
            qv_probe_socket() { printf '%s/validation.sock\n' "$CASE_ROOT"; }
            qv_host_os() { printf '%s\n' "$CASE_OS"; }
            gdbus() {
                printf '%s\n' "$*" >>"$CASE_ROOT/gdbus.log"
                case "$*" in
                    *"--address ${CASE_CHILD_BUS}"*"${QV_PROBE_IDENTIFIER}.SingleInstance"*)
                        case "$CASE_DBUS" in
                            owned) printf '(true,)\n' ;;
                            free) printf '(false,)\n' ;;
                            *) printf 'gdbus: no such bus\n' >&2; return 1 ;;
                        esac
                        ;;
                    *"--session"*)
                        printf '(false,)\n'
                        ;;
                    *)
                        printf 'gdbus: unexpected invocation\n' >&2
                        return 1
                        ;;
                esac
            }
        }
        main --skip-build
    ) >"$case_log" 2>&1 || case_rc=$?
    if pgrep -f "$tmp/fake-desktop" >/dev/null 2>&1; then
        pkill -f "$tmp/fake-desktop" >/dev/null 2>&1 || true
        tfail "$name left a test double running" "killed stragglers after main exited"
    fi
}

assert_probe() {
    local name="$1" want_rc="$2"
    shift 2
    if [ "$case_rc" -ne "$want_rc" ]; then
        tfail "$name exits $want_rc" "exited $case_rc" "$(grep -E 'RESULT:|PROBE:' "$case_log" | tail -30)"
        return
    fi
    local pattern
    for pattern in "$@"; do
        if ! grep -qF -- "$pattern" "$case_log"; then
            tfail "$name log contains: $pattern" "missing" "$(tail -40 "$case_log")"
            return
        fi
    done
    tok "$name"
}

assert_child_bus() {
    local name="$1"
    if [ ! -f "$case_root/gdbus.log" ]; then
        tfail "$name queried the child bus" "no gdbus.log"
        return
    fi
    if ! grep -qF -- "--address unix:path=${case_root}/child-bus" "$case_root/gdbus.log" ||
        ! grep -qF -- "${QV_PROBE_IDENTIFIER}.SingleInstance" "$case_root/gdbus.log"; then
        tfail "$name queried the child bus" "gdbus.log:" "$(cat "$case_root/gdbus.log")"
        return
    fi
    if grep -qF -- "--address unix:path=${case_root}/parent-bus" "$case_root/gdbus.log"; then
        tfail "$name did not query the caller's bus" "gdbus.log:" "$(cat "$case_root/gdbus.log")"
        return
    fi
    tok "$name queried the launched process's bus, not the caller's"
}

CASE_OS=darwin
CASE_DBUS=owned
CASE_NOTRAY=good
CASE_FATAL=""
run_probe_main darwin-pass
assert_probe "darwin control passes through main" 0 \
    "PROBE: PASS" \
    "RESULT: PASS (single-instance isolation from the ordinary application)" \
    "a validation socket appeared" \
    "RESULT: PASS (no-tray override reported the tray as unavailable (forced branch))" \
    "the no-tray run committed the expected deterministic fixture"

CASE_OS=linux
CASE_DBUS=owned
CASE_NOTRAY=good
CASE_FATAL=""
run_probe_main linux-owned
assert_probe "linux control passes without a Unix socket" 0 \
    "PROBE: PASS" \
    "RESULT: PASS (single-instance isolation from the ordinary application)" \
    "validation bus name ownership: owned" \
    "RESULT: PASS (no-tray override reported the tray as unavailable (forced branch))"
assert_child_bus "linux control"
if [ -e "$case_root/validation.sock" ]; then
    tfail "linux control did not require a Unix socket" "validation.sock was created"
elif grep -qF "no validation socket appeared, so single-instance registration is unconfirmed" "$case_log"; then
    tfail "linux control did not fail closed on a missing socket" "socket-only failure line was printed"
else
    tok "linux control passed with no validation socket"
fi

CASE_OS=linux
CASE_DBUS=free
CASE_NOTRAY=good
CASE_FATAL=""
run_probe_main linux-free
assert_probe "linux free bus name fails isolation" 1 \
    "PROBE: FAIL" \
    "RESULT: FAIL (single-instance isolation from the ordinary application)" \
    "validation bus name ownership: free"
assert_child_bus "linux free"

CASE_OS=linux
CASE_DBUS=fail
CASE_NOTRAY=good
CASE_FATAL=""
run_probe_main linux-unknown
assert_probe "linux unknown bus ownership fails isolation" 1 \
    "PROBE: FAIL" \
    "RESULT: FAIL (single-instance isolation from the ordinary application)" \
    "validation bus name ownership: unknown:query-failed"
assert_child_bus "linux unknown"

CASE_OS=darwin
CASE_DBUS=owned
CASE_NOTRAY=good
CASE_FATAL="demo seeding failed: synthetic"
run_probe_main notray-seed
assert_probe "no-tray demo seeding failed is fatal" 1 \
    "PROBE: FAIL" \
    "RESULT: FAIL (no-tray override reported the tray as unavailable (forced branch))" \
    "RESULT: PASS (single-instance isolation from the ordinary application)" \
    "RESULT: PASS (cold run committed the expected synthetic fixture)" \
    "demo seeding failed: synthetic"

CASE_OS=darwin
CASE_DBUS=owned
CASE_NOTRAY=good
CASE_FATAL="credential store unavailable: synthetic"
run_probe_main notray-secret
assert_probe "no-tray credential store unavailable is fatal" 1 \
    "PROBE: FAIL" \
    "RESULT: FAIL (no-tray override reported the tray as unavailable (forced branch))" \
    "RESULT: PASS (cold run committed the expected synthetic fixture)" \
    "credential store unavailable: synthetic"

CASE_OS=darwin
CASE_DBUS=owned
CASE_NOTRAY=empty
CASE_FATAL=""
run_probe_main notray-empty
assert_probe "no-tray empty store fails fixture validation" 1 \
    "PROBE: FAIL" \
    "RESULT: FAIL (no-tray override reported the tray as unavailable (forced branch))" \
    "RESULT: PASS (cold run committed the expected synthetic fixture)" \
    "no database"

CASE_OS=darwin
CASE_DBUS=owned
CASE_NOTRAY=schema
CASE_FATAL=""
run_probe_main notray-schema
assert_probe "no-tray schema-only store fails fixture validation" 1 \
    "PROBE: FAIL" \
    "RESULT: FAIL (no-tray override reported the tray as unavailable (forced branch))" \
    "RESULT: PASS (cold run committed the expected synthetic fixture)" \
    "the no-tray store did not match the expected fixture"

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
