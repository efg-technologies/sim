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
#   screen_esc         print the screen with its SGR sequences (colors, reverse, bold)
#   wait_esc PATTERN   wait until the screen with its SGR sequences matches the (basic) regex
#   reply OSC          type a terminal's reply to sim's color query, e.g. reply '11;rgb:1e1e/1e1e/1e1e'
#   mouse B COL ROW M  type what a terminal reports for the mouse in SGR coding (1006): button code B
#                      (0 left, 32 left dragged, 64 / 65 wheel up / down), COL and ROW 1-based, M the
#                      press `M` or the release `m`; click, press, drag, release, wheel are shorter
#   mouse_flags        the mouse modes the pane's application has asked tmux for (any, SGR, button)
#   restart            end the case's tmux session, so that `start` can run sim again (files stay)
#   $CASE_DIR          the case's temp directory; $HOME and $XDG_STATE_HOME are inside it
#   $W                 sim's working directory, $CASE_DIR/work: the project a case makes
#   $CT                set it (CT=truecolor) before `start` to run sim with COLORTERM=$CT; otherwise sim
#                      runs without COLORTERM, whatever the user's terminal sets
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
  cmd="cd $(shq "$W") && env -u COLORTERM HOME=$(shq "$HOME") XDG_STATE_HOME=$(shq "$XDG_STATE_HOME") ${CT:+COLORTERM=$CT} $(shq "$SIM_BIN")"
  for arg in "$@"; do cmd="$cmd $(shq "$arg")"; done
  cmd="$cmd; echo \$? > $(shq "$CASE_DIR/.exit_code")"
  # sh -c, so that the user's login shell doesn't matter; remain-on-exit keeps the screen readable after sim exits.
  tm new-session -d -s "$SESSION" -x "$COLS" -y "$ROWS" "exec sh -c $(shq "$cmd")" \; set-option -w -t "$SESSION" remain-on-exit on \; set-option -sg escape-time 0
}

keys() { tm send-keys -t "$SESSION" "$@"; }
typed() { tm send-keys -t "$SESSION" -l -- "$1"; }
screen() { tm capture-pane -p -t "$SESSION"; }
screen_esc() { tm capture-pane -p -e -t "$SESSION"; }
# What a terminal answers to the color query (OSC 10, 11 and 4): typed into sim as keys.
reply() { tm send-keys -t "$SESSION" -l -- "$(printf '\033]%s\033\\' "$1")"; }

# What a terminal reports for the mouse (PTR-1), typed into sim like the replies above.
mouse() { tm send-keys -t "$SESSION" -l -- "$(printf '\033[<%s;%s;%s%s' "$1" "$2" "$3" "$4")"; }
press() { mouse 0 "$1" "$2" M; }
drag() { mouse 32 "$1" "$2" M; }
release() { mouse 0 "$1" "$2" m; }
click() { press "$1" "$2"; release "$1" "$2"; }
wheel() { mouse "$1" "$2" "$3" M; } # wheel 65 COL ROW is down, 64 up
# Which mouse modes the application in the pane has switched on: any, SGR coding, button events.
mouse_flags() { tm display-message -p -t "$SESSION" '#{mouse_any_flag}#{mouse_sgr_flag}#{mouse_button_flag}'; }

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

