#!/usr/bin/env bash
# Non-interactive native launch probe (P1-13).
#
# `scripts/native-checks.sh --interactive` needs a human at the desktop. This
# probe covers the part of a native run that does NOT: it launches the built
# prototype, reads the machine-readable diagnostics the composition prints on
# stderr, and inspects the throwaway state the run created. Everything here is
# an observation of a real process on a real operating system, so the same
# script produces comparable evidence on macOS and on Ubuntu.
#
# What this probe can and cannot prove:
#   * It proves the binary started, the composition initialised, the credential
#     store choice took effect, the tray and notification adapters reported what
#     they detected, a second launch was handed to the first instance, and the
#     committed synthetic fixtures survived a restart with their identities,
#     relationships and settings intact.
#   * It does NOT prove a human can see the window, the tray icon, or a
#     notification banner. Those remain interactive steps in
#     `scripts/native-checks.sh --interactive`, and this script prints them as
#     NOT OBSERVED rather than guessing.
#   * Reaching `quotavista: capabilities` is a startup milestone of the process,
#     not evidence that a usable window was rendered.
#
# Isolation (why this probe does not run the ordinary binary):
#   The pinned `tauri-plugin-single-instance` 2.5.4 names its rendezvous point
#   from the compile-time application identifier, not from `TMPDIR`:
#   `/tmp/com_quotavista_prototype_si.sock` on macOS and the session D-Bus name
#   `com.quotavista.prototype.SingleInstance` on Linux. A second launch that can
#   connect exits 0 and activates whatever already owns that channel, so running
#   the ordinary build here could show a real instance's window and drive its
#   wake path against its real state. This probe therefore builds and runs a
#   validation-only identity, `com.quotavista.prototype.nativeprobe`, whose
#   channel is disjoint. Same source, same commit, one config field different.
#   `scripts/native-launch-lib.sh` records the mechanism and the evidence that
#   the ordinary channel is left untouched.
#
# Fail closed:
#   Before launching anything the probe refuses to run if it can detect an
#   incumbent - a process holding either single-instance socket, or a running
#   copy of either binary. Detection uses the process table and `lsof`; it never
#   connects to a socket, because connecting IS the activation being avoided.
#
# Safety:
#   * The prototype's own state directory is `std::env::temp_dir()` +
#     `quotavista-prototype`. Every launch below overrides `TMPDIR` with a fresh
#     throwaway directory created by this script, so the default prototype
#     directory is never created, read, modified, or deleted. The probe asserts
#     that before and after.
#   * Every launch sets `QUOTAVISTA_SECRET_STORE=session-only`. The probe
#     therefore performs no write, read, or delete against the real macOS
#     Keychain or the Linux Secret Service, and never risks a credential-store
#     access prompt that would need a human to dismiss. Real native
#     credential-store evidence comes from the separately gated smoke test
#     `macos_keychain_synthetic_roundtrip`, which uses a disposable
#     `quotavista-test-` service.
#   * All data is synthetic. No provider is contacted; the seeded demo
#     connections use the synthetic providers compiled into the prototype.
#
# Usage: scripts/native-launch-probe.sh [--keep] [--skip-build]
#   --keep        leave the probe root on disk and print its path (inspection).
#   --skip-build  use the existing validation binary; fail if it is missing.
#
# Sourcing this file (BASH_SOURCE != $0) defines its functions without running
# anything, which is how scripts/test-native-launch-probe.sh exercises the
# store validator against synthetic fixtures.

set -uo pipefail

# --------------------------------------------------------------- constants

READY_MARKER="quotavista: capabilities"
READY_TIMEOUT_SECONDS=90
QUIT_TIMEOUT_SECONDS=15

# The fixtures the prototype commits on a first launch, derived from source at
# this commit and asserted below rather than assumed:
#   * crates/providers/src/lib.rs:19 `synthetic_providers()` - two adapters,
#     ids `synthetic-kimi` and `synthetic-openrouter`.
#   * apps/desktop/src-tauri/src/lib.rs `seed_demo_connections()` - one
#     connection per provider (`{id}-demo`, generation 0, lifecycle active, no
#     credential reference) plus one scope (`{connection}-account`, key
#     `synthetic-account`, type `account`), then one refresh of all connections.
#   * crates/providers/src/simulated.rs:30 `HISTORY_DAYS = 3` - a complete
#     stream contributes exactly three UTC day facts per connection, so six
#     facts in total. The day intervals move with the current date; the counts
#     do not.
# If a future commit legitimately changes a fixture, these constants must change
# with it and the probe fails loudly until they do.
EXPECTED_REVISION=8
EXPECTED_CONNECTIONS=2
EXPECTED_CONNECTION_IDS="synthetic-kimi-demo synthetic-openrouter-demo"
EXPECTED_SCOPES=2
EXPECTED_SCOPE_KEY="synthetic-account"
EXPECTED_SCOPE_TYPE="account"
EXPECTED_FACTS=6
EXPECTED_PARTITIONS=2
EXPECTED_SYNC_RUNS=2
EXPECTED_SETTINGS="en/UTC/USD"
# Relational and content shape, measured from a real launch and derived from
# seed_demo_connections() plus simulated::history_facts(). These exist because
# aggregate counts alone cannot distinguish the fixture from a corruption that
# preserves them: facts moved to the wrong scope, a published partition pointing
# at another run's partition, or six facts whose values were all flattened still
# read back as 8|2|2|6|2|2|en/UTC/USD.
EXPECTED_SCOPE_IDS="synthetic-kimi-demo-account synthetic-openrouter-demo-account"
EXPECTED_FACTS_PER_SCOPE="3,3"
EXPECTED_PARTITIONS_PER_SCOPE="1,1"
EXPECTED_AMOUNT_VARIANTS_PER_SCOPE="3,3"
EXPECTED_FACT_KINDS="bucket"

# The lifecycle differs between the first launch and every later one, and the
# reason is behaviour worth asserting rather than tolerating: a demo connection is
# seeded Active with no credential_ref, and `reconcile_connection_credentials`
# (crates/infrastructure/src/secrets.rs:47) treats "Active with no retrievable
# credential" as needing credentials, so the restart reconciliation marks both
# connections needs_credentials. That is the documented promise in
# apps/desktop/src-tauri/src/lib.rs ("a session-only credential never survives a
# restart, so its connection reports needs_credentials in the first list the
# renderer reads, without any implicit credential collection"), observed here on a
# real native launch. The expectation is therefore per-launch, not a constant.
EXPECTED_LIFECYCLES_SEEDED="active "
EXPECTED_LIFECYCLES_RESTARTED="needs_credentials "

