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
# This never touches the user's tmux: every case starts its own tmux server on its own socket
# (-L si-e2e-<pid>), so options and paste buffers set by one case never reach another. It reads no
# config (-f /dev/null) and sets escape-time to 0, so tmux itself never delays an Escape.
# Each case also gets its own HOME and XDG_STATE_HOME.
#
# Helpers (available inside a case):
#   start [args...]    run si in a detached 80x24 session, in the case's temp directory
#   keys KEY...        send tmux key names (Escape, Enter, C-c, ...)
#   typed TEXT         send TEXT as typed characters
#   screen             print the screen (still readable after si has exited)
#   wait_for TEXT [s]  wait until the screen contains TEXT (fixed string), 5 s by default
#   wait_exit [s]      wait until si has exited
#   exit_code          print the exit code of si (after wait_exit)
#   now_ms             print the time in milliseconds since the epoch
#   restart            end the case's tmux session, so that `start` can run si again (files stay)
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
  tm new-session -d -s "$SESSION" -x "$COLS" -y "$ROWS" "exec sh -c $(shq "$cmd")" \; set-option -w -t "$SESSION" remain-on-exit on \; set-option -sg escape-time 0
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

# GNU date has %N; elsewhere (macOS prints a literal N) perl is the portable way to get milliseconds.
now_ms() {
  t=$(date +%s%N)
  case $t in
    *N) perl -MTime::HiRes=time -e 'printf "%.0f\n", time * 1000' ;;
    *) echo $((t / 1000000)) ;;
  esac
}

# ---- running one case, in its own process so that set -e works inside it ----

run_one() {
  name=$1
  CASE_DIR=$(mktemp -d "${TMPDIR:-/tmp}/si-e2e.XXXXXX")
  SESSION=c$$
  SOCKET=$SOCKET-$$
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
        panes=$(tm list-panes -a -F "#{pane_pid}" 2> /dev/null || true)
        tm kill-server 2> /dev/null || true
        # si saves its unsaved text when tmux hangs it up (SAFE-6): let it finish before the files go
        for pid in $panes; do
          tries=100
          while pgrep -s "$pid" > /dev/null 2>&1 && [ "$tries" -gt 0 ]; do
            sleep 0.05
            tries=$((tries - 1))
          done
        done
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

  echo "e2e: $passed passed, $failed failed"
  [ "$failed" -eq 0 ]
}

# ---- cases: add new ones below this line, named <family>_<n>_<what>() ----

# Runs si under a wrapper that records si's pid, then prints its exit status and the tty modes
# (`stty -a`). Call it before `start`; extra arguments are VAR=value pairs for si's environment.
wrap() {
  REAL=$SI_BIN
  SI_BIN=$CASE_DIR/wrap.sh
  {
    echo '#!/bin/sh'
    for kv in "$@"; do echo "export $(shq "$kv")"; done
    echo "sh -c 'echo \$\$ > \"\$0\"; exec \"\$@\"' $(shq "$CASE_DIR/si.pid") $(shq "$REAL") \"\$@\""
    # tmux scrolls the first line away when the pane dies (remain-on-exit), so lead with a blank one.
    echo 'rc=$?; echo; echo "exit=$rc"'
    echo 'stty -a'
  } > "$SI_BIN"
  chmod +x "$SI_BIN"
}

restart() {
  tm kill-session -t "$SESSION"
  rm -f "$CASE_DIR/.exit_code"
}

# After si is gone: the normal screen is back and the tty is in cooked mode again.
terminal_restored() {
  wait_for icanon
  [ "$(tm display-message -p -t "$SESSION" '#{alternate_on}')" = 0 ]
  screen | grep -E -q '(^| )icanon( |$)'
  screen | grep -E -q '(^| )echo( |$)'
}

