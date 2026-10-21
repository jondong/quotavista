#!/usr/bin/env bash
# Reproducible native desktop-lifecycle checks for QuotaVista.
#
# P1-11 implements the lifecycle behaviour (tray detection, close/reopen/quit,
# single instance, wake, synthetic notification). This script is how a human
# records what the real desktop actually did, on any target. It is the procedure
# P1-13 runs on macOS and on Ubuntu 26.04 GNOME x86_64.
#
# Nothing here observes on your behalf. Every interactive step prints what to
# look for and then asks the operator to type what happened, so the transcript
# this script produces can be pasted into a task's validation.md as evidence.
# A step that is not run is printed as PENDING, never as passed.
#
# Usage:
#   scripts/native-checks.sh --environment   print the environment block
#   scripts/native-checks.sh --automated     run the checks that need no display
#   scripts/native-checks.sh --interactive   drive the window/tray/notification steps
#   scripts/native-checks.sh --all           all three, in that order
#
# Safety: the prototype reads and writes only the throwaway directory
# $TMPDIR/quotavista-prototype, uses synthetic providers, and never touches a
# real credential store entry. The synthetic notification carries no account
# data. Set QUOTAVISTA_SECRET_STORE=session-only to keep credentials in memory.

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT" || exit 1

CRATE="quotavista-desktop"
DATA_DIR="${TMPDIR:-/tmp}/quotavista-prototype"

# ---------------------------------------------------------------- environment

print_environment() {
  echo "## Environment"
  echo
  echo "- Date (UTC): $(date -u '+%Y-%m-%d %H:%M:%S')"
  echo "- Kernel: $(uname -srm)"
  case "$(uname -s)" in
    Darwin)
      echo "- macOS: $(sw_vers -productName 2>/dev/null) $(sw_vers -productVersion 2>/dev/null) ($(sw_vers -buildVersion 2>/dev/null))"
      echo "- Architecture: $(uname -m)"
      echo "- Session type: n/a (Aqua)"
      ;;
    Linux)
      echo "- Distribution: $(. /etc/os-release 2>/dev/null && echo "${PRETTY_NAME:-unknown}")"
      echo "- Architecture: $(uname -m)"
      # The session type is part of the recorded evidence: Wayland and X11 do
      # not behave the same for tray icons.
      echo "- Session type: ${XDG_SESSION_TYPE:-unknown} (XDG_CURRENT_DESKTOP=${XDG_CURRENT_DESKTOP:-unknown})"
      echo "- Desktop: ${XDG_CURRENT_DESKTOP:-unknown}"
      if command -v gnome-shell >/dev/null 2>&1; then
        echo "- GNOME Shell: $(gnome-shell --version 2>/dev/null)"
      fi
      # An appindicator/StatusNotifier host is what makes a Linux tray icon
      # visible. Its absence is why the prototype reports the tray as
      # unavailable on Linux rather than trusting a successful registration.
      if command -v gdbus >/dev/null 2>&1; then
        if gdbus call --session --dest org.kde.StatusNotifierWatcher \
             --object-path /StatusNotifierWatcher \
             --method org.freedesktop.DBus.Properties.Get \
             org.kde.StatusNotifierWatcher IsStatusNotifierHostRegistered \
             >/dev/null 2>&1; then
          echo "- StatusNotifier host: registered"
        else
          echo "- StatusNotifier host: not reachable (tray icons will not be visible)"
        fi
      fi
      ;;
    *)
      echo "- Platform: $(uname -s) (not a supported native target)"
      ;;
  esac
  echo "- Rust: $(rustc --version 2>/dev/null || echo 'not installed')"
  echo "- Cargo: $(cargo --version 2>/dev/null || echo 'not installed')"
  echo "- Node: $(node --version 2>/dev/null || echo 'not installed')"
  echo "- npm: $(npm --version 2>/dev/null || echo 'not installed')"
  echo "- Tauri (locked): $(grep -A1 '^name = "tauri"$' Cargo.lock | grep version | head -1 | sed 's/.*"\(.*\)".*/\1/')"
  echo "- Prototype data directory: ${DATA_DIR} (throwaway)"
}

# ------------------------------------------------------------------ automated

