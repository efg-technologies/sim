#!/bin/sh
# End-to-end tests: run si inside tmux and look at the screen.
#
#   sh scripts/e2e.sh [prefix]
#
# Runs every case, or only the cases whose name starts with the prefix (e.g. `safe_1`).
# A case is a shell function named like an ID in SPEC, `<family>_<n>_<what>()`
# (e.g. `vi_1_word_motions()`); scripts/trace.sh counts them as tests for that ID.
# Cases use the helpers below, and any failing command fails the case (set -e).
#
# This never touches the user's tmux: it has its own socket (-L si-e2e) and reads no
# config (-f /dev/null). Each case also gets its own HOME and XDG_STATE_HOME.
#
# Helpers (available inside a case):
#   start [args...]    run si in a detached 80x24 session, in the case's temp directory
#   keys KEY...        send tmux key names (Escape, Enter, C-c, ...)
#   typed TEXT         send TEXT as typed characters
#   screen             print the screen (still readable after si has exited)
#   wait_for TEXT [s]  wait until the screen contains TEXT (fixed string), 5 s by default
#   wait_exit [s]      wait until si has exited
#   exit_code          print the exit code of si (after wait_exit)
#   $CASE_DIR          the case's temp directory (cwd of si); $HOME and $XDG_STATE_HOME are inside it
set -eu

ROOT=$(cd "$(dirname "$0")/.." && pwd)
SELF=$ROOT/scripts/e2e.sh
SOCKET=si-e2e
COLS=80
ROWS=24

# Never inherit the user's tmux session: only this socket is used.
unset TMUX || true
tm() { tmux -u -L "$SOCKET" -f /dev/null "$@"; }

# Single-quote a word for the shell that tmux runs.
shq() { printf "'%s'" "$(printf '%s' "$1" | sed "s/'/'\\\\''/g")"; }

# ---- helpers used by cases ----

start() {
  cmd="cd $(shq "$CASE_DIR") && env HOME=$(shq "$HOME") XDG_STATE_HOME=$(shq "$XDG_STATE_HOME") $(shq "$SI_BIN")"
  for arg in "$@"; do cmd="$cmd $(shq "$arg")"; done
  cmd="$cmd; echo \$? > $(shq "$CASE_DIR/.exit_code")"
  # sh -c, so that the user's login shell doesn't matter; remain-on-exit keeps the screen readable after si exits.
  tm new-session -d -s "$SESSION" -x "$COLS" -y "$ROWS" "exec sh -c $(shq "$cmd")" \; set-option -w -t "$SESSION" remain-on-exit on
}

keys() { tm send-keys -t "$SESSION" "$@"; }
typed() { tm send-keys -t "$SESSION" -l -- "$1"; }
screen() { tm capture-pane -p -t "$SESSION"; }

wait_for() {
  tries=$((${2:-5} * 20))
  while [ "$tries" -gt 0 ]; do
    if screen 2> /dev/null | grep -F -q -- "$1"; then return 0; fi
    sleep 0.05
    tries=$((tries - 1))
  done
  echo "timed out waiting for the screen to contain: $1" >&2
  return 1
}

wait_exit() {
  tries=$((${1:-5} * 20))
  while [ "$tries" -gt 0 ]; do
    if [ -s "$CASE_DIR/.exit_code" ]; then return 0; fi
    sleep 0.05
    tries=$((tries - 1))
  done
  echo "timed out waiting for si to exit" >&2
  return 1
}

exit_code() { cat "$CASE_DIR/.exit_code"; }

# ---- running one case, in its own process so that set -e works inside it ----

run_one() {
  name=$1
  CASE_DIR=$(mktemp -d "${TMPDIR:-/tmp}/si-e2e.XXXXXX")
  SESSION=c$$
  HOME=$CASE_DIR/home
  XDG_STATE_HOME=$CASE_DIR/state
  mkdir -p "$HOME" "$XDG_STATE_HOME"
  export CASE_DIR SESSION HOME XDG_STATE_HOME
  trap 'status=$?
        if [ "$status" -ne 0 ] && tm has-session -t "$SESSION" 2> /dev/null; then
          echo "---- screen at failure ----"
          screen
          echo "---------------------------"
        fi
        tm kill-session -t "$SESSION" 2> /dev/null || true
        rm -rf "$CASE_DIR"
        exit "$status"' EXIT
  trap 'exit 130' HUP INT TERM
  "$name"
}

cases() {
  grep -E '^[a-z]+_[0-9]+_[a-z0-9_]*\(\)' "$SELF" | sed 's/().*//'
}

# Build, then run the cases one by one. Called on the last line, after all cases are defined.
main() {
  if [ "${1:-}" = "--run-one" ]; then
    run_one "$2"
    exit 0
  fi

  filter=${1:-}
  selected=
  for name in $(cases); do
    case $name in
      "$filter"*) selected="$selected $name" ;;
    esac
  done

  cd "$ROOT"
  cargo build --quiet
  SI_BIN=${CARGO_TARGET_DIR:-$ROOT/target}/debug/si
  export SI_BIN

  if [ -z "$selected" ]; then
    if [ -n "$filter" ]; then
      echo "e2e: no case starts with '$filter'" >&2
      exit 1
    fi
    echo "e2e: 0 cases, 0 failed"
    exit 0
  fi

  command -v tmux > /dev/null || {
    echo "e2e: tmux not found" >&2
    exit 1
  }

  passed=0
  failed=0
  for name in $selected; do
    log=$(mktemp "${TMPDIR:-/tmp}/si-e2e-log.XXXXXX")
    if sh "$SELF" --run-one "$name" > "$log" 2>&1; then
      echo "ok    $name"
      passed=$((passed + 1))
    else
      echo "FAIL  $name"
      sed 's/^/      /' "$log"
      failed=$((failed + 1))
    fi
    rm -f "$log"
  done
  tm kill-server 2> /dev/null || true

  echo "e2e: $passed passed, $failed failed"
  [ "$failed" -eq 0 ]
}

# ---- cases: add new ones below this line, named <family>_<n>_<what>() ----

main "$@"
