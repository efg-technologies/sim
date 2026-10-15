#!/bin/sh
# End-to-end tests: run sim inside tmux and look at the screen. This is si's scripts/e2e.sh for sim.
#
#   sh sim/scripts/e2e.sh [prefix]
#
# Runs every case, or only the cases whose name starts with the prefix (e.g. `tree_1`).
# A case is a shell function named like an ID in sim's SPEC, `<family>_<n>_<what>()`
# (e.g. `tree_1_starts_with_the_tree()`); sim/scripts/trace.sh counts them as tests for that ID.
# Cases use the helpers below, and any failing command fails the case (set -e).
#
# This never touches the user's tmux: every case starts its own tmux server on its own socket
# (-L sim-e2e-<pid>), so options and paste buffers set by one case never reach another. It reads no
# config (-f /dev/null) and sets escape-time to 0, so tmux itself never delays an Escape.
# Each case also gets its own HOME and XDG_STATE_HOME.
#
# Helpers (available inside a case):
#   start [args...]    run sim in a detached 80x24 session, in the case's temp directory
#   keys KEY...        send tmux key names (Escape, Enter, C-c, ...)
#   typed TEXT         send TEXT as typed characters
#   screen             print the screen (still readable after sim has exited)
#   wait_for TEXT [s]  wait until the screen contains TEXT (fixed string), 5 s by default
#   wait_exit [s]      wait until sim has exited
#   exit_code          print the exit code of sim (after wait_exit)
#   now_ms             print the time in milliseconds since the epoch
#   restart            end the case's tmux session, so that `start` can run sim again (files stay)
#   $CASE_DIR          the case's temp directory; $HOME and $XDG_STATE_HOME are inside it
#   $W                 sim's working directory, $CASE_DIR/work: the project a case makes
set -eu

HERE=$(cd "$(dirname "$0")/.." && pwd)
ROOT=$(cd "$HERE/.." && pwd)
SELF=$HERE/scripts/e2e.sh
SOCKET=sim-e2e
COLS=80
ROWS=24

# Never inherit the user's tmux session: only this socket is used.
unset TMUX || true
tm() { tmux -u -L "$SOCKET" -f /dev/null "$@"; }

# Single-quote a word for the shell that tmux runs.
shq() { printf "'%s'" "$(printf '%s' "$1" | sed "s/'/'\\\\''/g")"; }

# ---- helpers used by cases ----