wait_esc() {
  tries=$((${2:-5} * 20))
  while [ "$tries" -gt 0 ]; do
    if screen_esc 2> /dev/null | grep -q -- "$1"; then return 0; fi
    sleep 0.05
    tries=$((tries - 1))
  done
  echo "timed out waiting for the colored screen to match: $1" >&2
  screen_esc | cat -v >&2
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
  # Under /tmp whatever $TMPDIR says (macOS's is 50 characters, and the 80 columns of the status line
  # then cut off the text that cases wait for), and canonical: /tmp and /var are links on macOS, and
  # sim records and shows the paths it resolved.
  CASE_DIR=$(cd "$(mktemp -d /tmp/sim-e2e.XXXXXX)" && pwd -P)
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
        sock=$(tm display-message -p "#{socket_path}" 2> /dev/null || true)
        tm kill-server 2> /dev/null || true
        # tmux leaves its socket behind when the server is killed: one file for every case, for ever
        rm -f "${sock:-${TMUX_TMPDIR:-/tmp}/tmux-$(id -u)/$SOCKET}"
        # sim saves its unsaved text when tmux hangs it up (SAFE-6): let it finish before the files go
        for pid in $panes; do
          tries=100
          while { pgrep -s "$pid" > /dev/null 2>&1 || kill -0 -- "-$pid" 2> /dev/null; } && [ "$tries" -gt 0 ]; do
            sleep 0.05
            tries=$((tries - 1))
          done
        done
        # a process on its way out can still add a file while rm empties a directory ("Directory not
        # empty", seen on macOS): rm again, which finds it empty once the process is gone
        tries=40
        until rm -rf "$CASE_DIR" 2> /dev/null || [ "$tries" -le 0 ]; do
          sleep 0.05
          tries=$((tries - 1))
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
    # A process that was killed leaves the tty raw: `stty -a` is read first, then the output modes are put
    # right, or on macOS its long report staircases down the screen and pushes the exit status off the top.
    printf '%s\n' 'rc=$?; modes=$(stty -a); stty opost onlcr; echo; echo "exit=$rc"; printf "%s\n" "$modes"'
  } > "$SIM_BIN"
  chmod +x "$SIM_BIN"
}

restart() {
  tm kill-session -t "$SESSION"
  rm -f "$CASE_DIR/.exit_code"
}

# The screen no longer contains TEXT.
gone() { ! screen | grep -F -q -- "$1"; }

# What is left in si's state directory but the places where files were closed (CLI-6, which stay).
journal_left() { ls -A "$XDG_STATE_HOME/si" 2> /dev/null | grep -v '^positions$' || true; }

