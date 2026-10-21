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
#     committed synthetic data survived a restart.
#   * It does NOT prove a human can see the window, the tray icon, or a
#     notification banner. Those remain interactive steps in
#     `scripts/native-checks.sh --interactive`, and this script prints them as
#     NOT OBSERVED rather than guessing.
#
# Safety:
#   * The prototype's own state directory is `std::env::temp_dir()` +
#     `quotavista-prototype`. Every launch below overrides `TMPDIR` with a fresh
#     throwaway directory created by this script, so the default prototype
#     directory is never created, read, modified, or deleted.
#   * Every launch sets `QUOTAVISTA_SECRET_STORE=session-only`. The probe
#     therefore never reads, writes, or deletes an entry in the real macOS
#     Keychain or the Linux Secret Service, and never risks a credential-store
#     access prompt that would need a human to dismiss. Real native
#     credential-store evidence comes from the separately gated smoke test
#     `macos_keychain_synthetic_roundtrip`, which uses a disposable
#     `quotavista-test-` service.
#   * All data is synthetic. No provider is contacted; the seeded demo
#     connections use the synthetic providers compiled into the prototype.
#
# Usage: scripts/native-launch-probe.sh [--keep]
#   --keep   leave the probe root on disk and print its path (for inspection).

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT" || exit 2

CRATE="quotavista-desktop"
BIN="target/debug/${CRATE}"
READY_MARKER="quotavista: capabilities"
READY_TIMEOUT_SECONDS=90
QUIT_TIMEOUT_SECONDS=15

KEEP=0
case "${1:-}" in
  --keep) KEEP=1 ;;
  "") ;;
  *)
    echo "usage: $0 [--keep]" >&2
    exit 2
    ;;
esac

PROBE_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/qv-native-probe.XXXXXX")" || exit 2
DEFAULT_DATA_DIR="${TMPDIR:-/tmp}/quotavista-prototype"

failures=0
pids=()

now_ms() {
  if command -v python3 >/dev/null 2>&1; then
    python3 -c 'import time; print(int(time.time() * 1000))'
  else
    echo $(( $(date +%s) * 1000 ))
  fi
}

note() {
  printf '%s\n' "$*"
}

# Print a step header, then the exact command an operator would type.
step() {
  printf '\n### %s\n' "$1"
}

pass() {
  printf 'RESULT: PASS (%s)\n' "$1"
}

fail() {
  printf 'RESULT: FAIL (%s)\n' "$1"
  failures=$((failures + 1))
}

# A check this probe deliberately cannot make. Never printed as a pass.
not_observed() {
  printf 'NOT OBSERVED: %s\n' "$1"
}

# launch <label> <tmpdir> [EXTRA=value ...]
# Starts the prototype in the background under an isolated TMPDIR and records
# its pid in $launch_pid.
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
  printf -- '- launched pid=%s tmpdir=%s%s\n' "$launch_pid" "$dir" \
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
# so the graceful quit stays an interactive observation.
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

# inspect_store <label> <datadir>
# Reads the prototype's SQLite state read-only, after the writer has exited, and
# prints the row counts that decide whether committed data survived.
inspect_store() {
  local label="$1"
  local data="$2"
  local db="$data/quotavista.sqlite"
  if ! command -v sqlite3 >/dev/null 2>&1; then
    not_observed "${label}: sqlite3 is not installed, so committed state was not inspected"
    return 1
  fi
  if [ ! -f "$db" ]; then
    note "- ${label}: no database at $db"
    return 1
  fi
  local query='SELECT (SELECT data_revision FROM store_state)
    || "|" || (SELECT COUNT(*) FROM connections)
    || "|" || (SELECT COUNT(*) FROM provider_scopes)
    || "|" || (SELECT COUNT(*) FROM usage_facts)
    || "|" || (SELECT COUNT(*) FROM published_partitions)
    || "|" || (SELECT COUNT(*) FROM sync_runs)
    || "|" || (SELECT locale || "/" || reporting_zone || "/" || currency FROM settings);'
  local row
  if ! row="$(sqlite3 "file:${db}?mode=ro" "$query" 2>&1)"; then
    note "- ${label}: sqlite3 could not read the store: ${row}"
    return 1
  fi
  note "- ${label} store_state.data_revision | connections | provider_scopes | usage_facts | published_partitions | sync_runs | settings(locale/zone/currency):"
  note "    ${row}"
  inspect_row="$row"
  return 0
}

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