start() {
  cmd="cd $(shq "$W") && env HOME=$(shq "$HOME") XDG_STATE_HOME=$(shq "$XDG_STATE_HOME") $(shq "$SIM_BIN")"
  for arg in "$@"; do cmd="$cmd $(shq "$arg")"; done
  cmd="$cmd; echo \$? > $(shq "$CASE_DIR/.exit_code")"
  # sh -c, so that the user's login shell doesn't matter; remain-on-exit keeps the screen readable after sim exits.
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

# The screen stops containing TEXT.
wait_gone() {
  tries=$((${2:-5} * 20))
  while [ "$tries" -gt 0 ]; do
    if ! screen 2> /dev/null | grep -F -q -- "$1"; then return 0; fi
    sleep 0.05
    tries=$((tries - 1))
  done
  echo "timed out waiting for the screen to lose: $1" >&2
  return 1
}

# The file has this content (checked for a while: a save is asynchronous).
wait_content() {
  tries=100
  while [ "$tries" -gt 0 ]; do
    if [ "$(cat "$1" 2> /dev/null)" = "$2" ]; then return 0; fi
    sleep 0.05
    tries=$((tries - 1))
  done
  echo "timed out waiting for $1 to hold: $2 (it holds: $(cat "$1"))" >&2
  return 1
}

wait_exit() {
  tries=$((${1:-5} * 20))
  while [ "$tries" -gt 0 ]; do
    if [ -s "$CASE_DIR/.exit_code" ]; then return 0; fi
    sleep 0.05
    tries=$((tries - 1))
  done
  echo "timed out waiting for sim to exit" >&2
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
  CASE_DIR=$(mktemp -d "${TMPDIR:-/tmp}/sim-e2e.XXXXXX")
  SESSION=c$$
  SOCKET=$SOCKET-$$
  HOME=$CASE_DIR/home
  XDG_STATE_HOME=$CASE_DIR/state
  W=$CASE_DIR/work
  mkdir -p "$HOME" "$XDG_STATE_HOME" "$W"
  export CASE_DIR SESSION HOME XDG_STATE_HOME W
  trap 'status=$?
        if [ "$status" -ne 0 ] && tm has-session -t "$SESSION" 2> /dev/null; then
          echo "---- screen at failure ----"
          screen
          echo "---------------------------"
        fi
        panes=$(tm list-panes -a -F "#{pane_pid}" 2> /dev/null || true)
        tm kill-server 2> /dev/null || true
        # sim saves its unsaved text when tmux hangs it up (SAFE-6): let it finish before the files go
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
  cargo build --quiet -p sim
  SIM_BIN=${CARGO_TARGET_DIR:-$ROOT/target}/debug/sim
  export SIM_BIN

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
    log=$(mktemp "${TMPDIR:-/tmp}/sim-e2e-log.XXXXXX")
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

# A project to browse: two directories and four files.
project() {
  mkdir -p "$W/src" "$W/docs"
  printf 'fn lib() {}\n' > "$W/src/lib.rs"
  printf 'fn main() {}\n' > "$W/src/main.rs"
  printf '# guide\n' > "$W/docs/guide.md"
  printf 'readme\n' > "$W/README.md"
}

# Runs sim under a wrapper that records sim's pid, then prints its exit status and the tty modes
# (`stty -a`). Call it before `start`; extra arguments are VAR=value pairs for sim's environment.
wrap() {
  REAL=$SIM_BIN
  SIM_BIN=$CASE_DIR/wrap.sh
  {
    echo '#!/bin/sh'
    for kv in "$@"; do echo "export $(shq "$kv")"; done
    echo "sh -c 'echo \$\$ > \"\$0\"; exec \"\$@\"' $(shq "$CASE_DIR/sim.pid") $(shq "$REAL") \"\$@\""
    # tmux scrolls the first line away when the pane dies (remain-on-exit), so lead with a blank one.
    echo 'rc=$?; echo; echo "exit=$rc"'
    echo 'stty -a'
  } > "$SIM_BIN"
  chmod +x "$SIM_BIN"
}

restart() {
  tm kill-session -t "$SESSION"
  rm -f "$CASE_DIR/.exit_code"
}

# The screen no longer contains TEXT.
gone() { ! screen | grep -F -q -- "$1"; }

# After sim is gone: the normal screen is back and the tty is in cooked mode again.
terminal_restored() {
  wait_for icanon
  [ "$(tm display-message -p -t "$SESSION" '#{alternate_on}')" = 0 ]
  screen | grep -E -q '(^| )icanon( |$)'
  screen | grep -E -q '(^| )echo( |$)'
}

# Opens src/lib.rs and src/main.rs (the second is current), edits the end of both lines, and
# waits until sim shows both are modified. Call `project` and `start` first.
two_files_modified() {
  wait_for "src/lib.rs  1:1  NORMAL"
  typed ":e src/main.rs"
  keys Enter
  wait_for "src/main.rs  1:1  NORMAL  2/2"
  typed "A // MAIN"
  keys Escape
  wait_for "modified  2/2"
  keys Tab
  wait_for "src/lib.rs  1:1  NORMAL  1/2"
  typed "A // LIB"
  keys Escape
  wait_for "modified  1/2"
}

tree_1_starts_with_the_tree_of_the_current_directory_and_the_focus_in_it() {
  project
  start
  wait_for "▸ src/"
  screen | grep -F -q "▸ docs/"
  screen | grep -F -q "  README.md"
  screen | grep -F -q "Ctrl-n  tree"
  wait_for "docs/  TREE"
  gone "lib.rs" # only the top level has been read
}

tree_1_a_file_argument_opens_the_file_with_the_panel_closed() {
  project
  start src/main.rs
  wait_for "src/main.rs  1:1  NORMAL"
  gone "▸"
  screen | grep -F -q "   1 fn main() {}"
  keys C-n
  wait_for "▸ docs/"
}

tree_1_a_directory_argument_is_the_root_of_the_tree() {
  project
  start src
  wait_for "  lib.rs"
  gone "docs"
  gone "README"
}

tree_1_file_line_col_goes_to_the_place() {
  printf 'one\ntwo\nthree\n' > "$W/f.txt"
  start f.txt:3:2
  wait_for "f.txt  3:2  NORMAL"
}

tree_2_ctrl_n_toggles_the_panel_and_esc_gives_the_focus_back() {
  project
  start src/main.rs
  wait_for "src/main.rs  1:1  NORMAL"
  keys C-n
  wait_for "▸ docs/"
  wait_for "docs/  TREE" # the panel has the focus: its status line is the selected row
  keys Escape
  wait_for "src/main.rs  1:1  NORMAL" # the status line is the file's again
  typed x # the text has the focus: x deletes a char
  wait_for "modified"
  screen | grep -F -q "▸ docs/" # and the panel is still there
  keys C-n
  wait_for "TREE"
  keys C-n # closes it
  wait_gone "▸ docs/"
  screen | grep -F -q "   1 n main() {}"
}

tree_4_keys_move_expand_collapse_and_open() {
  project
  start
  wait_for "docs/  TREE"
  typed j
  wait_for "src/  TREE"
  typed l
  wait_for "    lib.rs" # the listing comes from a thread: wait for it before the next key
  typed jj
  wait_for "src/main.rs  TREE"
  typed h
  wait_for "src/  TREE"
  typed h
  wait_for "▸ src/"
  typed G
  wait_for "README.md  TREE"
  typed gg
  wait_for "docs/  TREE"
  typed l
  wait_for "guide.md"
  typed j
  wait_for "docs/guide.md  TREE"
  typed -
  wait_for "▸ docs/"
  wait_for "docs/  TREE"
  gone "guide.md"
  keys Down Down Down
  wait_for "README.md  TREE"
  keys Enter # opens it, and the focus moves to the text
  wait_for "README.md  1:1  NORMAL"
  screen | grep -F -q "   1 readme"
  keys C-n
  typed q # q closes the panel
  wait_gone "▸ docs/"
}

tree_5_directories_are_read_when_they_are_opened() {
  project
  start
  wait_for "▸ src/"
  printf 'x\n' > "$W/src/later.rs" # made after the top level was read
  typed jl
  wait_for "later.rs"
  mkdir "$W/zzz"
  keys C-n # closes the panel
  keys C-n # opens it again: what changed meanwhile shows
  wait_for "▸ zzz/"
}

buf_1_two_files_are_edited_switched_with_tab_saved_with_wa_and_closed_with_qa() {
  project
  start
  wait_for "▸ src/"
  typed jl
  wait_for "    lib.rs"
  typed j
  keys Enter
  wait_for "src/lib.rs  1:1  NORMAL"
  typed "A // LIB"
  keys Escape
  wait_for "modified"
  keys C-n
  typed j
  keys Enter
  wait_for "src/main.rs  1:1  NORMAL  2/2"
  typed "A // MAIN"
  keys Escape
  wait_for "modified  2/2"
  keys Tab
  wait_for "src/lib.rs  1:18  NORMAL  modified  1/2"
  screen | grep -F -q "fn lib() {} // LIB"
  keys BTab # Shift-Tab goes back the other way
  wait_for "src/main.rs  1:20  NORMAL  modified  2/2"
  typed ":wa"
  keys Enter
  wait_content "$W/src/lib.rs" "fn lib() {} // LIB"
  wait_content "$W/src/main.rs" "fn main() {} // MAIN"
  gone modified
  typed ":qa"
  keys Enter
  wait_exit
  [ "$(exit_code)" = 0 ]
  [ -z "$(ls -A "$XDG_STATE_HOME/si" 2> /dev/null)" ] # nothing left in the journal
}

buf_1_a_change_on_disk_is_merged_into_the_file_it_belongs_to() {
  project
  start src/lib.rs
  wait_for "src/lib.rs  1:1  NORMAL"
  typed ":e src/main.rs"
  keys Enter
  wait_for "src/main.rs  1:1  NORMAL  2/2"
  printf 'fn LIB() {}\n' > "$W/src/lib.new"
  mv "$W/src/lib.new" "$W/src/lib.rs"
  sleep 0.5
  gone "change on disk" # main.rs is on screen, and it did not change
  keys Tab
  wait_for "src/lib.rs  1:1  NORMAL  1 change on disk  1/2"
  screen | grep -F -q "fn LIB() {}"
}

buf_1_kill_9_leaves_a_snapshot_for_every_modified_file() {
  project
  wrap
  start src/lib.rs
  two_files_modified
  sleep 1.5 # a tick has surely come
  kill -9 "$(cat "$CASE_DIR/sim.pid")"
  wait_for "exit=137"
  restart
  start src/lib.rs
  wait_for "recovered unsaved changes from"
  screen | grep -F -q "fn lib() {} // LIB"
  typed ":e src/main.rs"
  keys Enter
  wait_for "recovered unsaved changes from"
  screen | grep -F -q "fn main() {} // MAIN"
  printf 'fn lib() {}\n' | cmp - "$W/src/lib.rs"
  typed ":qa!"
  keys Enter
  wait_for "exit=0"
  [ -z "$(ls -A "$XDG_STATE_HOME/si")" ] # discarding forgot them
}

buf_1_sigterm_snapshots_every_modified_file() {
  project
  wrap
  start src/lib.rs
  two_files_modified
  kill -TERM "$(cat "$CASE_DIR/sim.pid")" # at most a second after the last key
  wait_for "exit=143"
  restart
  start src/lib.rs
  wait_for "recovered unsaved changes from"
  screen | grep -F -q "fn lib() {} // LIB"
  typed ":e src/main.rs"
  keys Enter
  wait_for "recovered unsaved changes from"
  screen | grep -F -q "fn main() {} // MAIN"
}

buf_2_tab_switches_only_in_normal_mode() {
  project
  start src/lib.rs
  wait_for "src/lib.rs  1:1  NORMAL"
  typed ":e src/main.rs"
  keys Enter
  wait_for "src/main.rs  1:1  NORMAL  2/2"
  typed i
  wait_for INSERT
  keys Tab # a tab in the text, not the next file
  keys Escape
  wait_for "modified  2/2"
  screen | grep -F -q "src/main.rs"
  typed u
  keys Tab
  wait_for "src/lib.rs  1:1  NORMAL  1/2"
}

buf_3_q_closes_the_file_and_the_last_one_exits() {
  project
  start src/lib.rs
  wait_for "src/lib.rs  1:1  NORMAL"
  typed ":e src/main.rs"
  keys Enter
  wait_for "src/main.rs  1:1  NORMAL  2/2"
  typed ":q"
  keys Enter
  wait_for "src/lib.rs  1:1  NORMAL"
  gone "1/1"
  typed q
  wait_exit
  [ "$(exit_code)" = 0 ]
}

buf_3_qa_names_the_modified_files_and_qa_bang_discards_them() {
  project
  start src/lib.rs
  two_files_modified
  typed ":qa"
  keys Enter
  wait_for "no write since last change in src/lib.rs, src/main.rs (:qa! to discard)"
  [ ! -s "$CASE_DIR/.exit_code" ]
  typed ":qa!"
  keys Enter
  wait_exit
  [ "$(exit_code)" = 0 ]
  printf 'fn lib() {}\n' | cmp - "$W/src/lib.rs"
  printf 'fn main() {}\n' | cmp - "$W/src/main.rs"
}

buf_3_cq_exits_with_1() {
  project
  start src/lib.rs
  wait_for "src/lib.rs  1:1  NORMAL"
  typed x
  wait_for modified
  typed ":cq"
  keys Enter
  wait_exit
  [ "$(exit_code)" = 1 ]
}

buf_3_qa_restores_the_terminal() {
  project
  wrap
  start src/lib.rs
  wait_for "src/lib.rs  1:1  NORMAL"
  typed ":qa"
  keys Enter
  wait_for "exit=0"
  terminal_restored
}

buf_3_sigterm_restores_the_terminal() {
  project
  wrap
  start src/lib.rs
  wait_for "src/lib.rs  1:1  NORMAL"
  kill -TERM "$(cat "$CASE_DIR/sim.pid")"
  wait_for "exit=143"
  terminal_restored
}

buf_3_sighup_restores_the_terminal() {
  project
  wrap
  start src/lib.rs
  wait_for "src/lib.rs  1:1  NORMAL"
  kill -HUP "$(cat "$CASE_DIR/sim.pid")"
  wait_for "exit=129"
  terminal_restored
}

buf_3_a_panic_restores_the_terminal() {
  project
  wrap SIM_E2E_PANIC=1
  start src/lib.rs
  wait_for "SIM_E2E_PANIC is set"
  wait_for "exit=101"
  terminal_restored
}

main "$@"