# Diagnostics that must never appear in a launch this probe treats as healthy.
FATAL_DIAGNOSTICS="demo seeding failed|credential store unavailable"

# Outputs of store_snapshot. Declared here as well as inside the function so a
# caller that reads one before any snapshot has run sees an empty string rather
# than an unset variable under `set -u`.
snap_row="" snap_readable=0
snap_revision="" snap_connections="" snap_scopes="" snap_facts=""
snap_partitions="" snap_sync_runs="" snap_settings=""
snap_connection_ids="" snap_lifecycles="" snap_credential_refs=""
snap_scope_orphans="" snap_scope_keys=""

# --------------------------------------------------------------- utilities

now_ms() {
  if command -v python3 >/dev/null 2>&1; then
    python3 -c 'import time; print(int(time.time() * 1000))'
  else
    echo $(( $(date +%s) * 1000 ))
  fi
}

note() { printf '%s\n' "$*"; }

step() { printf '\n### %s\n' "$1"; }

pass() { printf 'RESULT: PASS (%s)\n' "$1"; }

fail() {
  printf 'RESULT: FAIL (%s)\n' "$1"
  failures=$((failures + 1))
}

# A check this probe deliberately cannot make. Never printed as a pass.
not_observed() { printf 'NOT OBSERVED: %s\n' "$1"; }

# expect <label> <field> <actual> <expected>
expect() {
  local label="$1" field="$2" actual="$3" wanted="$4"
  if [ "$actual" = "$wanted" ]; then
    note "    ${label} ${field}: ${actual} (as expected)"
    return 0
  fi
  note "    ${label} ${field}: found '${actual}', expected '${wanted}'"
  return 1
}

# --------------------------------------------------------- store inspection

# store_query <db-path> <sql>
# Read-only query against the prototype's SQLite state. Kept at top level (not
# nested inside store_snapshot) so the reader of a failing line can see exactly
# which file and SQL produced it.
store_query() {
  sqlite3 "file:$1?mode=ro" "$2" 2>&1
}

# store_snapshot <label> <data-dir>
# Reads the prototype's SQLite state read-only, after the writer has exited, and
# fills the snap_* globals. Success here means "the database was readable"; it
# says nothing about whether the fixtures are correct. That is what
# validate_snapshot is for, and the two are deliberately separate calls so a
# readable-but-empty store cannot pass as a seeded one.
store_snapshot() {
  local label="$1"
  local data="$2"
  local db="$data/quotavista.sqlite"

  snap_readable=0
  snap_revision="" snap_connections="" snap_scopes="" snap_facts=""
  snap_partitions="" snap_sync_runs="" snap_settings=""
  snap_connection_ids="" snap_scope_orphans="" snap_scope_keys=""
  snap_credential_refs="" snap_lifecycles="" snap_row=""
  snap_scope_ids="" snap_fact_scope_mismatch="" snap_run_chain_breaks=""
  snap_facts_per_scope="" snap_partitions_per_scope=""
  snap_amount_variants_per_scope="" snap_amount_bad="" snap_amount_gaps=""
  snap_fact_kinds=""

  if ! command -v sqlite3 >/dev/null 2>&1; then
    not_observed "${label}: sqlite3 is not installed, so committed state was not inspected"
    return 1
  fi
  if [ ! -f "$db" ]; then
    note "- ${label}: no database at $db"
    return 1
  fi


  local out
  if ! out="$(store_query "$db" 'SELECT (SELECT data_revision FROM store_state)
      || "|" || (SELECT COUNT(*) FROM connections)
      || "|" || (SELECT COUNT(*) FROM provider_scopes)
      || "|" || (SELECT COUNT(*) FROM usage_facts)
      || "|" || (SELECT COUNT(*) FROM published_partitions)
      || "|" || (SELECT COUNT(*) FROM sync_runs)
      || "|" || (SELECT locale || "/" || reporting_zone || "/" || currency FROM settings);')"; then
    note "- ${label}: sqlite3 could not read the store: ${out}"
    return 1
  fi

  snap_row="$out"
  snap_revision="${out%%|*}"; out="${out#*|}"
  snap_connections="${out%%|*}"; out="${out#*|}"
  snap_scopes="${out%%|*}"; out="${out#*|}"
  snap_facts="${out%%|*}"; out="${out#*|}"
  snap_partitions="${out%%|*}"; out="${out#*|}"
  snap_sync_runs="${out%%|*}"; out="${out#*|}"
  snap_settings="$out"

  snap_connection_ids="$(store_query "$db" 'SELECT id FROM connections ORDER BY id;' | tr '\n' ' ')"
  snap_lifecycles="$(store_query "$db" 'SELECT DISTINCT lifecycle FROM connections ORDER BY lifecycle;' | tr '\n' ' ')"
  snap_credential_refs="$(store_query "$db" 'SELECT COUNT(*) FROM connections WHERE credential_ref IS NOT NULL;')"
  snap_scope_orphans="$(store_query "$db" 'SELECT COUNT(*) FROM provider_scopes s
      WHERE NOT EXISTS (SELECT 1 FROM connections c WHERE c.id = s.connection_id);')"
  # The NULL guards matter: `x <> 'literal'` is NULL, not true, when x is NULL,
  # so an unguarded comparison would silently pass a row the schema forbids.
  snap_scope_keys="$(store_query "$db" "SELECT COUNT(*) FROM provider_scopes
      WHERE scope_key IS NULL OR scope_type IS NULL
         OR scope_key <> '${EXPECTED_SCOPE_KEY}' OR scope_type <> '${EXPECTED_SCOPE_TYPE}';")"
  snap_scope_ids="$(store_query "$db" 'SELECT id FROM provider_scopes ORDER BY id;' | tr '\n' ' ')"
  # Counts alone do not establish that the fixture is the fixture: a corruption
  # that preserves every aggregate could still attach facts to the wrong scope or
  # flatten their values. These are relational and content checks on the same rows.
  snap_fact_scope_mismatch="$(store_query "$db" 'SELECT COUNT(*) FROM usage_facts f
      JOIN partitions p ON p.id = f.partition_id WHERE p.scope_id <> f.scope_id;')"
  snap_run_chain_breaks="$(store_query "$db" 'SELECT COUNT(*) FROM published_partitions pp
      JOIN sync_runs r ON r.id = pp.run_id WHERE r.partition_id <> pp.partition_id;')"
  snap_facts_per_scope="$(store_query "$db" "SELECT COALESCE(group_concat(c, ','), '')
      FROM (SELECT COUNT(*) AS c FROM usage_facts GROUP BY scope_id ORDER BY scope_id);")"
  snap_partitions_per_scope="$(store_query "$db" "SELECT COALESCE(group_concat(c, ','), '')
      FROM (SELECT COUNT(*) AS c FROM partitions GROUP BY scope_id ORDER BY scope_id);")"
  snap_amount_variants_per_scope="$(store_query "$db" "SELECT COALESCE(group_concat(c, ','), '')
      FROM (SELECT COUNT(DISTINCT amount) AS c FROM usage_facts GROUP BY scope_id ORDER BY scope_id);")"
  snap_amount_bad="$(store_query "$db" "SELECT COUNT(*) FROM usage_facts
      WHERE amount IS NULL OR amount GLOB '*[^0-9]*' OR CAST(amount AS INTEGER) <= 0;")"
  # history_facts() writes token_total + day for three consecutive days, so each
  # scope's amounts are a contiguous run of distinct integers. Provider-specific
  # constants are deliberately not hardcoded; the shape is.
  snap_amount_gaps="$(store_query "$db" 'SELECT COUNT(*) FROM (SELECT scope_id FROM usage_facts
      GROUP BY scope_id
      HAVING COUNT(DISTINCT amount) <> COUNT(*)
          OR (MAX(CAST(amount AS INTEGER)) - MIN(CAST(amount AS INTEGER)) + 1) <> COUNT(*));')"
  snap_fact_kinds="$(store_query "$db" "SELECT COALESCE(group_concat(DISTINCT kind), '') FROM usage_facts;")"
  snap_readable=1

  note "- ${label} store_state.data_revision | connections | provider_scopes | usage_facts | published_partitions | sync_runs | settings(locale/zone/currency):"
  note "    ${snap_row}"
  note "- ${label} connection ids: ${snap_connection_ids:-<none>}"
  note "- ${label} connection lifecycles: ${snap_lifecycles:-<none>}"
  note "- ${label} scope ids: ${snap_scope_ids:-<none>}"
  note "- ${label} facts per scope: ${snap_facts_per_scope:-<none>} | partitions per scope: ${snap_partitions_per_scope:-<none>}"
  note "- ${label} fact kinds: ${snap_fact_kinds:-<none>} | distinct amounts per scope: ${snap_amount_variants_per_scope:-<none>}"
  return 0
}