# After sim is gone: the normal screen is back and the tty is in cooked mode again.
terminal_restored() {
  wait_for icanon
  [ "$(tm display-message -p -t "$SESSION" '#{alternate_on}')" = 0 ]
  [ "$(mouse_flags)" = 000 ] # and the mouse is let go (PTR-1)
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
  [ -z "$(journal_left)" ] # nothing left in the journal
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

buf_1_a_change_on_disk_after_a_save_is_still_merged() {
  project
  start src/lib.rs
  wait_for "src/lib.rs  1:1  NORMAL"
  typed "A // edit"
  keys Escape
  typed ":w"
  keys Enter
  wait_content "$W/src/lib.rs" "fn lib() {} // edit"
  wait_for "saved src/lib.rs"
  sleep 0.3 # the save's answer has replaced the watcher by now
  printf 'fn lib() {} // edit\nfn more() {}\n' > "$W/src/lib.new"
  mv "$W/src/lib.new" "$W/src/lib.rs"
  wait_for "+  2 fn more() {}" # taken in and marked, though the message of the save still shows
}

buf_1_a_reload_does_not_merge_what_the_old_watcher_read_of_the_file_it_left() {
  printf 'alpha\n' > "$W/a"
  printf 'beta\n' > "$W/b"
  ln -s a "$W/cur"
  start cur
  wait_for "cur  1:1  NORMAL"
  # a is rewritten every few milliseconds, so the watcher of `cur` (which is a) always has a read on
  # its way when the link moves to b and `:e` reads it: a Disk carries no name, and that read, begun
  # after the number of the `:e`'s read was taken, was merged into b's buffer as news of the disk
  (
    i=0
    while [ "$i" -lt 300 ]; do
      printf 'rewritten %s\n' "$i" > "$W/a"
      i=$((i + 1))
      sleep 0.01
    done
  ) &
  writer=$!
  sleep 0.2
  ln -sf b "$W/cur"
  typed ":e"
  keys Enter
  wait_for "beta"
  sleep 0.5
  gone "rewritten" # b's buffer holds b, and no word of a's
  wait "$writer"
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
  # The text first: the message of the first recovery is still on the screen until the keys are taken.
  wait_for "fn main() {} // MAIN"
  wait_for "recovered unsaved changes from"
  printf 'fn lib() {}\n' | cmp - "$W/src/lib.rs"
  typed ":qa!"
  keys Enter
  wait_for "exit=0"
  [ -z "$(journal_left)" ] # discarding forgot them
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
  # The text first: the message of the first recovery is still on the screen until the keys are taken.
  wait_for "fn main() {} // MAIN"
  wait_for "recovered unsaved changes from"
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

buf_3_qa_and_wa_respect_unresolved_conflicts_with_the_disk() {
  project
  start src/lib.rs
  two_files_modified
  printf 'fn LIB() {}\n' > "$W/src/lib.new"
  mv "$W/src/lib.new" "$W/src/lib.rs" # the disk changes the line that lib.rs has changed too
  wait_for "1 proposal"
  typed ":wa"
  keys Enter
  wait_for "not saved: conflicts with the disk in src/lib.rs"
  wait_content "$W/src/main.rs" "fn main() {} // MAIN" # the other file was saved
  printf 'fn LIB() {}\n' | cmp - "$W/src/lib.rs" # and this one was not
  typed ":qa"
  keys Enter
  wait_for "unresolved proposals in src/lib.rs (]c to review, :qa! discards)"
  [ ! -s "$CASE_DIR/.exit_code" ]
  typed ":qa!"
  keys Enter
  wait_exit
  [ "$(exit_code)" = 0 ]
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
  wrap SIM_E2E_PANIC=1 RUST_BACKTRACE=0 # a backtrace would scroll the message off the screen
  start src/lib.rs
  wait_for "SIM_E2E_PANIC is set"
  wait_for "exit=101"
  terminal_restored
}

# ---- S2: finding and marks ----

# The project as a git repository: target/ is ignored, everything else is committed.
gitproject() {
  project
  printf 'target/\n' > "$W/.gitignore"
  mkdir -p "$W/target"
  printf 'x\n' > "$W/target/junk.rs"
  git -C "$W" init -q
  git -C "$W" add -A
  git -C "$W" -c user.name=sim -c user.email=sim@example.com commit -q -m init
}

find_1_slash_searches_the_whole_tree_and_enter_opens_the_best_match() {
  project
  start
  wait_for "▸ src/"
  gone "main.rs" # src/ is closed: the search has to reach into it
  typed /main
  wait_for "/main  1 match"
  wait_for "▾ src/"
  screen | grep -F -q "    main.rs"
  keys Enter
  wait_for "src/main.rs  1:1  NORMAL"
  screen | grep -F -q "fn main"
  screen | grep -F -q "▾ src/" # the tree shows where the file is
}

find_1_esc_ends_the_search_and_restores_the_expansion() {
  project
  start
  wait_for "▸ src/"
  typed /lib
  wait_for "/lib  1 match"
  wait_for "    lib.rs"
  keys Escape
  wait_gone "lib.rs"
  screen | grep -F -q "▸ src/"
  wait_for "docs/  TREE"
}

find_1_matches_are_bold_the_rest_is_dimmed_and_backspace_edits_the_query() {
  project
  start
  wait_for "▸ src/"
  typed /mainx
  wait_for "/mainx  no match"
  wait_esc '2m.*docs/' # nothing matches: everything is dimmed
  keys BSpace
  wait_for "/main  1 match"
  wait_esc '1m.*src/' # a directory with a match below it is bold
  screen_esc | grep 'docs/' | grep -v -q '1m'
  screen_esc | grep 'docs/' | grep -q '2m'
}

find_1_down_commits_the_query_and_steps_to_the_next_match() {
  project
  start
  wait_for "▸ src/"
  typed /.md
  wait_for "/.md  2 matches"
  keys Down # from README.md (the best) round to docs/guide.md
  wait_for "/.md  2 matches"
  keys Enter
  wait_for "docs/guide.md  1:1  NORMAL"
}

find_3_ignored_files_are_not_found_and_untracked_ones_are() {
  gitproject
  printf 'u\n' > "$W/src/untracked.rs"
  start
  wait_for "▸ src/"
  typed /junk
  wait_for "/junk  no match"
  keys Escape
  typed /untracked
  wait_for "/untracked  1 match"
}

find_3_outside_git_the_files_come_from_a_walk() {
  project
  mkdir -p "$W/.git" # a directory called .git that is not a repository: skipped by the walk
  printf 'x\n' > "$W/.git/hidden.rs"
  start
  wait_for "▸ src/"
  typed /lib
  wait_for "/lib  1 match"
  keys Escape
  typed /hidden
  wait_for "/hidden  no match"
}

find_4_ctrl_p_from_a_file_opens_the_panel_and_starts_the_search() {
  project
  start src/main.rs
  wait_for "src/main.rs  1:1  NORMAL"
  gone "▸"
  keys C-p
  typed lib
  wait_for "/lib  1 match"
  screen | grep -F -q "▸ docs/"
  keys Enter
  wait_for "src/lib.rs  1:1  NORMAL  2/2"
}

find_4_ctrl_p_works_in_insert_mode_and_never_types() {
  project
  start src/main.rs
  wait_for "src/main.rs  1:1  NORMAL"
  typed i
  wait_for INSERT
  keys C-p
  typed guide
  wait_for "/guide  1 match"
  keys Escape # ends the search
  wait_for "docs/  TREE"
  keys Escape # gives the focus back (tmux would send two at once as Alt-Esc)
  wait_for "src/main.rs  1:1  INSERT" # still inserting, and nothing was typed
  gone modified
}

mark_1_changed_files_have_the_change_color_and_their_directories_a_tilde() {
  gitproject
  printf 'more\n' >> "$W/src/lib.rs"
  printf 'new\n' > "$W/docs/new.md"
  start
  wait_for "▸ src/ ~"
  screen | grep -F -q "▸ docs/ ~"
  typed l # opens docs/
  wait_for "new.md"
  wait_esc '7mnew\.md'
  screen_esc | grep 'guide\.md' | grep -v -q '7mguide\.md'
  wait_for "docs/  TREE"
  typed jjjl # src/
  wait_for "lib.rs"
  wait_esc '7mlib\.rs'
  screen_esc | grep 'main\.rs' | grep -v -q '7mmain\.rs'
}

mark_1_git_is_skipped_where_the_repositorys_config_could_run_commands() {
  gitproject
  printf 'more\n' >> "$W/src/lib.rs"
  git -C "$W" config filter.canary.clean "touch $W/witness; cat"
  printf '* filter=canary\n' > "$W/.gitattributes"
  start
  wait_for "git skipped: this repository's config could run commands"
  keys C-n
  keys C-n
  wait_for "▸ src/"
  gone "~" # no marks
  gone "git skipped" # said once
  [ ! -e "$W/witness" ] # and nothing ran
}

mark_1_a_file_that_changes_on_disk_while_sim_runs_is_marked_without_git() {
  project
  start src/main.rs
  wait_for "src/main.rs  1:1  NORMAL"
  keys C-n
  wait_for "▸ src/"
  gone "~"
  printf 'fn main() { 1 }\n' > "$W/src/main.new"
  mv "$W/src/main.new" "$W/src/main.rs"
  wait_for "▸ src/ ~"
}

mark_1_git_is_asked_again_after_a_save() {
  gitproject
  start src/lib.rs
  wait_for "src/lib.rs  1:1  NORMAL"
  keys C-n
  wait_for "▸ src/"
  gone "~"
  keys Escape
  typed "A // edit"
  keys Escape
  typed ":w"
  keys Enter
  wait_content "$W/src/lib.rs" "fn lib() {} // edit"
  wait_for "▸ src/ ~"
}

mark_2_files_you_open_are_marked_with_a_bullet_in_the_next_session() {
  project
  start src/main.rs
  wait_for "src/main.rs  1:1  NORMAL"
  typed ":q"
  keys Enter
  wait_exit
  tries=100
  while [ ! -s "$XDG_STATE_HOME/sim/history" ] && [ "$tries" -gt 0 ]; do
    sleep 0.05
    tries=$((tries - 1))
  done
  ls -l "$XDG_STATE_HOME/sim/history" | grep -q '^-rw-------'
  ls -ld "$XDG_STATE_HOME/sim" | grep -q '^drwx------'
  grep -q "$W/src/main.rs" "$XDG_STATE_HOME/sim/history"
  restart
  start
  wait_for "▸ src/"
  typed jl
  wait_for "• main.rs"
  screen | grep -F -q "    lib.rs"
  gone "• lib.rs"
}

mark_3_dot_narrows_the_tree_to_what_changed_and_dot_again_shows_everything() {
  gitproject
  printf 'more\n' >> "$W/src/lib.rs"
  start
  wait_for "▸ src/ ~"
  typed .
  wait_for "lib.rs"
  wait_gone "README.md"
  gone "docs/"
  typed .
  wait_for "README.md"
  screen | grep -F -q "▸ docs/"
}

speed_2_the_first_frame_comes_before_the_list_of_files() {
  # 100,000 files take too long to make here; the benchmark (tests/speed_2.rs) has them. Here:
  # the tree shows at once, however long the list takes (a slow git stands in for a big repository),
  # and the search finds what is in the list when it arrives.
  gitproject
  mkdir -p "$CASE_DIR/bin"
  printf '#!/bin/sh\nsleep 1\nexec %s "$@"\n' "$(command -v git)" > "$CASE_DIR/bin/git"
  chmod +x "$CASE_DIR/bin/git"
  wrap "PATH=$CASE_DIR/bin:$PATH"
  start
  wait_for "Ctrl-n  tree" 1
  wait_for "▸ src/" 1
  typed /lib
  wait_for "/lib  no match" 1
  wait_for "/lib  1 match" 10
}

# ---- S3: highlighting in color ----

syn_1_keywords_strings_and_comments_use_the_terminals_own_colors_without_a_reply() {
  printf 'fn main() { let s = "x"; } // c\n' > "$W/a.rs"
  start a.rs
  wait_for "a.rs  1:1  NORMAL"
  wait_esc '31mfn'
  wait_esc '31mlet'
  wait_esc '32m"x"'
  wait_esc '2;3m// c'
  ! screen_esc | grep -q '3[4]m'
}

syn_1_the_palette_moves_the_colors_and_the_reply_never_reaches_the_app() {
  printf 'fn main() { let s = "x"; } // c\n' > "$W/a.rs"
  CT=truecolor
  start a.rs
  wait_for "a.rs  1:1  NORMAL"
  wait_esc '31mfn' # first the theme's own
  reply '11;rgb:1e1e/1e1e/2e2e'
  reply '4;1;rgb:cc/00/00'
  reply '4;2;rgb:00/aa/00'
  wait_esc '38;2;[0-9;]*mfn'
  wait_esc '38;2;[0-9;]*m"x"'
  screen | grep -F -q "a.rs  1:1  NORMAL"
  gone modified
  typed x # keys still reach the App
  wait_for "modified"
}

syn_1_si_s_palette_paints_the_text_and_the_change_once_the_terminal_answers() {
  printf 'fn main() {} // c\n' > "$W/a.rs"
  CT=truecolor
  start a.rs
  wait_for "a.rs  1:1  NORMAL"
  wait_esc '31mfn' # the theme's own until the terminal answers
  reply '11;rgb:1e1e/1e1e/1e1e'
  reply '4;1;rgb:cc/00/00'
  wait_esc '38;2;[0-9;]*mfn'
  screen_esc | grep -q '38;2;[0-9;]*m *1 ' # the line number: si's secondary gray
  ! screen_esc | grep -q '[;[]2m' # no faint stand-in left for the comment
  wait_esc '3m.*38;2;[0-9;]*m// c\|38;2;[0-9;]*m.*3m// c' # italic and the secondary gray
  # A change on disk: its mark is the change color (a background) with si's gray on it.
  printf 'fn main() {} // c\nfn more() {}\n' > "$W/a.new"
  mv "$W/a.new" "$W/a.rs"
  wait_esc '48;2;[0-9;]*m'
  ! screen_esc | grep -q '[;[]7m' # and no reverse video
  gone modified
}

syn_2_zh_turns_the_colors_off_and_on() {
  printf 'fn main() {}\n' > "$W/a.rs"
  start a.rs
  wait_esc '31mfn'
  typed zh
  wait_for "syntax colors off"
  ! screen_esc | grep -q '31m'
  typed zh
  wait_for "syntax colors on"
  wait_esc '31mfn'
}

syn_3_a_parse_that_never_ends_is_given_up_and_the_other_files_keep_their_colors() {
  # syntect loops for ever on this text in JavaScript; sim stays at its keys, says so after the
  # highlighter's 5 s, and highlights the next file
  printf '/\n/\n /*\n' > "$W/loop.js"
  printf 'fn main() { let s = "x"; }\n' > "$W/a.rs"
  start loop.js
  wait_for "loop.js  1:1  NORMAL"
  typed jj
  wait_for "loop.js  3:1  NORMAL" # the keys are answered while it loops
  wait_for "syntax colors stopped for loop.js" 15
  typed ":e a.rs"
  keys Enter
  wait_for "a.rs  1:1  NORMAL  2/2"
  wait_esc '31mfn' # and a.rs has its colors
}

syn_3_a_file_over_10_mb_is_not_highlighted() {
  yes 'fn main() { let s = "x"; }' | head -c 11000000 > "$W/big.rs"
  printf 'fn main() { let s = "x"; }\n' > "$W/small.rs"
  start big.rs
  wait_for "big.rs  1:1  NORMAL"
  sleep 0.5
  ! screen_esc | grep -q '31m'
  typed ":e small.rs"
  keys Enter
  wait_for "small.rs  1:1  NORMAL"
  wait_esc '31mfn'
}

syn_4_toml_typescript_and_dockerfile_are_highlighted() {
  printf '[package]\nname = "sim"\n' > "$W/Cargo.toml"
  printf 'const a: string = "x";\n' > "$W/a.ts"
  printf 'FROM rust:1\nRUN echo "x"\n' > "$W/Dockerfile"
  start Cargo.toml
  wait_esc '32m"sim"'
  typed ":e a.ts"
  keys Enter
  wait_esc '31mconst'
  typed ":e Dockerfile"
  keys Enter
  wait_esc '31mFROM'
}

syn_5_the_selection_is_underlined_without_truecolor_and_gray_with_it() {
  printf 'fn main() {}\n' > "$W/a.rs"
  start a.rs
  wait_for "a.rs  1:1  NORMAL"
  typed vlll
  wait_for VISUAL
  wait_esc '4m'
  ! screen_esc | grep -q '48;2'
  restart
  CT=truecolor
  start a.rs
  wait_for "a.rs  1:4  NORMAL" # where the file was left (CLI-6)
  reply '11;rgb:1e1e/1e1e/1e1e'
  typed vlll
  wait_for VISUAL
  wait_esc '48;2;'
  ! screen_esc | grep -q '[;[]4m'
}

buf_1_snapshots_are_off_when_the_state_directory_is_not_private() {
  project
  mkdir -p "$XDG_STATE_HOME/si"
  chmod 755 "$XDG_STATE_HOME/si"
  start src/lib.rs
  wait_for "src/lib.rs  1:1  NORMAL"
  typed x
  wait_for "cannot write snapshots: $XDG_STATE_HOME/si is not private" # the first tick says so, once
  typed ":w"
  keys Enter
  wait_content "$W/src/lib.rs" "n lib() {}" # saving works all the same
  [ -z "$(ls -A "$XDG_STATE_HOME/si")" ] # and nothing was written there
}

# ---- S4: the mouse ----

ptr_1_a_click_in_the_panel_opens_a_file_and_toggles_a_directory() {
  project
  start
  wait_for "▸ src/"
  click 3 2 # the row of src/
  wait_for "▾ src/"
  wait_for "    lib.rs"
  click 3 2
  wait_gone "lib.rs"
  click 5 3 # README.md
  wait_for "README.md  1:1  NORMAL"
  screen | grep -F -q "   1 readme"
  keys C-n
  wait_for "▸ docs/"
  click 5 3 # the same row: the file is shown again, not opened a second time
  wait_for "README.md  1:1  NORMAL"
  gone "2/2"
}

ptr_1_a_click_in_the_text_moves_the_cursor_and_a_drag_selects_until_a_click() {
  printf 'alpha beta\ngamma delta\n' > "$W/f.txt"
  start f.txt
  wait_for "f.txt  1:1  NORMAL"
  click 12 2 # the text starts at column 6: this is its 7th char of line 2
  wait_for "f.txt  2:7  NORMAL"
  press 8 1 # the 3rd char of line 1
  drag 9 1
  drag 12 1
  wait_for VISUAL
  release 12 1
  wait_for "f.txt  1:7  VISUAL" # the pointer's char is the cursor
  wait_esc '[;[]4mpha b'
  typed d # the selection is Visual mode's: its keys work on it
  wait_for "aleta"
  click 7 1
  wait_for "f.txt  1:2  NORMAL  modified"
  click 7 2
  wait_for "f.txt  2:2  NORMAL  modified"
}

ptr_1_the_wheel_scrolls_the_text_and_the_panel_by_what_is_under_the_pointer() {
  project
  seq 1 100 | sed 's/^/line /' > "$W/long.txt"
  for i in $(seq 1 30); do : > "$W/file$i.txt"; done
  start long.txt
  wait_for "long.txt  1:1  NORMAL"
  wheel 65 40 5 # three lines on; the cursor keeps its 8 lines of context
  wait_for "   4 line 4"
  wait_for "long.txt  12:1  NORMAL"
  wheel 65 40 5
  wheel 65 40 5
  wait_for "  10 line 10"
  wheel 64 40 5
  wait_for "   7 line 7"
  keys C-n
  wait_for "▸ docs/"
  wheel 65 3 5 # over the panel: it scrolls, the text does not
  wheel 65 3 5
  wait_for "   7 line 7"
  gone "▸ docs/"
  wait_for "file5.txt" # three rows a notch, and the text did not follow
  wheel 64 3 5
  wheel 64 3 5
  wait_for "▸ docs/"
}

ptr_1_mouse_reporting_is_on_while_sim_runs_and_off_after_every_way_out() {
  project
  wrap
  start src/lib.rs
  wait_for "src/lib.rs  1:1  NORMAL"
  [ "$(mouse_flags)" = 111 ] # buttons and drags, in SGR coding
  typed ":qa"
  keys Enter
  wait_for "exit=0"
  terminal_restored
}

ptr_2_the_mouse_edits_nothing() {
  printf 'alpha beta\ngamma delta\n' > "$W/f.txt"
  start f.txt
  wait_for "f.txt  1:1  NORMAL"
  click 8 1
  press 8 1
  drag 14 2
  release 14 2
  mouse 2 8 1 M # the right button
  mouse 2 8 1 m
  mouse 1 8 1 M # the middle button (paste, in some editors)
  mouse 1 8 1 m
  wheel 65 8 1
  wheel 64 8 1
  click 3 24 # the status line
  sleep 0.3
  gone modified
  screen | grep -F -q "   1 alpha beta"
  [ "$(cat "$W/f.txt")" = "$(printf 'alpha beta\ngamma delta')" ]
  keys Escape
  typed ":q"
  keys Enter # nothing is modified, so this quits
  wait_exit
  [ "$(exit_code)" = 0 ]
}

main "$@"