# git commit with si as the editor: ZZ makes the commit, :cq aborts it (the exit code reaches git).
cli_3_git_commit_with_si() {
  repo=$CASE_DIR/repo
  git init -q "$repo"
  printf 'one\n' > "$repo/a"
  git -C "$repo" add a
  cat > "$CASE_DIR/commit.sh" << EOF
#!/bin/sh
cd $(shq "$repo") || exit 1
export GIT_EDITOR=$(shq "$SI_BIN") GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_SYSTEM=/dev/null
export GIT_AUTHOR_NAME=si GIT_AUTHOR_EMAIL=si@example.com
export GIT_COMMITTER_NAME=si GIT_COMMITTER_EMAIL=si@example.com
git commit -q
rc=\$?
echo # tmux scrolls the first line away when the pane dies
echo "git=\$rc"
EOF
  chmod +x "$CASE_DIR/commit.sh"
  SI_BIN=$CASE_DIR/commit.sh
  start
  wait_for "COMMIT_EDITMSG  1:1  NORMAL"
  typed "ifeat: add a"
  keys Escape
  typed ZZ
  wait_for "git=0"
  [ "$(git -C "$repo" log --format=%s)" = "feat: add a" ]

  restart
  printf 'two\n' > "$repo/b"
  git -C "$repo" add b
  start
  wait_for "COMMIT_EDITMSG  1:1  NORMAL"
  typed "iwill not be used"
  keys Escape
  typed ":cq"
  keys Enter
  wait_for "git=1"
  [ "$(git -C "$repo" rev-list --count HEAD)" = 1 ]
  ! git -C "$repo" diff --cached --quiet # b is still staged: nothing was committed
}

cli_4_quit_restores_terminal() {
  wrap
  start f
  wait_for "f  1:1  NORMAL"
  typed ":q"
  keys Enter
  wait_for "exit=0"
  terminal_restored
}

cli_4_sigterm_restores_terminal() {
  wrap
  start f
  wait_for "f  1:1  NORMAL"
  kill -TERM "$(cat "$CASE_DIR/si.pid")"
  wait_for "exit=143"
  terminal_restored
}

cli_4_sighup_restores_terminal() {
  wrap
  start f
  wait_for "f  1:1  NORMAL"
  kill -HUP "$(cat "$CASE_DIR/si.pid")"
  wait_for "exit=129"
  terminal_restored
}

cli_4_panic_restores_terminal() {
  wrap SI_E2E_PANIC=1
  start f
  wait_for "SI_E2E_PANIC is set"
  wait_for "exit=101"
  terminal_restored
}

safe_1_edit_save_quit_writes_the_bytes() {
  printf 'one\ntwo\n' > "$CASE_DIR/f"
  start f
  wait_for "f  1:1  NORMAL"
  typed "jddothree"
  keys Escape
  wait_for "modified"
  typed ":wq"
  keys Enter
  wait_exit
  [ "$(exit_code)" = 0 ]
  printf 'one\nthree\n' | cmp - "$CASE_DIR/f"
}

# Starts si on the arguments, types into the second line, waits until a snapshot has surely been
# taken (a tick comes every second), and kills si with SIGKILL. Call `wrap` first.
edit_and_kill_9() {
  start "$@"
  wait_for "  1:1  NORMAL"
  typed "jcwTWO"
  keys Escape
  wait_for "modified"
  sleep 1.5
  kill -9 "$(cat "$CASE_DIR/si.pid")"
  wait_for "exit=137"
  restart
}

safe_6_recovers_after_kill_9() {
  printf 'one\ntwo\n' > "$CASE_DIR/f"
  wrap
  edit_and_kill_9 f
  start f
  wait_for "recovered unsaved changes from"
  wait_for "u to undo"
  screen | grep -F -q "  2 TWO"
  typed u
  wait_for "  2 two"
  gone modified
  printf 'one\ntwo\n' | cmp - "$CASE_DIR/f"
  typed ":q"
  keys Enter
  wait_for "exit=0"
  # quitting forgot the snapshot: nothing is left to offer next time
  [ -z "$(ls -A "$XDG_STATE_HOME/si")" ]
}

safe_6_sigterm_keeps_the_last_edit() {
  printf 'one\ntwo\n' > "$CASE_DIR/f"
  wrap
  start f
  wait_for "f  1:1  NORMAL"
  # Less than a second after the start, no tick has snapshotted this yet: only the snapshot
  # taken on the signal can hold it.
  typed "jcwTWO"
  keys Escape
  wait_for "modified"
  kill -TERM "$(cat "$CASE_DIR/si.pid")"
  wait_for "exit=143"
  restart
  start f
  wait_for "recovered unsaved changes from"
  screen | grep -F -q "  2 TWO"
}