# validate_snapshot <label> [expected-lifecycles]
# Asserts the deterministic fixture: identities, counts, relationships, revision
# and settings. Every mismatch is reported individually so a partial seed says
# which part is missing instead of collapsing into "the rows differ".
validate_snapshot() {
  local label="$1"
  local expected_lifecycles="${2:-$EXPECTED_LIFECYCLES_SEEDED}"
  local ok=0

  if [ "$snap_readable" -ne 1 ]; then
    note "- ${label}: the store was not readable, so no fixture could be validated"
    return 1
  fi

  expect "$label" "data_revision" "$snap_revision" "$EXPECTED_REVISION" || ok=1
  expect "$label" "connections" "$snap_connections" "$EXPECTED_CONNECTIONS" || ok=1
  expect "$label" "provider_scopes" "$snap_scopes" "$EXPECTED_SCOPES" || ok=1
  expect "$label" "usage_facts" "$snap_facts" "$EXPECTED_FACTS" || ok=1
  expect "$label" "published_partitions" "$snap_partitions" "$EXPECTED_PARTITIONS" || ok=1
  expect "$label" "sync_runs" "$snap_sync_runs" "$EXPECTED_SYNC_RUNS" || ok=1
  expect "$label" "settings(locale/zone/currency)" "$snap_settings" "$EXPECTED_SETTINGS" || ok=1

  local ids_sorted expected_sorted
  ids_sorted="$(printf '%s\n' $snap_connection_ids | sort | tr '\n' ' ')"
  expected_sorted="$(printf '%s\n' $EXPECTED_CONNECTION_IDS | sort | tr '\n' ' ')"
  expect "$label" "connection ids" "$ids_sorted" "$expected_sorted" || ok=1

  # A demo connection is seeded without a credential, so no row may carry one.
  expect "$label" "connections with a credential_ref" "$snap_credential_refs" "0" || ok=1
  # Every scope must belong to a connection that exists.
  expect "$label" "scopes with no matching connection" "$snap_scope_orphans" "0" || ok=1
  # Every scope must be the synthetic account scope, not something else.
  expect "$label" "scopes off the ${EXPECTED_SCOPE_KEY}/${EXPECTED_SCOPE_TYPE} fixture" \
    "$snap_scope_keys" "0" || ok=1
  expect "$label" "connection lifecycles" "$snap_lifecycles" "$expected_lifecycles" || ok=1

  # Relational and content invariants. Every one of these still holds for a
  # corruption that keeps the aggregate counts identical, which is exactly the
  # hole the count-and-compare version of this validator left open.
  local scope_ids_sorted expected_scope_ids_sorted
  scope_ids_sorted="$(printf '%s\n' $snap_scope_ids | sort | tr '\n' ' ')"
  expected_scope_ids_sorted="$(printf '%s\n' $EXPECTED_SCOPE_IDS | sort | tr '\n' ' ')"
  expect "$label" "scope ids" "$scope_ids_sorted" "$expected_scope_ids_sorted" || ok=1
  expect "$label" "facts attached to a partition of another scope" \
    "$snap_fact_scope_mismatch" "0" || ok=1
  expect "$label" "published partitions pointing at another partition's run" \
    "$snap_run_chain_breaks" "0" || ok=1
  expect "$label" "facts per scope" "$snap_facts_per_scope" "$EXPECTED_FACTS_PER_SCOPE" || ok=1
  expect "$label" "partitions per scope" \
    "$snap_partitions_per_scope" "$EXPECTED_PARTITIONS_PER_SCOPE" || ok=1
  expect "$label" "fact kinds" "$snap_fact_kinds" "$EXPECTED_FACT_KINDS" || ok=1
  expect "$label" "distinct fact amounts per scope" \
    "$snap_amount_variants_per_scope" "$EXPECTED_AMOUNT_VARIANTS_PER_SCOPE" || ok=1
  expect "$label" "fact amounts null, non-numeric, or non-positive" \
    "$snap_amount_bad" "0" || ok=1
  expect "$label" "scopes whose fact amounts are not a contiguous distinct run" \
    "$snap_amount_gaps" "0" || ok=1

  # Seeding ends with one refresh of every connection, so committed facts and a
  # published partition per connection are what "the seed completed" means. A
  # schema-only store has the tables and none of this.
  if [ "${snap_facts:-0}" -lt 1 ] || [ "${snap_partitions:-0}" -lt 1 ]; then
    note "- ${label}: no committed facts or published partitions, so the post-seed refresh did not complete"
    ok=1
  fi

  return "$ok"
}

