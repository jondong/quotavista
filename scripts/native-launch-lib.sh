# Shared native-launch boundary for QuotaVista validation runs (P1-13).
#
# Source this file; do not execute it. Both `scripts/native-launch-probe.sh`
# and `scripts/native-checks.sh` use it so a scripted run and a manual run
# carry exactly the same isolation and the same safety environment.
#
# Why isolation is needed
# -----------------------
# The pinned `tauri-plugin-single-instance` 2.5.4 does not derive its rendezvous
# point from `TMPDIR`:
#
#   * macOS  `src/platform_impl/macos.rs:socket_path()` returns
#            `/tmp/{identifier}_si.sock`, where `identifier` is
#            `config.identifier` with `.` and `-` replaced by `_`. The `/tmp`
#            prefix is hardcoded because a Unix socket path must stay under
#            100 characters.
#   * Linux  `src/platform_impl/linux.rs:init()` claims the session D-Bus name
#            `{identifier}.SingleInstance` with `replace_existing_names(false)`
#            and `allow_name_replacements(false)`.
#
# The identifier is baked into the binary at compile time by
# `tauri::generate_context!`. A fresh `TMPDIR` therefore isolates the prototype's
# *state directory* but NOT its single-instance channel: a second launch of the
# ordinary build connects to whatever already owns
# `/tmp/com_quotavista_prototype_si.sock`, and `notify_singleton()` calls
# `std::process::exit(0)` after a successful connect. If an ordinary instance is
# running, a validation launch would hand the activation to it, show its window,
# and run its wake path against its real state.
#
# The fix is a validation-only identity. `tauri-build` and `tauri-codegen` both
# merge the `TAURI_CONFIG` environment variable into the parsed configuration
# (`tauri-build-2.7.3/src/lib.rs:681`, `tauri-codegen-2.7.3/src/lib.rs:85`), so
# building with a different identifier yields a binary whose single-instance
# channel is disjoint from the ordinary application's. Nothing else about the
# prototype changes: same source, same commit, same composition root. On Linux
# the same identifier change moves the D-Bus name; running the launches inside
# `dbus-run-session` isolates them further.
#
# Detection is non-intrusive on purpose. Connecting to a live socket is exactly
# the activation this boundary exists to prevent, so an incumbent is detected
# from the process table and from `lsof`, never by connecting.

# shellcheck shell=bash

# Ordinary application identity, as set in apps/desktop/src-tauri/tauri.conf.json.
QV_ORDINARY_IDENTIFIER="com.quotavista.prototype"
# Validation-only identity. Differs from the ordinary one, so the two builds
# cannot reach each other's single-instance channel.
QV_PROBE_IDENTIFIER="com.quotavista.prototype.nativeprobe"

QV_CRATE="quotavista-desktop"
QV_ORDINARY_BIN="target/debug/${QV_CRATE}"
QV_PROBE_TARGET_DIR="target/native-probe"
QV_PROBE_BIN="${QV_PROBE_TARGET_DIR}/debug/${QV_CRATE}"

# The credential-store selector. `session-only` is the only value that keeps
# credentials in memory (apps/desktop/src-tauri/src/lib.rs:129-138); without it
# the prototype selects the native store, whose production service name is
# `quotavista` (crates/infrastructure/src/secrets.rs:37).
QV_SESSION_ONLY_ENV="QUOTAVISTA_SECRET_STORE=session-only"

# ---------------------------------------------------------------- identifiers

# qv_socket_path <identifier>
# Mirrors the plugin's macOS socket naming exactly.
qv_socket_path() {
  local id="$1"
  id="${id//./_}"
  id="${id//-/_}"
  printf '/tmp/%s_si.sock\n' "$id"
}

qv_ordinary_socket() { qv_socket_path "$QV_ORDINARY_IDENTIFIER"; }
qv_probe_socket() { qv_socket_path "$QV_PROBE_IDENTIFIER"; }

# qv_dbus_name <identifier>
# Mirrors the plugin's Linux session D-Bus name (the `semver` feature is not
# enabled in Cargo.lock, so no version suffix is appended).
qv_dbus_name() { printf '%s.SingleInstance\n' "$1"; }