safe_6_an_unnamed_buffer_is_recovered_too() {
  wrap
  start
  wait_for "[no name]  1:1  NORMAL"
  typed "ilost words"
  keys Escape
  wait_for "modified"
  sleep 1.5
  kill -9 "$(cat "$CASE_DIR/si.pid")"
  wait_for "exit=137"
  restart
  start
  wait_for "recovered unsaved changes from"
  screen | grep -F -q "  1 lost words"
}

safe_6_a_file_changed_meanwhile_keeps_the_snapshot_and_says_where() {
  printf 'one\ntwo\n' > "$CASE_DIR/f"
  wrap
  edit_and_kill_9 f
  printf 'changed elsewhere\n' > "$CASE_DIR/f"
  start f
  wait_for "kept unsaved changes from"
  wait_for "not applied"
  screen | grep -F -q "  1 changed elsewhere"
  gone modified
  [ "$(ls "$XDG_STATE_HOME/si" | wc -l)" -eq 1 ]
}

vi_8_yank_reaches_the_clipboard() {
  printf 'yank me please\nsecond\n' > "$CASE_DIR/f"
  start f
  wait_for "f  1:1  NORMAL"
  # tmux's default (external) drops OSC 52 from programs; with `on` it makes a paste buffer.
  tm set-option -sg set-clipboard on
  typed yy
  tries=100
  until [ "$(tm show-buffer 2> /dev/null)" = "yank me please" ]; do
    tries=$((tries - 1))
    [ "$tries" -gt 0 ] || { echo "tmux's buffer holds: $(tm show-buffer 2>&1)" >&2; return 1; }
    sleep 0.05
  done
}

# The screen no longer contains TEXT.
gone() { ! screen | grep -F -q -- "$1"; }

vi_9_esc_is_immediate() {
  start f
  wait_for "f  1:1  NORMAL"
  typed i
  wait_for INSERT
  keys Escape
  t0=$(now_ms)
  # No sleep: keep asking tmux for the screen until it says NORMAL.
  tries=400
  until screen | grep -F -q NORMAL; do
    tries=$((tries - 1))
    [ "$tries" -gt 0 ] || { echo "the status line never said NORMAL" >&2; return 1; }
  done
  t1=$(now_ms)
  # This is the time from sending Escape to seeing NORMAL, so it includes asking tmux for the
  # screen (a process start and a round trip, a few ms) and one `date`. A parser that waited
  # for more bytes after a lone Escape would need 100 ms or more (a timeout), so 50 ms is
  # generous slack for tmux and still tells the two apart.
  echo "Escape to NORMAL: $((t1 - t0)) ms"
  [ $((t1 - t0)) -le 50 ]
}

vi_9_bracketed_paste_is_one_undo() {
  printf 'base\n' > "$CASE_DIR/f"
  start f
  wait_for "f  1:1  NORMAL"
  # In Normal mode these lines are commands: dd and x delete, ZZ saves and quits. If the paste
  # were run as keys, the text would be gone or si would have exited.
  tm set-buffer -b paste 'dd one
x two
ZZ end'
  tm paste-buffer -p -d -b paste -t "$SESSION"
  wait_for "ZZ endbase"
  wait_for "modified"
  screen | grep -F -q "dd one"
  screen | grep -F -q "x two"
  [ ! -s "$CASE_DIR/.exit_code" ]
  typed u
  wait_for "  1 base"
  gone "dd one"
  gone "x two"
  gone "ZZ end"
  gone modified
}

vi_13_zz_saves_and_quits() {
  printf 'one\n' > "$CASE_DIR/f"
  start f
  wait_for "f  1:1  NORMAL"
  typed "A two"
  keys Escape
  wait_for "modified"
  typed ZZ
  wait_exit
  [ "$(exit_code)" = 0 ]
  printf 'one two\n' | cmp - "$CASE_DIR/f"
}

main "$@"