# require_clean_diagnostics <label> <errfile> [unexpected-regex-extra]
# Fails on diagnostics that mean the run was not healthy. Previously these were
# only printed, which let a failed seed reach PROBE: PASS.
require_clean_diagnostics() {
  local label="$1" err="$2" extra="${3:-}"
  local pattern="$FATAL_DIAGNOSTICS"
  [ -n "$extra" ] && pattern="${pattern}|${extra}"
  local hits
  hits="$(grep -E "^quotavista: (${pattern})" "$err" 2>/dev/null)"
  if [ -n "$hits" ]; then
    note "- ${label}: unhealthy diagnostics on stderr:"
    printf '%s\n' "$hits" | sed 's/^/    /'
    return 1
  fi
  note "- ${label}: no seeding, credential-store, or tray failure on stderr"
  return 0
}

# --------------------------------------------------------------- launching

# launch <label> <tmpdir> [EXTRA=value ...]
# Starts the validation-identity prototype in the background under an isolated
# TMPDIR and records its pid in $launch_pid.
launch() {
  local label="$1"
  local dir="$2"
  shift 2
  local out="$PROBE_ROOT/$label.out"
  local err="$PROBE_ROOT/$label.err"
  local start
  start="$(now_ms)"
  env TMPDIR="$dir" \
    QUOTAVISTA_SECRET_STORE=session-only \
    "$@" \
    "$BIN" >"$out" 2>"$err" &
  launch_pid=$!
  launch_start_ms="$start"
  pids+=("$launch_pid")
  printf -- '- launched pid=%s tmpdir=%s binary=%s%s\n' "$launch_pid" "$dir" "$BIN" \
    "$( [ $# -gt 0 ] && printf ' extra_env=%s' "$*" )"
}

# wait_ready <label> <errfile> <start_ms>
# Polls stderr for the composition's capability line and reports the real
# elapsed time, so a launch that never got there cannot look like a fast one.
wait_ready() {
  local label="$1"
  local err="$2"
  local start="$3"
  local deadline=$(( start + READY_TIMEOUT_SECONDS * 1000 ))
  local now
  while :; do
    if grep -q "$READY_MARKER" "$err" 2>/dev/null; then
      now="$(now_ms)"
      ready_elapsed_ms=$(( now - start ))
      ready_line="$(grep "$READY_MARKER" "$err" | head -1)"
      return 0
    fi
    now="$(now_ms)"
    if [ "$now" -ge "$deadline" ]; then
      ready_elapsed_ms=""
      ready_line=""
      note "- ${label} did not print '${READY_MARKER}' within ${READY_TIMEOUT_SECONDS}s"
      if [ -s "$err" ]; then
        note '- stderr so far:'
        sed 's/^/    /' "$err" | head -20
      fi
      return 1
    fi
    sleep 0.25
  done
}

# stop <label> <pid>
# Asks the process to terminate, then reports how it actually ended. SIGTERM is
# not the tray "Quit" path: it exercises no in-flight-job cancellation message,
# so the graceful quit stays an interactive observation. It also does not run the
# plugin's `RunEvent::Exit` handler, which is why the validation socket can be
# left behind stale and why cleanup removes it.
stop() {
  local label="$1"
  local pid="$2"
  local waited=0
  kill -TERM "$pid" 2>/dev/null
  while kill -0 "$pid" 2>/dev/null; do
    if [ "$waited" -ge "$QUIT_TIMEOUT_SECONDS" ]; then
      kill -KILL "$pid" 2>/dev/null
      note "- ${label} ignored SIGTERM for ${QUIT_TIMEOUT_SECONDS}s; sent SIGKILL"
      wait "$pid" 2>/dev/null
      return 0
    fi
    sleep 0.5
    waited=$((waited + 1))
  done
  local status=0
  wait "$pid" 2>/dev/null || status=$?
  note "- ${label} exited after SIGTERM with status ${status}"
}

# --------------------------------------------------------------- reporting

report_diagnostics() {
  local label="$1"
  local err="$2"
  note "- ${label} stderr (quotavista: lines):"
  if grep -q '^quotavista: ' "$err" 2>/dev/null; then
    grep '^quotavista: ' "$err" | sed 's/^/    /'
  else
    note '    (none)'
  fi
}

report_process() {
  local label="$1"
  local pid="$2"
  if ! kill -0 "$pid" 2>/dev/null; then
    note "- ${label}: pid ${pid} is no longer running"
    return
  fi
  local ps_line
  ps_line="$(ps -o rss=,vsz=,etime= -p "$pid" 2>/dev/null)"
  note "- ${label}: pid ${pid} alive; ps rss/vsz(KiB)/elapsed: ${ps_line:-unavailable}"
  local children
  children="$(pgrep -P "$pid" 2>/dev/null | tr '\n' ' ')"
  if [ -n "${children// /}" ]; then
    note "- ${label}: child pids ${children}(a webview content process here is supplementary evidence a window was created)"
  else
    not_observed "${label}: no child process, so webview creation was not confirmed by process inspection"
  fi
}