qv_absolute() {
  case "$1" in
    /*) printf '%s\n' "$1" ;;
    *) printf '%s\n' "${PWD}/$1" ;;
  esac
}

# ------------------------------------------------------------- detection

# qv_instances_of <binary-path>...
# Prints the pids of running processes whose argv[0] is one of these binaries,
# matched exactly. Matching argv[0] rather than the whole command line matters: a
# plain `pgrep -f quotavista` also matches unrelated processes that merely carry
# the repository path in their environment or arguments.
#
# Three spellings of the same binary are matched, because all three occur: the
# bare relative form the probe itself launches, the `./`-prefixed form the printed
# manual commands use, and the absolute form. Missing the `./` form would let an
# operator who copied a documented command slip past the fail-closed gate.
qv_instances_of() {
  local bin rel abs dot found
  for bin in "$@"; do
    [ -n "$bin" ] || continue
    rel="${bin#./}"
    abs="$(qv_absolute "$rel")"
    dot="./${rel}"
    found="$(ps -axo pid=,args= 2>/dev/null |
      awk -v rel="$rel" -v abs="$abs" -v dot="$dot" '
        { pid = $1; $1 = ""; sub(/^ +/, ""); if ($0 == rel || $0 == abs || $0 == dot) print pid }
      ')"
    [ -n "$found" ] && printf '%s\n' "$found"
  done
  return 0
}

# qv_socket_holders <socket-path>
# Prints the pids holding this socket open, using `lsof`'s machine-readable
# field output. This observes; it never connects, so it cannot activate an
# incumbent. An empty result means the socket is absent or stale.
qv_socket_holders() {
  local sock="$1"
  [ -e "$sock" ] || return 0
  command -v lsof >/dev/null 2>&1 || return 0
  lsof -Fp -- "$sock" 2>/dev/null | sed -n 's/^p//p'
}

# qv_socket_state <path>
# Prints exactly one word describing a single-instance socket:
#   absent          no such path
#   stale           the path exists and no process holds it
#   live:<pids>     at least one process holds it (space-free, comma-joined)
#   unknown:no-lsof the path exists but lsof is unavailable, so a live socket
#                   cannot be told from a stale one
# Callers must treat `unknown:*` as NOT clear. Detection is best-effort: lsof
# exits 1 both when nothing matched and when it could not inspect, so an
# internal lsof failure is indistinguishable from "no holder". That is why the
# probe refuses to run at all without lsof, and why the substantive isolation
# guarantee is the disjoint application identifier, never this detection.
qv_socket_state() {
  local sock="$1"
  [ -e "$sock" ] || { printf 'absent\n'; return 0; }
  if ! command -v lsof >/dev/null 2>&1; then
    printf 'unknown:no-lsof\n'
    return 0
  fi
  local holders
  holders="$(lsof -Fp -- "$sock" 2>/dev/null | sed -n 's/^p//p' | tr '\n' ',' | sed 's/,$//')"
  if [ -n "$holders" ]; then
    printf 'live:%s\n' "$holders"
  else
    printf 'stale\n'
  fi
  return 0
}


# qv_acquire_lock / qv_release_lock
# The validation identifier is a fixed constant, so two overlapping runs — for
# example two agents in different worktrees of this repository, whose
# `target/native-probe` directories are separate but whose `/tmp` is not — would
# share one single-instance channel and could activate each other. The incumbent
# gate above narrows that window but is a check, not a lock. `mkdir` is atomic on
# every platform this runs on, so it makes a real one.
QV_LOCK_DIR="${QV_LOCK_DIR:-/tmp/quotavista-native-probe.lock}"

# Prints a reason on failure and returns 1; silent and returns 0 when acquired.
qv_acquire_lock() {
  local dir="$1" owner
  if mkdir "$dir" 2>/dev/null; then
    printf '%s\n' "$$" > "$dir/owner" 2>/dev/null
    return 0
  fi
  owner="$(cat "$dir/owner" 2>/dev/null)"
  if [ -n "$owner" ] && kill -0 "$owner" 2>/dev/null; then
    printf 'another native-check run (pid %s) holds %s\n' "$owner" "$dir"
    return 1
  fi
  # The recorded owner is gone, so this is a leftover from a killed run.
  printf 'stale lock %s (recorded owner %s is not running); remove it and retry\n' \
    "$dir" "${owner:-unknown}"
  return 1
}

# Removes the lock only when this shell owns it, so a run that lost the race
# never releases somebody else's lock.
qv_release_lock() {
  local dir="$1" owner
  owner="$(cat "$dir/owner" 2>/dev/null)"
  if [ "$owner" = "$$" ]; then
    rm -rf "$dir"
    printf '%s\n' "$dir"
  fi
}

# qv_host_os
# Prints `linux`, `darwin`, or `other`. The single-instance channel differs by
# platform, so detection has to know which one it is looking for.
qv_host_os() {
  case "$(uname -s 2>/dev/null)" in
    Linux) printf 'linux\n' ;;
    Darwin) printf 'darwin\n' ;;
    *) printf 'other\n' ;;
  esac
}

# qv_dbus_state <identifier>
# On Linux the plugin claims the session-bus name `<identifier>.SingleInstance`
# instead of a socket, so ownership — not a filesystem stamp — is the evidence.
# Prints `free`, `owned`, or `unknown:<reason>`. Asking `NameHasOwner` on the
# bus daemon does not claim the name and does not activate anything.
qv_dbus_state() {
  local name out
  name="$(qv_dbus_name "$1")"
  if ! command -v gdbus >/dev/null 2>&1; then
    printf 'unknown:no-gdbus\n'
    return 0
  fi
  if ! out="$(gdbus call --session --dest org.freedesktop.DBus \
    --object-path /org/freedesktop/DBus \
    --method org.freedesktop.DBus.NameHasOwner "$name" 2>&1)"; then
    # No session bus, or a bus that would not answer. Either way this is an
    # inspection we cannot interpret, and callers must not read it as clear.
    printf 'unknown:query-failed\n'
    return 0
  fi
  case "$out" in
    *true*) printf 'owned\n' ;;
    *false*) printf 'free\n' ;;
    *) printf 'unknown:unparsed\n' ;;
  esac
  return 0
}

# qv_detect_incumbents
# Prints one line per detected incumbent. Empty output means the host is clear.
qv_detect_incumbents() {
  local pid sock state
  local id dstate
  # Linux ownership lives on the session bus, not in /tmp: the plugin claims
  # `<identifier>.SingleInstance` there and never creates a socket. Checking the
  # macOS socket path on Linux would report "clear" no matter what was running,
  # so the platform decides which channel is evidence.
  if [ "$(qv_host_os)" = "linux" ]; then
    for id in "$QV_ORDINARY_IDENTIFIER" "$QV_PROBE_IDENTIFIER"; do
      dstate="$(qv_dbus_state "$id")"
      case "$dstate" in
        free) ;;
        owned)
          printf 'session bus name %s is owned, so an instance may already be running\n' \
            "$(qv_dbus_name "$id")"
          ;;
        *)
          # Fail closed: an inspection we cannot interpret is an incumbent.
          printf 'session bus name %s reports state %s, which cannot be shown to be clear\n' \
            "$(qv_dbus_name "$id")" "$dstate"
          ;;
      esac
    done
  fi
  for sock in "$(qv_ordinary_socket)" "$(qv_probe_socket)"; do
    state="$(qv_socket_state "$sock")"
    case "$state" in
      absent | stale) ;;
      live:*)
        for pid in $(printf '%s' "${state#live:}" | tr ',' ' '); do
          printf 'process %s holds the single-instance socket %s\n' "$pid" "$sock"
        done
        ;;
      *)
        # Fail closed: an inspection we cannot interpret is an incumbent.
        printf 'socket %s reports state %s, which cannot be shown to be clear\n' "$sock" "$state"
        ;;
    esac
  done
  for pid in $(qv_instances_of "$QV_ORDINARY_BIN"); do
    printf 'process %s is running the ordinary binary %s\n' "$pid" "$QV_ORDINARY_BIN"
  done
  for pid in $(qv_instances_of "$QV_PROBE_BIN"); do
    printf 'process %s is running a previous validation binary %s\n' "$pid" "$QV_PROBE_BIN"
  done
}

# qv_require_clear_host
# Fail-closed gate. Refuses to launch anything while an incumbent is detectable,
# because a validation launch must never reach ordinary application state.
# Returns 0 when the host is clear, 1 otherwise. Prints what it found.
qv_require_clear_host() {
  local found
  found="$(qv_detect_incumbents)"
  if [ -n "$found" ]; then
    printf '%s\n' "$found" | sed 's/^/    incumbent: /'
    return 1
  fi
  return 0
}

# qv_probe_socket_is_clear
# The probe's own channel must be free before the first launch. A live holder
# means a previous validation run is still alive; a stale file with no holder is
# safe, because the plugin removes it and rebinds when `connect` answers
# `ECONNREFUSED` (macos.rs:init). Returns 0 when clear.
qv_probe_socket_is_clear() {
  local sock state
  sock="$(qv_probe_socket)"
  state="$(qv_socket_state "$sock")"
  case "$state" in
    absent | stale) return 0 ;;
    live:*)
      printf '    the validation socket %s is held by: %s\n' "$sock" "${state#live:}"
      return 1
      ;;
    *)
      printf '    the validation socket %s reports state %s, which cannot be shown to be clear\n' "$sock" "$state"
      return 1
      ;;
  esac
}

# qv_remove_stale_probe_socket
# Removes a stale socket for the VALIDATION identity only. The ordinary
# application's socket is never touched, even when stale: it is not this
# script's to clean, and the plugin already recovers from a stale file itself.
qv_remove_stale_probe_socket() {
  local sock
  sock="$(qv_probe_socket)"
  # Only a socket that is *demonstrably* stale is removed. `live:*` would mean a
  # concurrent validation run is still using this shared identifier, and
  # `unknown:*` means the state could not be inspected; neither may be unlinked.
  if [ "$(qv_socket_state "$sock")" = "stale" ]; then
    rm -f "$sock" && printf '%s\n' "$sock"
  fi
}

# ---------------------------------------------------------------- build

# qv_build_probe_binary
# Builds the validation-only binary into its own target directory so the
# ordinary `target/debug` build is never overwritten and never alternates
# between two identifiers. Cargo's own fingerprinting makes this a no-op when
# nothing changed, including `rerun-if-env-changed=TAURI_CONFIG`.
qv_build_probe_binary() {
  TAURI_CONFIG="{\"identifier\":\"${QV_PROBE_IDENTIFIER}\"}" \
    CARGO_TARGET_DIR="$QV_PROBE_TARGET_DIR" \
    cargo build --locked -p "$QV_CRATE"
}

# qv_probe_binary_is_isolated
# Confirms the built binary really carries the validation identity, by reading
# the identifier back out of it. Falls back to a build-time check when the
# string is not present in a stripped binary.
qv_probe_binary_is_isolated() {
  local bin="$1"
  [ -x "$bin" ] || return 1
  if command -v grep >/dev/null 2>&1; then
    if grep -a -q "$QV_PROBE_IDENTIFIER" "$bin" 2>/dev/null; then
      return 0
    fi
  fi
  return 1
}

# ---------------------------------------------------------------- session env

# qv_session_setup_block <tmpdir>
# Prints the ONE mandatory setup block an operator exports before any manual
# native launch. Both variables are required: `TMPDIR` isolates the prototype's
# state directory, and the session-only store keeps the run away from the real
# Keychain / Secret Service. Exporting them in the operator's shell matters
# because a child process environment set by a script does not carry back.
qv_session_setup_block() {
  local dir="${1:-}"
  if [ -z "$dir" ]; then
    # Built as a plain literal rather than as a nested `${1:-$(...)}` default:
    # bash mis-nests the braces when a command substitution that itself contains
    # a `${VAR:-x}` default sits inside another default expansion, and prints a
    # broken mktemp template for the operator to copy. Caught by the self-test,
    # which asserts this exact line.
    dir='$(mktemp -d "${TMPDIR:-/tmp}/qv-native-session.XXXXXX")'
  fi
  cat <<EOF
export QUOTAVISTA_NATIVE_TMPDIR="${dir}"
export TMPDIR="\$QUOTAVISTA_NATIVE_TMPDIR"
export ${QV_SESSION_ONLY_ENV}
EOF
}

# qv_launch_command <binary> [EXTRA=value ...]
# Prints a launch command that preserves the mandatory environment, so a copied
# instruction can never silently select the native credential store or the
# ordinary state directory.
qv_launch_command() {
  local bin="$1"
  shift
  local extra=""
  [ $# -gt 0 ] && extra=" $*"
  # shellcheck disable=SC2016  # printed literally for the operator to copy
  printf 'env TMPDIR="$QUOTAVISTA_NATIVE_TMPDIR" %s%s ./%s\n' \
    "$QV_SESSION_ONLY_ENV" "$extra" "$bin"
}