cleanup() {
  local leftover=0
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
  strays="$(pgrep -f "target/debug/${CRATE}" 2>/dev/null | tr '\n' ' ')"
  if [ -n "${strays// /}" ]; then
    note "- cleanup: WARNING probe-named processes still running: ${strays}(inspect before assuming a clean host)"
  else
    note "- cleanup: no ${CRATE} process remains"
  fi
}

trap cleanup EXIT

printf '## Native launch probe (non-interactive)\n'
printf -- '- Repository: %s\n' "$REPO_ROOT"
printf -- '- Commit: %s\n' "$(git rev-parse HEAD 2>/dev/null || echo unknown)"
printf -- '- Date (UTC): %s\n' "$(date -u '+%Y-%m-%d %H:%M:%S')"
printf -- '- Binary: %s\n' "$BIN"
printf -- '- Probe root (throwaway, created by this run): %s\n' "$PROBE_ROOT"
printf -- '- Credential store for every launch: QUOTAVISTA_SECRET_STORE=session-only\n'
printf -- '- Real credential store entries touched: none\n'
printf -- "- Default prototype directory left alone: %s\n" "$DEFAULT_DATA_DIR"

step "Preflight: the probe runs a built binary, not a rebuild"
if [ -x "$BIN" ]; then
  note "- $BIN exists and is executable"
  note "- Build it with: cargo build --locked -p ${CRATE}"
  pass "binary present"
else
  note "- $BIN is missing"
  note "- Build it with: cargo build --locked -p ${CRATE}"
  fail "binary present"
  printf '\nPROBE: FAIL\n'
  exit 1
fi

COLD_DIR="$PROBE_ROOT/cold"
WARM_DIR="$PROBE_ROOT/warm"
NOTRAY_DIR="$PROBE_ROOT/notray"
mkdir -p "$COLD_DIR" "$WARM_DIR" "$NOTRAY_DIR"
# A warm launch must reuse the cold launch's state to be a warm launch at all.
WARM_DIR="$COLD_DIR"

step "1. Cold launch on an empty throwaway state directory"
note "- Command: env TMPDIR=$COLD_DIR QUOTAVISTA_SECRET_STORE=session-only ./$BIN"
launch cold "$COLD_DIR"
if wait_ready cold "$PROBE_ROOT/cold.err" "$launch_start_ms"; then
  note "- time from exec to '${READY_MARKER}': ${ready_elapsed_ms} ms"
  note "- ${ready_line}"
  report_process cold "$launch_pid"
  report_data_dir cold "$COLD_DIR/quotavista-prototype"
  report_diagnostics cold "$PROBE_ROOT/cold.err"
  not_observed "window content: the probe cannot see whether a human sees the window, the four screens, or the tray icon (interactive steps 1, 2, 5)"
  pass "cold launch reached composition readiness"
else
  fail "cold launch reached composition readiness"
fi
COLD_PID="$launch_pid"

step "2. Single instance: a second launch hands over to the first"
note "- Command: env TMPDIR=$COLD_DIR QUOTAVISTA_SECRET_STORE=session-only ./$BIN   (while pid $COLD_PID is still running)"
if kill -0 "$COLD_PID" 2>/dev/null; then
  launch second "$COLD_DIR"
  second_start="$launch_start_ms"
  second_status=0
  wait "$launch_pid" 2>/dev/null || second_status=$?
  second_elapsed=$(( $(now_ms) - second_start ))
  note "- second launch exited with status ${second_status} after ${second_elapsed} ms"
  if kill -0 "$COLD_PID" 2>/dev/null; then
    note "- first instance (pid ${COLD_PID}) is still running"
    report_diagnostics first-instance "$PROBE_ROOT/cold.err"
    if grep -q 'SingleInstanceActivation' "$PROBE_ROOT/cold.err" 2>/dev/null; then
      note "- the first instance logged the handover"
      pass "single instance handover"
    else
      not_observed "handover wake line: the first instance did not print SingleInstanceActivation, so only the second launch's early exit was observed"
      fail "single instance handover"
    fi
  else
    note "- first instance (pid ${COLD_PID}) is NOT running any more"
    fail "single instance handover"
  fi
  not_observed "focus: the probe cannot see whether the first window was shown and focused (interactive step 9)"