report_data_dir() {
  local label="$1"
  local data="$2"
  note "- ${label}: state directory contents (${data}):"
  if [ -d "$data" ]; then
    ( cd "$data" && find . -maxdepth 2 -print | sort | sed 's/^/    /' )
  else
    note '    (directory does not exist)'
  fi
}

# socket_stamp <path>
# Prints a comparable stamp for a socket: absent, or its modification time.
socket_stamp() {
  local s="$1"
  if [ ! -e "$s" ]; then
    printf 'absent'
    return
  fi
  stat -f '%m' "$s" 2>/dev/null || stat -c '%Y' "$s" 2>/dev/null || printf 'present'
}

cleanup() {
  local leftover=0
  local pid
  for pid in "${pids[@]:-}"; do
    [ -n "$pid" ] || continue
    if kill -0 "$pid" 2>/dev/null; then
      kill -KILL "$pid" 2>/dev/null
      leftover=$((leftover + 1))
    fi
  done
  if [ "$leftover" -gt 0 ]; then
    note "- cleanup: force-killed ${leftover} surviving probe process(es)"
  fi
  local strays
  strays="$(qv_instances_of "$QV_PROBE_BIN" | tr '\n' ' ')"
  if [ -n "${strays// /}" ]; then
    note "- cleanup: WARNING validation processes still running: ${strays}(inspect before assuming a clean host)"
  else
    note "- cleanup: no ${QV_PROBE_BIN} process remains"
  fi
  # SIGTERM does not run the plugin's Exit handler, so remove our own stale
  # socket. Only the validation identity's socket is ever removed.
  local removed
  removed="$(qv_remove_stale_probe_socket)"
  if [ -n "$removed" ]; then
    note "- cleanup: removed the stale validation socket ${removed}"
  fi
  # The lock is taken after `pids=()` and released here, so every exit path —
  # including the refusals that exit 2 — hands `/tmp` back to the next run.
  # `qv_release_lock` only removes a lock this shell owns.
  local released
  released="$(qv_release_lock "$QV_LOCK_DIR")"
  if [ -n "$released" ]; then
    note "- cleanup: released the run lock ${released}"
  fi
}

# -------------------------------------------------------------------- main