# Run one labelled command and report PASS or FAIL from its real exit status, so
# a launcher that never started cannot be mistaken for a green run.
run_step() {
  local label="$1"
  shift
  echo
  echo "### ${label}"
  echo "\$ $*"
  if "$@"; then
    local rc=$?
    echo "RESULT: PASS (${label})"
    return "$rc"
  else
    local rc=$?
    echo "RESULT: FAIL (${label}, exit ${rc})"
    return "$rc"
  fi
}

run_automated() {
  echo
  echo "## Automated checks (no display required)"
  local failed=0

  run_step "lifecycle policy: close, wake, Off mode, quit" \
    cargo test --locked -p "$CRATE" lifecycle:: || failed=1
  run_step "platform capability adapters: tray and notification detection" \
    cargo test --locked -p "$CRATE" platform:: || failed=1
  run_step "IPC boundary still sealed" \
    cargo test --locked -p "$CRATE" the_generated_handler || failed=1

  echo
  if [ "$failed" -eq 0 ]; then
    echo "AUTOMATED: PASS"
  else
    echo "AUTOMATED: FAIL"
  fi
  return "$failed"
}

# ---------------------------------------------------------------- interactive

# Ask the operator what they observed. Without a terminal the step is recorded
# as PENDING rather than guessed.
observe() {
  local step="$1"
  local hint="$2"
  echo
  echo "### ${step}"
  echo "Look for: ${hint}"
  if [ ! -t 0 ]; then
    echo "OBSERVED: PENDING (no interactive terminal; run this step manually)"
    return 0
  fi
  printf 'OBSERVED (type what happened, or "skip"): '
  local answer
  read -r answer
  if [ -z "$answer" ] || [ "$answer" = "skip" ]; then
    echo "OBSERVED: PENDING (operator skipped)"
  else
    echo "OBSERVED: ${answer}"
  fi
}

run_interactive() {
  echo
  echo "## Interactive checks (require a desktop session)"
  echo
  echo "Build first: cargo build --locked -p ${CRATE}"
  echo "Run with:    cargo run --locked -p ${CRATE}"
  echo "No tray:     QUOTAVISTA_TRAY=off cargo run --locked -p ${CRATE}"

  observe "1. Cold launch" \
    "the main window opens and the four screens are reachable from it"

  observe "2. Capability detection (in-app)" \
    "press 'Check desktop capabilities' in the Desktop integration region and read the System tray and System notifications rows"

  observe "3. Synthetic notification, English" \
    "press 'Send a test notification'; note whether an OS banner appeared AND what the in-app verdict says ('accepted' vs 'stays here')"

  observe "4. Synthetic notification, Simplified Chinese" \
    "switch the interface language to 简体中文, press 'Send a test notification' again, and confirm the banner text is Chinese and carries no account data"

  observe "5. Close with a tray present" \
    "with the tray reported available, close the main window: the window hides, the process keeps running, and the menu-bar/tray icon is still there"

  observe "6. Reopen from the tray" \
    "choose 'Show QuotaVista' from the tray menu: the window reappears and takes focus"

  observe "7. Explicit quit from the tray" \
    "choose 'Quit' from the tray menu: the process exits (check with pgrep -f ${CRATE})"

  observe "8. No-tray close exits" \
    "relaunch with QUOTAVISTA_TRAY=off, confirm the region reports the tray as unavailable, then close the main window: the process exits and no icon or collector is left behind"

  observe "9. Single instance" \
    "with the application running, launch a second copy: the second process exits immediately and the first window is shown and focused"

  observe "10. Wake merges refresh work" \
    "enable automatic refresh in settings, hide or defocus the window, then bring it back and watch stderr for a 'quotavista: wake ...' line reporting merged and not-due counts"

  observe "11. Refresh Off starts nothing" \
    "disable automatic refresh, repeat step 10: the wake line reports mode Off and permitted false, and no provider query is issued"

  observe "12. Quit cancels in-flight work" \
    "start a manual refresh and choose Quit immediately: stderr reports 'quotavista: quit cancelled N in-flight job(s)' and the process exits"

  observe "13. Restart keeps committed data" \
    "after a refresh that committed, quit and relaunch: the connections and the last data revision are still there"
}

# ----------------------------------------------------------------------- main

case "${1:-}" in
  --environment) print_environment ;;
  --automated) run_automated ;;
  --interactive) print_environment; run_interactive ;;
  --all)
    print_environment
    run_automated
    automated_rc=$?
    run_interactive
    exit "$automated_rc"
    ;;
  *)
    echo "usage: $0 {--environment|--automated|--interactive|--all}" >&2
    exit 2
    ;;
esac