else
  note "- the cold instance had already exited; the single-instance check was not run"
  fail "single instance handover"
fi

step "3. Terminate the cold instance"
note "- Command: kill -TERM $COLD_PID"
note "- SIGTERM is not the tray 'Quit' menu item, so no in-flight-job cancellation message is expected here (interactive steps 7 and 12)"
stop cold "$COLD_PID"

step "4. Committed synthetic state after the cold run"
cold_row=""
if inspect_store cold "$COLD_DIR/quotavista-prototype"; then
  cold_row="$inspect_row"
  pass "cold run committed synthetic state"
else
  fail "cold run committed synthetic state"
fi

step "5. Warm launch on the same throwaway state directory"
note "- Command: env TMPDIR=$WARM_DIR QUOTAVISTA_SECRET_STORE=session-only ./$BIN"
launch warm "$WARM_DIR"
if wait_ready warm "$PROBE_ROOT/warm.err" "$launch_start_ms"; then
  note "- time from exec to '${READY_MARKER}': ${ready_elapsed_ms} ms"
  note "- ${ready_line}"
  report_process warm "$launch_pid"
  report_diagnostics warm "$PROBE_ROOT/warm.err"
  if grep -q 'demo seeding failed' "$PROBE_ROOT/warm.err" 2>/dev/null; then
    note "- the warm run reported a seeding failure"
  else
    note "- the warm run reported no seeding failure (a store that already holds connections is left untouched)"
  fi
  pass "warm launch reached composition readiness"
else
  fail "warm launch reached composition readiness"
fi
WARM_PID="$launch_pid"

step "6. Terminate the warm instance"
stop warm "$WARM_PID"

step "7. Restart kept the committed data"
warm_row=""
if inspect_store warm "$WARM_DIR/quotavista-prototype"; then
  warm_row="$inspect_row"
  if [ -n "$cold_row" ] && [ "$cold_row" = "$warm_row" ]; then
    note "- identical to the cold run: no duplicate seeding, same data revision, same settings"
    pass "restart kept committed data"
  else
    note "- cold: ${cold_row:-<unavailable>}"
    note "- warm: ${warm_row:-<unavailable>}"
    note "- the rows differ; decide whether that is an expected refresh or a defect"
    fail "restart kept committed data"
  fi
else
  fail "restart kept committed data"
fi
not_observed "the renderer's own view of the restored data (interactive step 13)"

step "8. No-tray launch: the override can only remove a capability"
note "- Command: env TMPDIR=$NOTRAY_DIR QUOTAVISTA_SECRET_STORE=session-only QUOTAVISTA_TRAY=off ./$BIN"
launch notray "$NOTRAY_DIR" QUOTAVISTA_TRAY=off
if wait_ready notray "$PROBE_ROOT/notray.err" "$launch_start_ms"; then
  note "- time from exec to '${READY_MARKER}': ${ready_elapsed_ms} ms"
  note "- ${ready_line}"
  report_diagnostics notray "$PROBE_ROOT/notray.err"
  case "$ready_line" in
    *tray=Unavailable*)
      pass "no-tray override reported the tray as unavailable"
      ;;
    *)
      note "- the override did not report tray=Unavailable"
      fail "no-tray override reported the tray as unavailable"
      ;;
  esac
  NOTRAY_PID="$launch_pid"
  stop notray "$NOTRAY_PID"
else
  fail "no-tray override reported the tray as unavailable"
  stop notray "$launch_pid"
fi
not_observed "closing the window with no tray: whether the process exits is an interactive observation (interactive step 8)"

step "9. What this probe did not observe"
note "- Window rendering, tray icon visibility, notification banners, menu items,"
note "  focus behaviour, real suspend/resume, and both UI locales all need a human"
note "  at the desktop. Run: ./scripts/native-checks.sh --interactive"
not_observed "16 interactive steps in scripts/native-checks.sh"

printf '\n'
if [ "$KEEP" -eq 1 ]; then
  note "- --keep: probe root left at $PROBE_ROOT"
else
  note "- removing the throwaway probe root $PROBE_ROOT (created by this run)"
  rm -rf "$PROBE_ROOT"
  note "- default prototype directory untouched: $DEFAULT_DATA_DIR"
fi

if [ "$failures" -eq 0 ]; then
  printf 'PROBE: PASS\n'
  exit 0
fi
printf 'PROBE: FAIL (%d step(s))\n' "$failures"
exit 1