main() {
  local KEEP=0 SKIP_BUILD=0
  local arg
  for arg in "$@"; do
    case "$arg" in
      --keep) KEEP=1 ;;
      --skip-build) SKIP_BUILD=1 ;;
      *)
        echo "usage: $0 [--keep] [--skip-build]" >&2
        exit 2
        ;;
    esac
  done

  REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
  cd "$REPO_ROOT" || exit 2
  # shellcheck source=scripts/native-launch-lib.sh
  . "$REPO_ROOT/scripts/native-launch-lib.sh"

  BIN="$QV_PROBE_BIN"
  ORDINARY_SOCKET="$(qv_ordinary_socket)"
  PROBE_SOCKET="$(qv_probe_socket)"

  PROBE_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/qv-native-probe.XXXXXX")" || exit 2
  DEFAULT_DATA_DIR="${TMPDIR:-/tmp}/quotavista-prototype"

  failures=0
  pids=()
  # The validation identifier is a fixed constant and `/tmp` is shared across
  # worktrees, so two overlapping runs would share one single-instance channel.
  # The incumbent gate below is a check, not a lock; this is the lock.
  local lock_reason
  if ! lock_reason="$(qv_acquire_lock "$QV_LOCK_DIR")"; then
    printf -- '- refusing to run: %s\n' "$lock_reason"
    printf '\nPROBE: REFUSED (another native-check run holds %s)\n' "$QV_LOCK_DIR"
    exit 2
  fi
  trap cleanup EXIT

  printf '## Native launch probe (non-interactive)\n'
  printf -- '- Repository: %s\n' "$REPO_ROOT"
  printf -- '- Commit: %s\n' "$(git rev-parse HEAD 2>/dev/null || echo unknown)"
  printf -- '- Date (UTC): %s\n' "$(date -u '+%Y-%m-%d %H:%M:%S')"
  printf -- '- Binary: %s\n' "$BIN"
  printf -- '- Application identifier: %s (validation only)\n' "$QV_PROBE_IDENTIFIER"
  printf -- '- Ordinary identifier not used: %s\n' "$QV_ORDINARY_IDENTIFIER"
  if [ "$(qv_host_os)" = "linux" ]; then
    printf -- '- Single-instance channel: session bus name %s (Linux does not use %s)\n' \
      "$(qv_dbus_name "$QV_PROBE_IDENTIFIER")" "$PROBE_SOCKET"
    printf -- '- Ordinary channel left alone: session bus name %s\n' \
      "$(qv_dbus_name "$QV_ORDINARY_IDENTIFIER")"
  else
    printf -- '- Single-instance channel: %s\n' "$PROBE_SOCKET"
    printf -- '- Ordinary channel left alone: %s\n' "$ORDINARY_SOCKET"
  fi
  printf -- '- Probe root (throwaway, created by this run): %s\n' "$PROBE_ROOT"
  printf -- '- Credential store for every launch: QUOTAVISTA_SECRET_STORE=session-only\n'
  printf -- '- Real credential store entries written, read, or deleted: none\n'
  printf -- "- Default prototype directory left alone: %s\n" "$DEFAULT_DATA_DIR"

  step "0a. Fail closed before launching: is the host clear of incumbents?"
  local host_os detection_tool
  host_os="$(qv_host_os)"
  note "- host platform: ${host_os}"
  # The channel that has to be inspectable differs by platform: a Unix socket on
  # macOS, a session-bus name on Linux. Requiring the wrong tool would either
  # refuse a runnable host or, worse, let an uninspectable one through.
  if [ "$host_os" = "linux" ]; then
    detection_tool="gdbus"
    note "- the single-instance channel on Linux is a session-bus name, not a socket in /tmp"
  else
    detection_tool="lsof"
    note "- the single-instance channel here is a Unix socket, so lsof is what proves a holder"
  fi
  if ! command -v "$detection_tool" >/dev/null 2>&1; then
    note "- ${detection_tool} is unavailable, so a live channel cannot be distinguished from a free or stale one"
    note "- refusing to launch: install ${detection_tool} and re-run (this is a refusal, not a prototype failure)"
    printf '\nPROBE: REFUSED (cannot detect incumbents)\n'
    exit 2
  fi
  local ordinary_before probe_before
  ordinary_before="$(socket_stamp "$ORDINARY_SOCKET")"
  probe_before="$(socket_stamp "$PROBE_SOCKET")"
  note "- ordinary channel ${ORDINARY_SOCKET}: ${ordinary_before}"
  note "- validation channel ${PROBE_SOCKET}: ${probe_before}"
  local ordinary_state
  ordinary_state="$(qv_socket_state "$ORDINARY_SOCKET")"
  note "- ordinary channel state: ${ordinary_state}"
  note "- validation channel state: $(qv_socket_state "$PROBE_SOCKET")"
  case "$ordinary_state" in
    absent) ;;
    stale)
      # Recorded explicitly rather than left implicit: a stale socket from an
      # earlier signal-terminated run is expected, and this script never removes
      # another identity's socket (see qv_remove_stale_probe_socket).
      note "- the ordinary channel exists but no process holds it, so this run treats it as stale"
      note "- it is deliberately left untouched, and its recorded stamp is compared again at step 1b"
      ;;
    live:*)
      note "- the ordinary channel is LIVE; the gate below must refuse to launch"
      ;;
    *)
      note "- the ordinary channel state ${ordinary_state} cannot be shown to be clear; the gate below must refuse"
      ;;
  esac
  if ! qv_require_clear_host; then
    note "- refusing to launch while an incumbent is detectable; quit it and re-run"
    note "- this is a refusal to run, not a prototype failure"
    printf '\nPROBE: REFUSED (incumbent detected)\n'
    exit 2
  fi
  if [ "$host_os" = "linux" ]; then
    note "- no session-bus name is owned, no socket is held, and no copy of either binary is running"
  else
    note "- no process holds either single-instance socket and no copy of either binary is running"
  fi
  pass "host clear of incumbents (fail-closed gate)"

  step "0b. Validation binary with an isolated single-instance identity"
  if [ "$SKIP_BUILD" -eq 1 ]; then
    note "- --skip-build: using the existing ${BIN}"
    note "- provenance is still verified below: the identifier is read back out of"
    note "  the binary before anything is launched, so a cached artifact built for"
    note "  the ordinary identity is rejected rather than activated"
  else
    note "- Command: TAURI_CONFIG='{\"identifier\":\"${QV_PROBE_IDENTIFIER}\"}' CARGO_TARGET_DIR=${QV_PROBE_TARGET_DIR} cargo build --locked -p ${QV_CRATE}"
    # cargo reprints the same ts-rs attribute warning once per affected type,
    # which buries the handful of lines that carry information. Print the summary
    # and state how many lines were withheld, so nothing is dropped silently.
    local build_out build_status=0 build_total
    build_out="$(qv_build_probe_binary 2>&1)" || build_status=1
    build_total="$(printf '%s\n' "$build_out" | wc -l | tr -d ' ')"
    if [ "$build_status" -ne 0 ]; then
      printf '%s\n' "$build_out" | sed 's/^/    /'
      note "- the validation build failed; nothing was launched"
      fail "validation binary built"
      printf '\nPROBE: FAIL (%d step(s))\n' "$failures"
      exit 1
    fi
    printf '%s\n' "$build_out" |
      grep -E '^(error|    Compiling |    Finished )' |
      sed 's/^/    /' || true
    note "- build output: ${build_total} line(s) total; only error, Compiling and Finished lines are shown above"
  fi
  if [ ! -x "$BIN" ]; then
    note "- $BIN is missing"
    note "- Build it with: TAURI_CONFIG='{\"identifier\":\"${QV_PROBE_IDENTIFIER}\"}' CARGO_TARGET_DIR=${QV_PROBE_TARGET_DIR} cargo build --locked -p ${QV_CRATE}"
    fail "validation binary present"
    printf '\nPROBE: FAIL (%d step(s))\n' "$failures"
    exit 1
  fi
  note "- $BIN exists and is executable"
  if qv_probe_binary_is_isolated "$BIN"; then
    note "- the binary carries the validation identifier ${QV_PROBE_IDENTIFIER}"
    pass "validation binary carries the isolated identity"
  else
    note "- the validation identifier was not found inside the binary"
    note "- refusing to launch a binary whose single-instance channel is unverified"
    fail "validation binary carries the isolated identity"
    printf '\nPROBE: FAIL (%d step(s))\n' "$failures"
    exit 1
  fi
  # A stale socket from an earlier SIGTERM is safe: the plugin cleans it up and
  # rebinds when connect answers ECONNREFUSED. Record it rather than hide it.
  if [ -e "$PROBE_SOCKET" ]; then
    note "- removing the stale validation socket left by an earlier run: ${PROBE_SOCKET}"
    qv_remove_stale_probe_socket >/dev/null
  fi

  local COLD_DIR="$PROBE_ROOT/cold"
  local NOTRAY_DIR="$PROBE_ROOT/notray"
  mkdir -p "$COLD_DIR" "$NOTRAY_DIR"
  # A warm launch must reuse the cold launch's state to be a warm launch at all.
  local WARM_DIR="$COLD_DIR"

  if [ -e "$DEFAULT_DATA_DIR" ]; then
    note "- WARNING the default prototype directory already exists: ${DEFAULT_DATA_DIR}"
    note "- it is never read, modified, or deleted by this probe; every launch overrides TMPDIR"
  else
    note "- default prototype directory absent before the run: ${DEFAULT_DATA_DIR}"
  fi

  step "1. Cold launch on an empty throwaway state directory"
  note "- Command: env TMPDIR=$COLD_DIR QUOTAVISTA_SECRET_STORE=session-only ./$BIN"
  launch cold "$COLD_DIR"
  local COLD_PID="$launch_pid"
  if wait_ready cold "$PROBE_ROOT/cold.err" "$launch_start_ms"; then
    note "- time from exec to '${READY_MARKER}': ${ready_elapsed_ms} ms"
    note "- ${ready_line}"
    report_process cold "$launch_pid"
    report_data_dir cold "$COLD_DIR/quotavista-prototype"
    report_diagnostics cold "$PROBE_ROOT/cold.err"
    not_observed "window content: the probe cannot see whether a human sees the window, the four screens, or the tray icon (interactive steps 1, 2, 5)"
    if require_clean_diagnostics cold "$PROBE_ROOT/cold.err" 'system tray unavailable'; then
      pass "cold launch reached composition readiness with clean diagnostics"
    else
      fail "cold launch reached composition readiness with clean diagnostics"
    fi
  else
    fail "cold launch reached composition readiness with clean diagnostics"
  fi

  step "1b. Isolation: no recorded change to the ordinary single-instance channel"
  local ordinary_during probe_during
  ordinary_during="$(socket_stamp "$ORDINARY_SOCKET")"
  probe_during="$(socket_stamp "$PROBE_SOCKET")"
  note "- ordinary channel ${ORDINARY_SOCKET}: before=${ordinary_before} during=${ordinary_during}"
  note "- validation channel ${PROBE_SOCKET}: before=${probe_before} during=${probe_during}"
  local isolation_ok=0
  if [ "$ordinary_before" = "$ordinary_during" ]; then
    note "- the ordinary channel's recorded stamp is unchanged"
    note "- that is a point-in-time observation only: connecting to an existing socket need"
    note "  not change its mtime, so this supports 'no recorded change', not 'no activation'"
    note "- the substantive isolation guarantee is the disjoint application identifier named"
    note "  in the header, which gives this run a different channel by construction"
  else
    note "- the ordinary channel CHANGED during a validation launch; isolation did not hold"
    isolation_ok=1
  fi
  if [ "$(qv_host_os)" = "linux" ]; then
    # The plugin never creates /tmp/<identifier>_si.sock on Linux. Requiring
    # that file fails a healthy launch. Ownership of the validation name on
    # the launched process's own session bus is the evidence; free and
    # unknown both fail closed. Do not fall back to the caller's bus.
    local bus dbus_state validation_name
    validation_name="$(qv_dbus_name "$QV_PROBE_IDENTIFIER")"
    bus="$(qv_process_bus_address "$COLD_PID")"
    note "- Linux validation channel is the session-bus name ${validation_name}, not ${PROBE_SOCKET}"
    if [ -n "$bus" ]; then
      note "- launched process ${COLD_PID} session bus: ${bus}"
    else
      note "- launched process ${COLD_PID} has no readable DBUS_SESSION_BUS_ADDRESS; ownership cannot be shown"
    fi
    dbus_state="$(qv_dbus_state_on_bus "$QV_PROBE_IDENTIFIER" "$bus")"
    note "- validation bus name ownership: ${dbus_state}"
    case "$dbus_state" in
      owned)
        note "- the launched process's session bus owns ${validation_name}, so the validation identity registered its own channel"
        ;;
      free)
        note "- ${validation_name} is free on the launched process's session bus, so single-instance registration is unconfirmed"
        isolation_ok=1
        ;;
      *)
        note "- ${validation_name} reports ${dbus_state}; unknown or unreadable ownership is not a registered channel"
        isolation_ok=1
        ;;
    esac
    if [ -e "$PROBE_SOCKET" ]; then
      note "- a file exists at ${PROBE_SOCKET}; Linux does not require it, and it is not the isolation evidence"
    fi
  else
    if [ -e "$PROBE_SOCKET" ]; then
      note "- a validation socket appeared, so the validation identity registered its own channel"
    else
      note "- no validation socket appeared, so single-instance registration is unconfirmed"
      isolation_ok=1
    fi
  fi
  if [ "$isolation_ok" -eq 0 ]; then
    pass "single-instance isolation from the ordinary application"
  else
    fail "single-instance isolation from the ordinary application"
  fi

  step "2. Single instance: a second launch hands over to the first"
  note "- Command: env TMPDIR=$COLD_DIR QUOTAVISTA_SECRET_STORE=session-only ./$BIN   (while pid $COLD_PID is still running)"
  if kill -0 "$COLD_PID" 2>/dev/null; then
    launch second "$COLD_DIR"
    local second_start="$launch_start_ms"
    local second_status=0
    wait "$launch_pid" 2>/dev/null || second_status=$?
    local second_elapsed=$(( $(now_ms) - second_start ))
    note "- second launch exited with status ${second_status} after ${second_elapsed} ms"
    if kill -0 "$COLD_PID" 2>/dev/null; then
      note "- first instance (pid ${COLD_PID}) is still running"
      report_diagnostics first-instance "$PROBE_ROOT/cold.err"
      if grep -q 'SingleInstanceActivation' "$PROBE_ROOT/cold.err" 2>/dev/null; then
        note "- the first instance logged the handover"
        pass "single instance handover (process-level: second exited early, first survived and logged it)"
      else
        not_observed "handover wake line: the first instance did not print SingleInstanceActivation, so only the second launch's early exit was observed"
        fail "single instance handover (process-level: second exited early, first survived and logged it)"
      fi
    else
      note "- first instance (pid ${COLD_PID}) is NOT running any more"
      fail "single instance handover (process-level: second exited early, first survived and logged it)"
    fi
    not_observed "focus: the probe cannot see whether the first window was shown and focused (interactive step 9)"
  else
    note "- the cold instance had already exited; the single-instance check was not run"
    fail "single instance handover (process-level: second exited early, first survived and logged it)"
  fi

  step "3. Terminate the cold instance"
  note "- Command: kill -TERM $COLD_PID"
  note "- SIGTERM is not the tray 'Quit' menu item, so no in-flight-job cancellation message is expected here (interactive steps 7 and 12)"
  stop cold "$COLD_PID"

  step "4. Committed synthetic fixtures after the cold run"
  local cold_ok=0
  if store_snapshot cold "$COLD_DIR/quotavista-prototype"; then
    if validate_snapshot cold; then
      note "- the cold run committed the expected deterministic fixture"
    else
      note "- the cold store did not match the expected fixture"
      cold_ok=1
    fi
  else
    cold_ok=1
  fi
  if [ "$cold_ok" -eq 0 ]; then
    pass "cold run committed the expected synthetic fixture"
  else
    fail "cold run committed the expected synthetic fixture"
  fi

  step "5. Warm launch on the same throwaway state directory"
  note "- Command: env TMPDIR=$WARM_DIR QUOTAVISTA_SECRET_STORE=session-only ./$BIN"
  launch warm "$WARM_DIR"
  local WARM_PID="$launch_pid"
  if wait_ready warm "$PROBE_ROOT/warm.err" "$launch_start_ms"; then
    note "- time from exec to '${READY_MARKER}': ${ready_elapsed_ms} ms"
    note "- ${ready_line}"
    report_process warm "$launch_pid"
    report_diagnostics warm "$PROBE_ROOT/warm.err"
    if require_clean_diagnostics warm "$PROBE_ROOT/warm.err" 'system tray unavailable'; then
      note "- the warm run reported no seeding failure (a store that already holds connections is left untouched)"
      pass "warm launch reached composition readiness with clean diagnostics"
    else
      fail "warm launch reached composition readiness with clean diagnostics"
    fi
  else
    fail "warm launch reached composition readiness with clean diagnostics"
  fi

  step "6. Terminate the warm instance"
  stop warm "$WARM_PID"

  step "7. Restart kept the committed fixtures"
  local warm_ok=0
  local cold_row="$snap_row"
  if store_snapshot warm "$WARM_DIR/quotavista-prototype"; then
    note "- expected after a restart: both demo connections report ${EXPECTED_LIFECYCLES_RESTARTED% },"
    note "  because reconcile_connection_credentials (crates/infrastructure/src/secrets.rs:47)"
    note "  marks an Active connection with no retrievable credential as needing one, rather"
    note "  than leaving it Active or substituting a session-only credential silently"
    if validate_snapshot warm "$EXPECTED_LIFECYCLES_RESTARTED"; then
      if [ -n "$cold_row" ] && [ "$cold_row" = "$snap_row" ]; then
        note "- identical to the cold run: no duplicate seeding, same data revision, same settings"
      else
        note "- cold: ${cold_row:-<unavailable>}"
        note "- warm: ${snap_row}"
        note "- the aggregates differ even though both match the fixture; decide whether that is an expected refresh or a defect"
        warm_ok=1
      fi
    else
      note "- the warm store did not match the expected fixture"
      warm_ok=1
    fi
  else
    warm_ok=1
  fi
  if [ "$warm_ok" -eq 0 ]; then
    pass "restart kept the committed synthetic fixture"
  else
    fail "restart kept the committed synthetic fixture"
  fi
  not_observed "the renderer's own view of the restored data (interactive step 13)"

  step "8. No-tray launch: the override can only remove a capability"
  note "- Command: env TMPDIR=$NOTRAY_DIR QUOTAVISTA_SECRET_STORE=session-only QUOTAVISTA_TRAY=off ./$BIN"
  # The no-tray launch runs after the cold instance is gone, but the channel is
  # shared across launches, so re-check before starting another process.
  if ! qv_probe_socket_is_clear; then
    note "- the validation channel is still held; refusing the no-tray launch"
    fail "no-tray override reported the tray as unavailable"
  else
    launch notray "$NOTRAY_DIR" QUOTAVISTA_TRAY=off
    if wait_ready notray "$PROBE_ROOT/notray.err" "$launch_start_ms"; then
      note "- time from exec to '${READY_MARKER}': ${ready_elapsed_ms} ms"
      note "- ${ready_line}"
      report_diagnostics notray "$PROBE_ROOT/notray.err"
      local notray_ok=0
      case "$ready_line" in
        *tray=Unavailable*)
          note "- this exercises the forced-unavailable branch of the override; it is not the discovery of a genuinely missing tray"
          ;;
        *)
          note "- the override did not report tray=Unavailable"
          notray_ok=1
          ;;
      esac
      # Do not pass 'system tray unavailable' as an extra: that pattern is
      # fatal, and the no-tray launch is expected to report the tray missing.
      # Seeding and credential-store failures still fail the step.
      if ! require_clean_diagnostics notray "$PROBE_ROOT/notray.err"; then
        notray_ok=1
      fi
      stop notray "$launch_pid"
      # Fresh directory, so this is a first seed (active), not the warm restart
      # whose expected lifecycles are still in EXPECTED_LIFECYCLES.
      EXPECTED_LIFECYCLES="$EXPECTED_LIFECYCLES_SEEDED"
      # Read the store only after the writer has exited.
      if store_snapshot notray "$NOTRAY_DIR/quotavista-prototype"; then
        if validate_snapshot notray; then
          note "- the no-tray run committed the expected deterministic fixture"
        else
          note "- the no-tray store did not match the expected fixture"
          notray_ok=1
        fi
      else
        notray_ok=1
      fi
      if [ "$notray_ok" -eq 0 ]; then
        pass "no-tray override reported the tray as unavailable (forced branch)"
      else
        fail "no-tray override reported the tray as unavailable (forced branch)"
      fi
    else
      fail "no-tray override reported the tray as unavailable (forced branch)"
      stop notray "$launch_pid"
    fi
  fi
  not_observed "closing the window with no tray: whether the process exits is an interactive observation (interactive step 8)"

  step "9. What this probe did not observe"
  note "- Window rendering, tray icon visibility, notification banners, menu items,"
  note "  focus behaviour, real suspend/resume, and both UI locales all need a human"
  note "  at the desktop. Run: ./scripts/native-checks.sh --interactive"
  not_observed "16 interactive steps in scripts/native-checks.sh"

  printf '\n'
  if [ -e "$DEFAULT_DATA_DIR" ]; then
    note "- WARNING the default prototype directory exists after the run: ${DEFAULT_DATA_DIR}"
    note "- it was not created by this probe unless it also existed before; inspect it, do not delete it here"
  else
    note "- default prototype directory untouched: $DEFAULT_DATA_DIR"
  fi
  if [ "$KEEP" -eq 1 ]; then
    note "- --keep: probe root left at $PROBE_ROOT"
  else
    note "- removing the throwaway probe root $PROBE_ROOT (created by this run)"
    rm -rf "$PROBE_ROOT"
  fi

  if [ "$failures" -eq 0 ]; then
    printf 'PROBE: PASS\n'
    exit 0
  fi
  printf 'PROBE: FAIL (%d step(s))\n' "$failures"
  exit 1
}

# Sourcing defines the functions above and runs nothing, so the regression suite
# in scripts/test-native-launch-probe.sh can exercise the validator directly.
if [ "${BASH_SOURCE[0]}" = "$0" ]; then
  main "$@"
fi
