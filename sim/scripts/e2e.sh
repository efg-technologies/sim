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
#   $NOCOLOR           set it before `start` to run sim with NO_COLOR=$NOCOLOR (COLOR-2); otherwise it is unset
#   $TP                set it before `start` to run sim with TERM_PROGRAM=$TP (ghostty brings Nerd Font symbols,
#                      ICON-1); otherwise the terminal is not known to bring any, whatever the user's is
#   $HINT_TAB          set it before `start` to see the hint of KEY-6 (otherwise Tab counts as learned)
#   $HINTED            set it before `start` to see the hint of ICON-1; otherwise the hint counts as given
#   $ICONS_ON          set it in a case that draws icons: no other screen may have a private-use code point
#   $SIM_PATH          set it before `start` to run sim with that PATH (for a fake or a missing rg)
#   look WORD          make the look the next `start` begins with (calm, flat, depth, or none for no file:
#                      LOOK-1); a case that does not say gets calm, so that the looks of the other cases stay what they were written for
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

# LOOK-1: the look is remembered in sim's state directory, and the first run is depth; the cases are
# written for calm unless they say otherwise.
look() {
  mkdir -p -m 700 "$XDG_STATE_HOME/sim"
  if [ "$1" = none ]; then rm -f "$XDG_STATE_HOME/sim/look"; else echo "$1" > "$XDG_STATE_HOME/sim/look"; fi
  LOOKED=1
}

start() {
  [ -n "${LOOKED:-}" ] || [ -e "$XDG_STATE_HOME/sim/look" ] || look calm
  # ICON-1: the icons start off in tmux (no Nerd Font is known), once with a hint on the status line;
  # the cases are written without that hint, so it is taken as given unless a case wants to see it.
  [ -n "${HINTED:-}" ] || [ -e "$XDG_STATE_HOME/sim/icons-hint" ] || { mkdir -p -m 700 "$XDG_STATE_HOME/sim" && echo shown > "$XDG_STATE_HOME/sim/icons-hint"; }
  # KEY-6: Tab is learned (three uses) unless a case wants the hint for a second file
  [ -n "${HINT_TAB:-}" ] || [ -e "$XDG_STATE_HOME/sim/tab-uses" ] || { mkdir -p -m 700 "$XDG_STATE_HOME/sim" && echo 3 > "$XDG_STATE_HOME/sim/tab-uses"; }
  cmd="cd $(shq "$W") && env -u COLORTERM -u NO_COLOR -u TERM_PROGRAM -u GHOSTTY_RESOURCES_DIR -u KITTY_PID -u WEZTERM_PANE HOME=$(shq "$HOME") XDG_STATE_HOME=$(shq "$XDG_STATE_HOME") ${CT:+COLORTERM=$CT} ${NOCOLOR:+NO_COLOR=$NOCOLOR} ${TP:+TERM_PROGRAM=$TP} ${SIM_PATH:+PATH=$(shq "$SIM_PATH")} $(shq "$SIM_BIN")"
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

# ICON-1: without icons no screen of any case has a private-use code point (U+E000-F8FF, planes 15
# and 16: what only a Nerd Font draws). Every wait_for that succeeds looks (a case with icons on
# sets ICONS_ON).
no_private_use() {
  [ -n "${ICONS_ON:-}" ] && return 0
  if screen 2> /dev/null | perl -CS -ne 'exit 1 if /[\x{E000}-\x{F8FF}\x{F0000}-\x{10FFFF}]/'; then return 0; fi
  echo "a private-use code point is on the screen, and the icons are off:" >&2
  screen >&2
  return 1
}

wait_for() {
  tries=$((${2:-5} * 20))
  while [ "$tries" -gt 0 ]; do
    if screen 2> /dev/null | grep -F -q -- "$1"; then no_private_use; return; fi
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
  cargo build --quiet -p sim-editor
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
# What the journal left in the state directory: not the places, and not the choices sim keeps beside it
# (`look`, `history`, `icons`, and the marker of the hint, STATE-1).
journal_left() { ls -A "$XDG_STATE_HOME/sim" 2> /dev/null | grep -Ev '^(positions|look|history|icons|icons-hint|tab-uses|no-hooks|\.migrated)$' || true; }

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
  wait_for "conflict"
  typed ":wa"
  keys Enter
  wait_for "not saved: conflicts with the disk in src/lib.rs"
  wait_content "$W/src/main.rs" "fn main() {} // MAIN" # the other file was saved
  printf 'fn LIB() {}\n' | cmp - "$W/src/lib.rs" # and this one was not
  typed ":qa"
  keys Enter
  wait_for "no write since last change in src/lib.rs"
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

find_4_ctrl_p_from_a_file_opens_the_picker_and_enter_opens_the_file() {
  project
  start src/main.rs
  wait_for "src/main.rs  1:1  NORMAL"
  gone "▸"
  keys C-p
  typed lib
  wait_for "Find files"
  wait_for "1 / 1" # the count
  screen | grep -F -q "▎src/lib.rs" # the selected result
  wait_for "fn lib() {}" # and its preview
  gone "▸" # the panel was closed before, and still is
  keys Enter
  wait_for "src/lib.rs  1:1  NORMAL  2/2"
  gone "Find files"
}

find_4_ctrl_p_works_in_insert_mode_and_never_types() {
  project
  start src/main.rs
  wait_for "src/main.rs  1:1  NORMAL"
  typed i
  wait_for INSERT
  keys C-p
  typed guide
  wait_for "1 / 1"
  keys Escape # one key closes the picker
  wait_for "src/main.rs  1:1  INSERT" # still inserting, and nothing was typed
  gone "Find files"
  gone modified
}

# A project for the search of contents: a file of 100 lines with `needle` on line 60.
grepproject() {
  project
  i=1
  while [ "$i" -le 100 ]; do
    if [ "$i" -eq 60 ]; then echo "the needle is here"; else echo "line $i"; fi
    i=$((i + 1))
  done > "$W/big.txt"
  printf 'fn needle() {}\nfn other() {}\n' > "$W/src/lib.rs"
}

grep_1_ctrl_f_searches_the_contents_as_you_type_and_esc_puts_everything_back() {
  project
  start src/main.rs
  wait_for "src/main.rs  1:1  NORMAL"
  gone "▸"
  keys C-f
  typed fn
  wait_for "Live grep"
  wait_for "1 / 2" # the count
  screen | grep -F -q "src/lib.rs:1  fn lib() {}" # path, line and text
  typed " m"
  wait_for "1 / 1"
  gone "src/lib.rs:1"
  keys BSpace BSpace
  wait_for "1 / 2"
  keys C-u # clears the query
  wait_for "0 / 0"
  gone "src/lib.rs:1"
  typed fn
  wait_for "1 / 2"
  keys Escape
  wait_for "src/main.rs  1:1  NORMAL"
  gone "Live grep"
  gone "▸" # the panel was closed before, and is again
  [ "$(cat "$W/src/main.rs")" = "fn main() {}" ] # nothing was written
}

grep_1_ctrl_f_works_in_insert_mode_and_never_types() {
  project
  start src/main.rs
  wait_for "src/main.rs  1:1  NORMAL"
  typed i
  wait_for INSERT
  keys C-f
  typed guide
  wait_for "1 / 1"
  keys Escape
  wait_for "src/main.rs  1:1  INSERT"
  gone modified
}

grep_3_the_matches_are_bold_and_the_path_and_line_dim() {
  grepproject
  start
  wait_for "▸ src/"
  keys C-f
  typed needle
  wait_for "1 / 2"
  wait_esc '1mneedle' # the match, bold
  screen_esc | grep -q '\[2m.*big.txt:60' # the path and the line are the secondary gray (faint)
  ! screen_esc | grep 'big.txt:60' | grep -q '\[[34][0-7]m' # and the list uses no color
}

grep_4_the_pane_previews_the_result_enter_opens_it_there_and_esc_puts_the_tree_back() {
  grepproject
  start
  wait_for "▸ src/"
  keys C-f
  typed needle
  wait_for "1 / 2"
  wait_for "the needle is here" # the preview of big.txt (the first result by path), in the pane
  wait_for " 57 line 57" # centered: three lines above it
  gone " 10 line 10"
  keys Down
  wait_for "2 / 2" # (the selection moved)
  wait_for "  1 fn needle() {}" # the preview follows the selection into the other file
  gone "line 57"
  keys Up
  wait_for " 57 line 57"
  [ -z "$(ls -A "$XDG_STATE_HOME/sim" 2> /dev/null | grep -Ev '^(look|icons-hint|tab-uses|no-hooks)$')" ] # not an open: no history yet (`look` is the seeded look)
  keys Enter
  wait_for "big.txt  60:5  NORMAL" # the line, and the column of the match
  screen | grep -F -q "▸ src/" # the tree is as it was
  gone "Live grep"
  keys Tab
  keys C-f
  typed other
  wait_for "1 / 1"
  keys Enter
  wait_for "src/lib.rs  2:4  NORMAL"
  wait_for "▾ src/" # the tree shows where the file is
}

grep_4_esc_puts_back_the_file_the_position_and_the_tree() {
  grepproject
  start big.txt
  wait_for "big.txt  1:1  NORMAL"
  typed 5j
  wait_for "big.txt  6:1  NORMAL"
  keys C-n
  keys Right # into src/? the focus is in the panel: open the first directory
  wait_for "▾ docs/"
  keys C-f
  typed needle
  wait_for "1 / 2"
  keys Escape
  wait_for "docs/  TREE" # the panel, as it was: the same row, the same expansion
  gone "Live grep"
  screen | grep -F -q "▾ docs/"
  keys Escape
  wait_for "big.txt  6:1  NORMAL" # and the file, with the cursor where it was
}

grep_5_a_slow_search_never_blocks_a_key_and_a_new_query_stops_the_old_rg() {
  project
  mkdir -p "$CASE_DIR/bin"
  # a rg that takes a long time and says where it is
  printf '#!/bin/sh\necho $$ >> "%s"\nexec sleep 30\n' "$CASE_DIR/rg.pids" > "$CASE_DIR/bin/rg"
  chmod +x "$CASE_DIR/bin/rg"
  SIM_PATH="$CASE_DIR/bin:$PATH"
  start src/main.rs
  wait_for "src/main.rs  1:1  NORMAL"
  keys C-f
  typed f
  wait_for "0 / 0" # (and a spinner)
  tries=100
  until [ -s "$CASE_DIR/rg.pids" ] || [ "$tries" -le 0 ]; do sleep 0.05; tries=$((tries - 1)); done
  first=$(head -n 1 "$CASE_DIR/rg.pids")
  kill -0 "$first"
  typed n # the keys go on while rg sleeps; the new query kills the old process
  wait_for "❯ fn"
  tries=100
  while kill -0 "$first" 2> /dev/null && [ "$tries" -gt 0 ]; do sleep 0.05; tries=$((tries - 1)); done
  ! kill -0 "$first" 2> /dev/null
  keys Escape # a key at once, and the search that was running is stopped too
  wait_for "src/main.rs  1:1  NORMAL"
  second=$(tail -n 1 "$CASE_DIR/rg.pids")
  tries=100
  while kill -0 "$second" 2> /dev/null && [ "$tries" -gt 0 ]; do sleep 0.05; tries=$((tries - 1)); done
  ! kill -0 "$second" 2> /dev/null
}

grep_5_without_rg_or_git_the_built_in_reader_searches_the_walk() {
  project
  printf 'ignored needle\n' > "$W/skip.txt"
  mkdir -p "$CASE_DIR/bin"
  ln -s "$(command -v sh)" "$CASE_DIR/bin/sh"
  ln -s "$(command -v env)" "$CASE_DIR/bin/env"
  SIM_PATH="$CASE_DIR/bin"
  start
  wait_for "▸ src/"
  keys C-f
  typed main
  wait_for "1 / 1"
  screen | grep -F -q "src/main.rs:1"
  keys Escape
  keys C-f
  typed needle
  wait_for "1 / 1"
  screen | grep -F -q "skip.txt:1"
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
  git -C "$W" config core.sshCommand "touch $W/witness"
  start
  wait_for "git skipped: core.sshcommand in .git/config could run a command"
  keys C-n
  keys C-n
  wait_for "▸ src/"
  gone "~" # no marks
  gone "git skipped" # said once
  [ ! -e "$W/witness" ] # and nothing ran
}

mark_1_core_hookspath_does_not_stop_git_and_no_hook_runs() {
  gitproject
  mkdir -p "$W/.husky/_"
  for hook in pre-commit post-index-change; do
    printf '#!/bin/sh\ntouch "%s/witness-%s"\n' "$CASE_DIR" "$hook" > "$W/.husky/_/$hook"
    chmod +x "$W/.husky/_/$hook"
  done
  git -C "$W" config core.hooksPath .husky/_ # as husky does
  printf 'more\n' >> "$W/src/lib.rs"
  start
  wait_for "▸ src/ ~" # the marks are there: git ran
  gone "git skipped"
  [ ! -e "$CASE_DIR/witness-pre-commit" ] && [ ! -e "$CASE_DIR/witness-post-index-change" ]
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

syn_1_a_terminal_that_answers_the_background_only_still_gets_truecolor_syntax_colors() {
  # (inside tmux the replies to OSC 4 often never come)
  printf 'fn main() {} // c\n' > "$W/a.rs"
  CT=truecolor
  start a.rs
  wait_for "a.rs  1:1  NORMAL"
  wait_esc '31mfn' # the theme's own until the terminal answers about its background
  reply '11;rgb:1e1e/1e1e/1e1e'
  wait_esc '38;2;[0-9;]*mfn' # derived from the hue of a keyword: truecolor, no reply for red needed
  ! screen_esc | grep -q '31mfn'
  keyword() { screen_esc | grep -a 'fn' | sed -n 's/.*\(38;2;[0-9;]*\)mfn.*/\1/p' | head -1; }
  before=$(keyword)
  [ -n "$before" ]
  reply '4;1;rgb:ff/00/00' # a late real reply replaces the derived color
  tries=60
  while [ "$(keyword)" = "$before" ] && [ "$tries" -gt 0 ]; do
    sleep 0.05
    tries=$((tries - 1))
  done
  [ "$(keyword)" != "$before" ]
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
  # syntect loops for ever on this text in QML; sim stays at its keys, says so after the
  # highlighter's 5 s, and highlights the next file
  printf 'x = y /* c */\ny\nz\n' > "$W/loop.qml"
  printf 'fn main() { let s = "x"; }\n' > "$W/a.rs"
  start loop.qml
  wait_for "loop.qml  1:1  NORMAL"
  typed jj
  wait_for "loop.qml  3:1  NORMAL" # the keys are answered while it loops
  wait_for "syntax colors stopped for loop.qml" 15
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
  mkdir -p -m 700 "$XDG_STATE_HOME/sim"
  chmod 755 "$XDG_STATE_HOME/sim"
  start src/lib.rs
  wait_for "history: $XDG_STATE_HOME/sim is not private" # sim's own choices follow the same rule (STATE-1)
  typed j
  wait_for "src/lib.rs  1:1  NORMAL"
  typed x
  wait_for "cannot write snapshots: $XDG_STATE_HOME/sim is not private" # the first tick says so, once
  typed ":w"
  keys Enter
  wait_content "$W/src/lib.rs" "n lib() {}" # saving works all the same
  [ -z "$(ls -A "$XDG_STATE_HOME/sim" | grep -Ev '^(look|icons-hint|tab-uses|no-hooks)$')" ] # and nothing was written there (the case's own files aside)
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

look_1_zl_cycles_the_looks_and_the_choice_is_the_next_runs() {
  printf 'one\ntwo\n' > "$W/a.txt"
  start a.txt
  wait_for "a.txt  1:1  NORMAL"
  gone "╭"
  typed zl
  wait_for "look: flat"
  gone "╭"
  typed zl
  wait_for "look: depth"
  wait_for "╭─" # the boxes, in the terminal's own colors: nothing answered for a palette
  screen | grep -F -q "╰"
  screen | grep -F -q "   1 one"
  typed zl
  wait_for "look: calm"
  gone "╭"
  typed zl
  wait_for "look: flat"
  typed ":q"
  keys Enter
  wait_exit
  [ "$(cat "$XDG_STATE_HOME/sim/look")" = flat ]
  restart
  start a.txt
  wait_for "a.txt  1:1  NORMAL"
  typed zl
  wait_for "look: depth" # the one after flat: flat was the look this run began with
  typed ":q"
  keys Enter
  wait_exit
  # No file, or one that says anything else: depth, the first look.
  restart
  look none
  start a.txt
  wait_for "╭─"
  restart
  look nonsense
  start a.txt
  wait_for "╭─"
}

look_3_the_border_badges_a_conflict_with_the_disk() {
  printf 'one\ntwo\nthree\nfour\n' > "$W/a.txt"
  look depth
  CT=truecolor
  start a.txt
  wait_for "a.txt  1:1  NORMAL"
  reply '11;rgb:1e1e/1e1e/1e1e'
  typed jA
  typed "!"
  keys Escape
  wait_for "●" # modified: the status line of the interface says it by its dot
  printf 'one\nTWO\nthree\nfour\n' > "$W/a.new"
  mv "$W/a.new" "$W/a.txt" # the disk changes the line that is changed here too
  wait_for "1 conflict with the disk"
  wait_esc '48;2;[0-9;]*m 1 conflict with the disk ' # the badge is in the top border, on the card's surface
  screen | grep -E -q '~2 +two!' # the line of mine is marked
  printf 'one\nTWO\nthree\nfour\n' | cmp - "$W/a.txt" # nothing was written
  typed ":w"
  keys Enter
  wait_for "not saved: the disk conflicts"
  typed ":w!"
  keys Enter
  wait_for "saved a.txt; the disk's version is in"
  gone "conflict with the disk"
  printf 'one\ntwo!\nthree\nfour\n' | cmp - "$W/a.txt"
}

look_6_the_text_fades_with_its_distance_from_the_cursor() {
  i=1
  while [ "$i" -le 40 ]; do echo "line $i" >> "$W/a.txt"; i=$((i + 1)); done
  look depth
  CT=truecolor
  start a.txt
  wait_for "a.txt  1:1  NORMAL"
  reply '11;rgb:1e1e/1e1e/1e1e'
  wait_esc '38;2;[0-9;]*m'
  # The color of the text just before `$1` on the screen (the last SGR color on its row).
  color_of() { screen_esc | grep -a "$1" | sed 's/.*\x1b\[\([0-9;]*38;2;[0-9;]*\)m[^m]*'"$1"'.*/\1/' | tail -1; }
  near=$(color_of "line 2$")
  far=$(color_of "line 20$")
  [ -n "$near" ] && [ -n "$far" ] && [ "$near" != "$far" ] # the far row is dimmer
  # Calm has no gradient: the same text, the same colors.
  typed zl
  wait_for "look: calm"
  [ "$(color_of 'line 2$')" = "$(color_of 'line 20$')" ]
}

ui_8_markdown_is_colored_and_every_char_of_the_file_is_on_the_screen() {
  printf '# Title\n\nSome `code` and **bold** here.\n\n> quoted\n\n```bash\nls -l\n```\n' > "$W/a.md"
  look flat
  CT=truecolor
  start a.md
  wait_for "a.md"
  reply '11;rgb:1e1e/1e1e/1e1e'
  wait_esc '1m.*38;2;[0-9;]*m Title' # a heading: bold, in a color of its own (the heading hue, cyan)
  ! screen_esc | grep -a 'Title' | grep -q '38;2;230;230;230m Title' # not the text's primary gray
  wait_esc '48;2;[0-9;]*m#' # on a band of that hue, across the row, behind its `#`
  # Nothing is hidden: the markers and the url are drawn, only dimmer.
  screen | grep -F -q '# Title'
  screen | grep -F -q 'Some `code` and **bold** here.'
  screen | grep -F -q '▌ quoted' # the quote's bar, in the cell of its `>`
  ! screen | grep -F -q '> quoted'
  screen | grep -F -q '```bash'
  screen | grep -F 'bash' | grep -q 'bash  *bash' # the language again, at the right
  wait_esc '48;2;[0-9;]*m`' # inline code is on a surface, its backtick too
  # Calm draws no surface and no bar: the file's own chars, colored like syntax.
  typed zl
  wait_for "look: depth"
  typed zl
  wait_for "look: calm"
  screen | grep -F -q '> quoted'
  ! screen_esc | grep -q '48;2;'
  wait_esc '1m.*38;2;230;230;230m Title'
}

ui_8_the_colors_of_a_fenced_block_far_from_the_cursor_reach_the_screen_through_tmux() {
  # tmux 3.7c drops an SGR sequence of more than 20 parameters: depth's fog, the band and the
  # language's color on one cell must fit in fewer, or the code of the block is drawn plain.
  { i=1; while [ "$i" -le 12 ]; do echo "line $i"; i=$((i + 1)); done; printf '```bash\nparu -S chezmoi\n```\n'; } > "$W/a.md"
  look depth
  CT=truecolor
  start a.md
  wait_for "a.md"
  reply '11;rgb:1e1e/1e1e/1e1e'
  reply '4;6;rgb:7f7f/bfbf/b4b4'
  wait_for "paru -S chezmoi"
  # The cell of `paru` has a color of its own and the band's background, which are two separate
  # sequences in what tmux reports (a foreground, then a background, then the text).
  wait_esc '38;2;[0-9;]*m.\[48;2;[0-9;]*mparu'
}

ui_3_the_tree_shows_the_home_relative_root_icons_git_letters_and_zi_turns_icons_off() {
  W=$HOME/work # the project is below the case's $HOME
  mkdir -p "$W"
  gitproject
  printf 'more\n' >> "$W/src/lib.rs"
  printf 'new\n' > "$W/docs/new.md"
  look depth
  CT=truecolor
  ICONS_ON=1 # the choice `zi` remembers beats the default (ICON-1)
  mkdir -p -m 700 "$XDG_STATE_HOME/sim" && echo on > "$XDG_STATE_HOME/sim/icons"
  start
  reply '11;rgb:1e1e/1e1e/1e1e' # the palette makes the interface
  wait_for "~/work" # the root, below $HOME (the case's)
  closed=$(printf '\357\201\273') # U+F07B, the closed folder of Nerd Fonts
  screen | grep -F 'docs' | grep -q "$closed"
  screen | grep -F 'docs' | grep -q 'U *│' # something below it is new: the letter of the strongest status
  screen | grep -F 'src' | grep -q 'M *│'
  typed l # docs/ opens
  wait_for "new.md"
  screen | grep -F 'new.md' | grep -q 'U *│' # the letter of git at the right edge of the panel
  typed jjjl # src/
  wait_for "lib.rs"
  screen | grep -F 'lib.rs' | grep -q 'M *│'
  # The text has the focus once a file is open: `zi` turns the icons off and on.
  typed jl
  wait_for "NORMAL"
  typed zi
  wait_for "icons off"
  ! screen | grep -q "$closed"
  screen | grep -F -q "▸ target/"
  [ "$(cat "$XDG_STATE_HOME/sim/icons")" = off ]
  typed zi
  wait_for "icons on"
  screen | grep -q "$closed"
  [ "$(cat "$XDG_STATE_HOME/sim/icons")" = on ]
}

ui_5_the_numbers_are_relative_the_cursors_line_is_lit_and_insert_mode_has_a_surface() {
  i=1
  while [ "$i" -le 12 ]; do echo "line $i" >> "$W/a.txt"; i=$((i + 1)); done
  look flat
  CT=truecolor
  start a.txt
  wait_for "a.txt"
  reply '11;rgb:1e1e/1e1e/1e1e'
  wait_esc '38;2;[0-9;]*m'
  typed 3j
  wait_for "line 4"
  screen | grep -F -q "  4   line 4" # the cursor's line: its own number, left-aligned
  screen | grep -F -q "    1 line 3" # the others: the distance
  screen | grep -F -q "    3 line 1"
  wait_esc '1m4  ' # primary and bold
  ! screen_esc | grep -a 'line' | sed 's/\(line [0-9]*\).*/\1/' | grep -q '48;2;' # Normal mode: no surface on the line (the tab and the scrollbar's handle have one)
  typed i
  wait_for "INSERT"
  tries=60
  surfaced() { screen_esc | grep -a "$1" | sed 's/\(line [0-9]*\).*/\1/' | grep -q '48;2;'; }
  while ! surfaced 'line 4' && [ "$tries" -gt 0 ]; do sleep 0.05; tries=$((tries - 1)); done
  surfaced 'line 4' # Insert mode: the whole line has one
  ! surfaced 'line 5'
  keys Escape
  wait_for "NORMAL"
  ! screen_esc | grep -a 'line' | sed 's/\(line [0-9]*\).*/\1/' | grep -q '48;2;'
}

ui_5_the_git_signs_of_the_file_against_head_are_in_the_sign_column() {
  gitproject
  look flat
  CT=truecolor
  # line 1 of src/lib.rs changed, a line added at the end
  sed -i '1s/^/changed /' "$W/src/lib.rs"
  printf 'added line\n' >> "$W/src/lib.rs"
  start src/lib.rs
  reply '11;rgb:1e1e/1e1e/1e1e'
  wait_for "changed "
  # The first cell of the sign column: a dotted bar in amber for the changed line, a solid one in green
  # for the added (COLOR-3: two shapes, so that the colors are not needed to tell them apart).
  tries=100
  while ! screen | grep -F 'changed ' | grep -q '^┆' && [ "$tries" -gt 0 ]; do sleep 0.05; tries=$((tries - 1)); done
  screen | grep -F 'changed ' | grep -q '^┆'
  screen | grep -F 'added line' | grep -q '^▎'
  screen_esc | grep -a 'added line' | sed 's/▎.*//' | grep -q '38;2;[0-9;]*m' # a color on the bar
}

ui_10_the_status_line_is_a_bar_with_the_mode_the_branch_the_file_and_the_position() {
  gitproject
  sed -i '1s/^/changed /' "$W/src/lib.rs"
  look depth
  CT=truecolor
  ICONS_ON=1 # powerline's separators are icons (ICON-1): the choice `zi` remembers is on
  mkdir -p -m 700 "$XDG_STATE_HOME/sim" && echo on > "$XDG_STATE_HOME/sim/icons"
  start src/lib.rs
  reply '11;rgb:1e1e/1e1e/1e1e'
  branch=$(git -C "$W" symbolic-ref --short HEAD)
  sep=$(printf '\356\202\260') # U+E0B0, powerline's separator
  tries=100
  while ! screen | tail -1 | grep -q "$sep" && [ "$tries" -gt 0 ]; do sleep 0.05; tries=$((tries - 1)); done
  last() { screen | grep -v '^$' | tail -1; }
  last | grep -q "NORMAL"
  last | grep -q "$branch"
  last | grep -q "~1" # one line changed against HEAD
  last | grep -q "lib.rs"
  last | grep -q "utf-8"
  last | grep -q "LF"
  last | grep -q "Top"
  last | grep -q "1:1"
  # The block of the mode is a background (blue in Normal mode); a message takes the row over.
  screen_esc | tail -3 | grep -a -q '48;2;'
  typed zi
  wait_for "icons off"
  ! last | grep -q "NORMAL"
  typed j
  wait_for "NORMAL"
  typed i
  wait_for "INSERT"
  keys Escape
}

ui_4_every_open_file_has_a_tab_and_a_click_on_a_tab_shows_that_file() {
  project
  look flat
  CT=truecolor
  start README.md
  reply '11;rgb:1e1e/1e1e/1e1e'
  wait_for "README.md"
  typed ':e src/lib.rs'
  keys Enter
  wait_for "src/lib.rs" # the status line's bar names the file shown
  typed ':e src/main.rs'
  keys Enter
  tries=100
  while ! screen | sed -n 1p | grep -q 'main.rs' && [ "$tries" -gt 0 ]; do sleep 0.05; tries=$((tries - 1)); done
  last() { screen | grep -v '^$' | tail -1; }
  first=$(screen | sed -n 1p)
  printf '%s\n' "$first" | grep -q 'README.md'
  printf '%s\n' "$first" | grep -q 'lib.rs'
  printf '%s\n' "$first" | grep -q 'main.rs'
  printf '%s\n' "$first" | grep -q '▎' # the shown tab's indicator
  last | grep -q 'src/main.rs'
  # A click on the first tab (tmux's columns count from 1): README.md is shown.
  col=$(printf '%s\n' "$first" | awk '{ print index($0, "README.md") }')
  click "$col" 1
  tries=100
  while ! last | grep -q 'README.md' && [ "$tries" -gt 0 ]; do sleep 0.05; tries=$((tries - 1)); done
  last | grep -q 'README.md'
  screen | sed -n 1p | grep -q 'main.rs' # no tab is lost
}

ui_6_indent_guides_are_drawn_over_the_blanks_and_the_file_is_untouched() {
  printf 'fn a() {\n    if x {\n        y();\n    }\n}\n' > "$W/a.rs"
  look flat
  CT=truecolor
  start a.rs
  reply '11;rgb:1e1e/1e1e/1e1e'
  tries=100
  while ! screen | grep -q '│   │   y();' && [ "$tries" -gt 0 ]; do sleep 0.05; tries=$((tries - 1)); done
  screen | grep -q '│   │   y();'
  screen | grep -q '│   if x {'
  screen | grep -q '│   }'
  # What is on the screen is the file's: its blanks are the cursor's to move over and delete.
  typed jjwdd
  typed ':w'
  keys Enter
  wait_for "saved a.rs"
  [ "$(sed -n 3p "$W/a.rs")" = "    }" ]
}

ui_6_whitespace_marks_show_tabs_trailing_blanks_and_nothing_at_the_end_of_a_line() {
  printf 'a  \n\tb\n\n' > "$W/a.txt"
  look flat
  CT=truecolor
  start a.txt
  reply '11;rgb:1e1e/1e1e/1e1e'
  tries=100
  while ! screen | grep -q 'a··' && [ "$tries" -gt 0 ]; do sleep 0.05; tries=$((tries - 1)); done
  screen | grep -q 'a··' # trailing blanks
  screen | grep -q '»   b' # a tab: a chevron and blanks, no dashes
  ! screen | grep -q '↲' # nothing at the end of a line
  typed ':w'
  keys Enter
  wait_for "saved a.txt"
  [ "$(sed -n 1p "$W/a.txt")" = "a  " ] # the file is as it was
}

ui_7_a_todo_comment_has_a_badge_in_one_style_for_every_keyword() {
  printf 'fn a() {\n    // TODO: later\n    // FIX: now\n}\n' > "$W/a.rs"
  look flat
  CT=truecolor
  start a.rs
  reply '11;rgb:1e1e/1e1e/1e1e'
  wait_for "// FIX: now"
  tries=100
  while ! screen_esc | grep -a 'TODO:' | grep -q '1m.*48;2;' && [ "$tries" -gt 0 ]; do sleep 0.05; tries=$((tries - 1)); done
  # Bold on a surface, for both keywords: the same two sequences, whatever the word.
  todo=$(screen_esc | grep -a 'TODO:' | sed 's/.*\(\x1b\[1m[^T]*\)TODO:.*/\1/')
  fix=$(screen_esc | grep -a 'FIX:' | sed 's/.*\(\x1b\[1m[^F]*\)FIX:.*/\1/')
  [ -n "$todo" ] && [ "$todo" = "$fix" ]
  printf '%s\n' "$todo" | grep -a -q '48;2;'
}

ui_11_the_scrollbar_has_a_marker_for_the_cursor_and_follows_it() {
  i=1
  while [ "$i" -le 200 ]; do echo "line $i" >> "$W/a.txt"; i=$((i + 1)); done
  look flat
  CT=truecolor
  start a.txt
  reply '11;rgb:1e1e/1e1e/1e1e'
  wait_for "line 1"
  tries=100
  while ! screen | grep -q '[⎺⎻⎼⎽]' && [ "$tries" -gt 0 ]; do sleep 0.05; tries=$((tries - 1)); done
  at=$(screen | grep -n '[⎺⎻⎼⎽]' | head -1 | cut -d: -f1)
  [ "$at" -le 4 ] # near the top of the bar for the first line
  typed G
  wait_for "line 200"
  tries=100
  while [ "$(screen | grep -n '[⎺⎻⎼⎽]' | head -1 | cut -d: -f1)" -le 4 ] && [ "$tries" -gt 0 ]; do sleep 0.05; tries=$((tries - 1)); done
  [ "$(screen | grep -n '[⎺⎻⎼⎽]' | head -1 | cut -d: -f1)" -ge 18 ] # and near the bottom for the last
}

ui_13_a_yank_is_lit_for_a_moment() {
  printf 'alpha beta\nsecond\n' > "$W/a.txt"
  look flat
  CT=truecolor
  start a.txt
  reply '11;rgb:1e1e/1e1e/1e1e'
  wait_for "alpha beta"
  typed yw
  wait_for "chars yanked"
  # Lit: a background on `alpha `, in the accent's block, which is gone 150 ms later.
  lit() { screen_esc | grep -a 'alpha' | grep -q '48;2;[^a]*alpha'; } # (the scrollbar's handle is after the text)
  tries=40
  while lit && [ "$tries" -gt 0 ]; do sleep 0.05; tries=$((tries - 1)); done
  ! lit
}

pick_1_the_picker_floats_a_rounded_window_with_a_title_over_the_text() {
  project
  start src/main.rs
  wait_for "src/main.rs  1:1  NORMAL"
  keys C-f
  wait_for "Live grep"
  screen | grep -F -q "╭─ Live grep · " # a rounded border, titled with the mode and the root
  screen | grep -F -q "^n/^p move" # and the keys in the border below
  screen | grep -F -q "╰─ 0 / 0 " # and the count in the border below
  screen | grep -F -q "❯" # the prompt
  screen | head -n 1 | grep -F -q "fn main() {}" # the text above the window is as it was
  screen | tail -n 1 | grep -F -q "src/main.rs  1:1  NORMAL" # and the status line
  keys Escape
  wait_for "src/main.rs  1:1  NORMAL"
  gone "Live grep"
  keys C-p
  wait_for "Find files"
  screen | grep -F -q "╭─ Find files · "
}

pick_2_the_keys_of_the_picker_move_clear_and_open() {
  project
  start src/main.rs
  wait_for "src/main.rs  1:1  NORMAL"
  keys C-p
  wait_for "1 / 4" # the files the history likes first, then the rest
  typed lib
  wait_for "1 / 1"
  keys C-u
  wait_for "1 / 4"
  keys C-n C-n
  wait_for "3 / 4"
  keys C-p
  wait_for "2 / 4"
  keys Down Up Up
  wait_for "1 / 4"
  keys C-n
  keys Enter # the second file by the history, then the list: README.md
  wait_for "README.md  1:1  NORMAL  2/2"
  gone "Find files"
  [ "$(cat "$W/src/main.rs")" = "fn main() {}" ] # nothing was typed
}

pick_4_the_preview_has_the_syntax_colors_and_the_matching_line_is_lit() {
  grepproject
  look flat
  CT=truecolor
  start src/main.rs
  reply '11;rgb:1e1e/1e1e/1e1e'
  wait_for "fn main() {}"
  keys C-f
  typed needle
  wait_for "1 / 2"
  wait_for "the needle is here"
  # the pane's row of the matching line is on a background (the accent's block), and its match on another
  wait_esc '48;2;[0-9;]*m  60 the '
  keys Down
  wait_for "2 / 2"
  wait_for "   1 fn needle() {}"
  # `fn` is a keyword: in the pane, once the highlighter has answered, an escape sequence follows
  # the number's blank (plain text has none); the row has a border and no path
  wait_esc "$(printf '   1 \033')"
}

pick_5_the_count_and_a_spinner_while_the_search_runs() {
  project
  mkdir -p "$CASE_DIR/bin"
  printf '#!/bin/sh\nexec sleep 30\n' > "$CASE_DIR/bin/rg"
  chmod +x "$CASE_DIR/bin/rg"
  SIM_PATH="$CASE_DIR/bin:$PATH"
  start src/main.rs
  wait_for "src/main.rs  1:1  NORMAL"
  keys C-f
  typed f
  wait_for "0 / 0"
  screen | grep -E -q '⠋|⠙|⠹|⠸|⠼|⠴|⠦|⠧|⠇|⠏' # the spinner turns
  first=$(screen | grep -F '0 / 0')
  sleep 0.3
  [ "$(screen | grep -F '0 / 0')" != "$first" ] # and it has moved on
  keys Escape
  wait_for "src/main.rs  1:1  NORMAL"
}

ui_14_the_matches_of_a_search_are_lit_until_noh_or_esc() {
  printf 'alpha beta alpha\nsecond\n' > "$W/a.txt"
  look flat
  CT=truecolor
  start a.txt
  reply '11;rgb:1e1e/1e1e/1e1e'
  wait_for "alpha beta"
  # Lit: a background before an `alpha` (the scrollbar's handle is after the text)
  lit() { screen_esc | grep -a 'alpha' | grep -q '48;2;[^a]*alpha'; }
  # (the state changes when the screen does: wait for it to, and fail if it does not)
  becomes() { tries=60; while [ "$tries" -gt 0 ]; do if "$@"; then return 0; fi; sleep 0.05; tries=$((tries - 1)); done; return 1; }
  unlit() { ! lit; }
  unlit
  typed /alpha
  keys Enter
  becomes lit
  typed :noh
  keys Enter
  becomes unlit
  typed n
  becomes lit
  keys Escape
  becomes unlit
  [ "$(cat "$W/a.txt")" = "$(printf 'alpha beta alpha\nsecond')" ]
}

ui_8_a_table_a_done_task_and_a_break_are_drawn_as_they_are_ruled_and_in_the_secondary_gray() {
  printf '| a | b |\n|---|:--:|\n| 1 | 2 |\n\n- [x] finished\n- [ ] open\n\n---\n' > "$W/a.md"
  look flat
  CT=truecolor
  start a.md
  reply '11;rgb:1e1e/1e1e/1e1e'
  wait_for "│ a │ b │" # the pipes are bars
  screen | grep -F -q "├───┼:──:┤" # the delimiter row is a grid, the colons kept
  screen | grep -F -q "│ 1 │ 2 │"
  screen | grep -q '─────────────────────' # a break goes across the pane
  screen | grep -F -q -- "- [x] finished" # the file's chars are all there
  # a done task is the secondary gray (180), an open one is not
  wait_esc '38;2;180;180;180mfinished'
  ! screen_esc | grep -a 'open' | grep -q '38;2;180;180;180mopen'
  [ "$(sed -n 1p "$W/a.md")" = "| a | b |" ] # nothing was written
}

ui_15_a_hidden_char_is_the_alert_s_and_the_same_text_typed_is_not() {
  printf 'a\033b <U+202E> end\n' > "$W/h.txt"
  look flat
  CT=truecolor
  start h.txt
  reply '11;rgb:1e1e/1e1e/1e1e'
  wait_for "^[b <U+202E> end"
  line=$(screen_esc | grep -a 'end')
  printf '%s' "$line" | grep -aq '48;2;[0-9;]*m^\[' # the control char: on a block of the alert's color
  ! printf '%s' "$line" | grep -aq '38;2;230;230;230m^\[' # not the primary gray
  printf '%s' "$line" | grep -aq '\[49mb <U+202E> end' # the chars after it, and the same text typed, are plain
  # calm: reverse video, no hue
  typed zl
  wait_for "look: depth"
  typed zl
  wait_for "look: calm"
  screen_esc | grep -a 'end' | grep -aq '[;[]7[;m][^m]*m*^\['
}

# COLOR-2: with NO_COLOR set (and not empty) sim writes no color at all, not even where the terminal
# says it has truecolor, and does not ask the terminal for its colors (OSC 11 and 4); the interface
# is still drawn, in weight, underline and inverse video. What sim writes is read from the pane's
# output (pipe-pane), the pane is only let go once it is being recorded.
color_2_no_color_writes_none_and_asks_for_none() {
  project
  look depth
  mkdir -p -m 700 "$XDG_STATE_HOME/sim" && echo shown > "$XDG_STATE_HOME/sim/icons-hint" # (the hint is not what is looked at)
  cmd="cd $(shq "$W") && while [ ! -e $(shq "$CASE_DIR/go") ]; do sleep 0.02; done; env -u TERM_PROGRAM -u GHOSTTY_RESOURCES_DIR -u KITTY_PID -u WEZTERM_PANE HOME=$(shq "$HOME") XDG_STATE_HOME=$(shq "$XDG_STATE_HOME") COLORTERM=truecolor NO_COLOR=1 $(shq "$SIM_BIN") src/lib.rs; echo \$? > $(shq "$CASE_DIR/.exit_code")"
  tm new-session -d -s "$SESSION" -x "$COLS" -y "$ROWS" "exec sh -c $(shq "$cmd")" \; set-option -w -t "$SESSION" remain-on-exit on \; set-option -sg escape-time 0
  tm pipe-pane -O -t "$SESSION" "cat > $(shq "$CASE_DIR/out")"
  touch "$CASE_DIR/go"
  wait_for "NORMAL"
  typed "A // x"
  keys Escape
  wait_for "●" # modified
  typed "ggvl"
  wait_for "VISUAL"
  keys Escape
  typed ":q!"
  keys Enter
  wait_exit
  esc=$(printf '\033')
  if grep -a -F -q "$esc]11;?" "$CASE_DIR/out"; then fail "sim asked for the background color"; fi
  if grep -a -F -q "$esc]4;" "$CASE_DIR/out"; then fail "sim asked for the colors of the palette"; fi
  # every SGR sequence it wrote has weight, underline and inverse only: no 30-38, 40-48, 90-97, 100-107
  LC_ALL=C grep -a -o "$esc\[[0-9;]*m" "$CASE_DIR/out" > "$CASE_DIR/sgr"
  [ -s "$CASE_DIR/sgr" ]
  if grep -E '(\[|;)(3[0-8]|4[0-8]|9[0-7]|10[0-7])(;|m)' "$CASE_DIR/sgr" > /dev/null; then
    fail "a color was written: $(grep -E '(\[|;)(3[0-8]|4[0-8]|9[0-7]|10[0-7])(;|m)' "$CASE_DIR/sgr" | head -3 | tr '\033' E)"
  fi
  grep -F -q "[0;7m" "$CASE_DIR/sgr" || grep -F -q ";7m" "$CASE_DIR/sgr" # inverse video is there
}

# COLOR-2: a terminal of fewer than 8 colors is the same: TERM=vt100 is no color at all.
color_2_a_terminal_that_cannot_do_8_colors_gets_none_either() {
  project
  look depth
  mkdir -p -m 700 "$XDG_STATE_HOME/sim" && echo shown > "$XDG_STATE_HOME/sim/icons-hint"
  cmd="cd $(shq "$W") && while [ ! -e $(shq "$CASE_DIR/go") ]; do sleep 0.02; done; env -u TERM_PROGRAM -u GHOSTTY_RESOURCES_DIR -u KITTY_PID -u WEZTERM_PANE -u NO_COLOR HOME=$(shq "$HOME") XDG_STATE_HOME=$(shq "$XDG_STATE_HOME") COLORTERM=truecolor TERM=vt100 $(shq "$SIM_BIN") src/lib.rs; echo \$? > $(shq "$CASE_DIR/.exit_code")"
  tm new-session -d -s "$SESSION" -x "$COLS" -y "$ROWS" "exec sh -c $(shq "$cmd")" \; set-option -w -t "$SESSION" remain-on-exit on \; set-option -sg escape-time 0
  tm pipe-pane -O -t "$SESSION" "cat > $(shq "$CASE_DIR/out")"
  touch "$CASE_DIR/go"
  wait_for "NORMAL"
  typed ":q"
  keys Enter
  wait_exit
  esc=$(printf '\033')
  if grep -a -F -q "$esc]11;?" "$CASE_DIR/out"; then fail "sim asked for the background color"; fi
  LC_ALL=C grep -a -o "$esc\[[0-9;]*m" "$CASE_DIR/out" > "$CASE_DIR/sgr"
  if grep -E '(\[|;)(3[0-8]|4[0-8]|9[0-7]|10[0-7])(;|m)' "$CASE_DIR/sgr" > /dev/null; then fail "a color was written"; fi
}

# ICON-1: in a terminal that is not known to bring Nerd Font symbols the icons start off, and the status
# line says once how to turn them on; what `zi` chose last wins over what the terminal is known for.
icon_1_the_icons_start_off_with_a_hint_once_and_the_remembered_choice_wins() {
  project
  look depth
  CT=truecolor
  HINTED=1 # (this case wants to see the hint)
  start src/lib.rs
  reply '11;rgb:1e1e/1e1e/1e1e'
  wait_for "zi: icons (needs a Nerd Font)"
  typed j
  wait_for "NORMAL  src/lib.rs" # a key puts it out (and no screen so far had an icon in it)
  typed ":q"
  keys Enter
  wait_exit
  [ -e "$XDG_STATE_HOME/sim/icons-hint" ]
  restart
  start src/lib.rs
  reply '11;rgb:1e1e/1e1e/1e1e'
  wait_for "NORMAL  src/lib.rs"
  sleep 0.3
  if screen | grep -F -q "Nerd Font"; then fail "the hint came twice"; fi
  typed ":q"
  keys Enter
  wait_exit
  restart
  # a terminal known to bring the symbols: on at once, and `zi` off is remembered over the terminal
  rm -f "$XDG_STATE_HOME/sim/icons-hint"
  TP=ghostty
  ICONS_ON=1
  start
  reply '11;rgb:1e1e/1e1e/1e1e'
  closed=$(printf '\357\201\273') # U+F07B, the closed folder of Nerd Fonts
  wait_for "src" # (a name with its icon has no slash)
  screen | grep -F 'src' | grep -q "$closed"
  if screen | grep -F -q "Nerd Font"; then fail "a hint where the icons are on"; fi
  [ ! -e "$XDG_STATE_HOME/sim/icons" ] # (nothing was chosen)
  typed jjj # README.md
  keys Enter
  wait_for "NORMAL"
  typed "zi"
  wait_for "icons off"
  [ "$(cat "$XDG_STATE_HOME/sim/icons")" = off ]
  typed ":qa"
  keys Enter
  wait_exit
  restart
  ICONS_ON=
  start
  reply '11;rgb:1e1e/1e1e/1e1e'
  wait_for "src/" # (and wait_for sees no icon: the remembered off wins over ghostty)
  if screen | grep -F 'src' | grep -q "$closed"; then fail "ghostty beat what zi chose"; fi
}

# CLI-7: `sim --version` and `sim --help` print and exit 0, an unknown flag prints the usage to stderr and
# exits 2, and none of them writes an escape sequence (no raw mode, no query).
cli_7_version_and_help_print_without_touching_the_terminal() {
  out=$("$SIM_BIN" --version)
  case $out in
    "sim "[0-9]*) ;;
    *) fail "--version printed: $out" ;;
  esac
  out=$("$SIM_BIN" --help)
  case $out in
    *"the next-generation Vim"*"--plain"*"NO_COLOR"*"SIM_ON_NORMAL"*":help or F1"*) ;;
    *) fail "--help printed: $out" ;;
  esac
  if "$SIM_BIN" --version --help | grep -a -q "$(printf '\033')"; then fail "an escape sequence"; fi
  rc=0
  "$SIM_BIN" --bogus > "$CASE_DIR/stdout" 2> "$CASE_DIR/stderr" || rc=$?
  [ "$rc" -eq 2 ]
  [ ! -s "$CASE_DIR/stdout" ]
  grep -F -q "usage: sim" "$CASE_DIR/stderr"
}

# KEY-3: a pause on a prefix shows which-key near the bottom, and Esc closes it.
key_3_a_pause_on_a_prefix_shows_which_key_and_esc_closes_it() {
  project
  look depth
  CT=truecolor
  start src/lib.rs
  reply '11;rgb:1e1e/1e1e/1e1e'
  wait_for "NORMAL"
  cursor() { tm display-message -p -t "$SESSION" '#{cursor_x},#{cursor_y},#{cursor_flag}'; }
  typed "3j2l"
  wait_for "1:3" # the cursor is in the text, not at its start
  at=$(cursor)
  typed " "
  wait_for "buffers" # after 200 ms
  screen | grep -F -q "╭─ <Space> "
  [ "$(cursor)" = "$at" ] # the terminal's cursor stays in the text: the window does not take it
  screen_esc | grep -E -q '48;2;[0-9];[0-9];[0-9]m' && return 1 # and no dark strip beside it that looks like a second one
  keys Escape
  wait_gone "buffers"
  typed "d"
  wait_for "delete lines"
  typed "w" # typing on is what it is without the window: `dw` takes a word
  wait_gone "delete lines"
  wait_for "lib() {}"
  gone "fn lib"
}

# KEY-4: `:help` and F1 open the key table as a picker; `tab` finds the keys that move between files.
key_4_help_opens_the_key_table_and_typing_tab_finds_the_tab_keys() {
  project
  start src/lib.rs
  wait_for "src/lib.rs  1:1  NORMAL"
  typed ":help"
  keys Enter
  wait_for "Help · keys and commands"
  typed tab
  wait_for "<S-Tab>"
  screen | grep -F -q "<Space>bp"
  screen | grep -F -q "Shows the next open file" # the longer text of the best entry
  keys Escape
  wait_gone "Help · keys and commands"
  [ "$(cat "$W/src/lib.rs")" = "fn lib() {}" ] # it ran nothing
  keys F1
  wait_for "Help · keys and commands"
  keys Escape
}

# KEY-5: where there is no tab bar the letters of `<Space>bp` are on the status line, and the letter shows its file.
key_5_space_b_p_puts_a_letter_on_each_file_and_the_letter_shows_it() {
  project
  start src/lib.rs
  wait_for "src/lib.rs  1:1  NORMAL"
  typed ":e src/main.rs"
  keys Enter
  wait_for "src/main.rs  1:1  NORMAL"
  typed " bp"
  wait_for "pick: l lib.rs · m main.rs"
  typed l
  wait_for "src/lib.rs  1:1  NORMAL"
}

# KEY-6: the first time a second file is open the message line says how to move between files, until Tab has
# been used three times, and the uses are kept in the state directory.
key_6_the_hint_for_tab_comes_with_the_second_file_until_it_is_used_three_times() {
  project
  HINT_TAB=1
  start src/lib.rs
  wait_for "src/lib.rs  1:1  NORMAL"
  typed ":e src/main.rs"
  keys Enter
  wait_for "Tab / S-Tab: next / previous file"
  typed j
  wait_gone "Tab / S-Tab"
  keys Tab Tab Tab
  wait_content "$XDG_STATE_HOME/sim/tab-uses" 3
  typed ":qa"
  keys Enter
  wait_exit
  restart
  start src/lib.rs
  wait_for "src/lib.rs  1:1  NORMAL"
  typed ":e src/main.rs"
  keys Enter
  wait_for "src/main.rs  1:1  NORMAL"
  sleep 0.3
  gone "Tab / S-Tab" # learned: never again
}

# UI-17: while the matches of a search are lit the status line says which one the cursor is at.
ui_17_the_status_line_counts_the_matches_of_the_lit_search() {
  printf 'foo bar foo\nbaz foo\n' > "$W/a.txt"
  start a.txt
  wait_for "a.txt  1:1  NORMAL"
  typed "/foo"
  keys Enter
  wait_for "[2/3]"
  typed n
  wait_for "[3/3]"
  keys Escape
  wait_gone "[3/3]"
}

# UI-16: the boxes' titles have no numbers, and the editor's border no longer repeats the file's name.
ui_16_the_boxes_have_no_numbers_and_the_editor_s_border_is_plain() {
  project
  look depth
  CT=truecolor
  start src/lib.rs
  reply '11;rgb:1e1e/1e1e/1e1e'
  wait_for "NORMAL"
  keys C-n
  wait_for "files"
  gone "¹"
  gone "²"
  top=$(screen | sed -n 1p)
  case $top in
    *"lib.rs"*) fail "the editor's border has the file's name: $top" ;;
  esac
}

# UI-5, MARK-1: a file named `*` is that file for git, not every file: the sign column's diff does not make
# git enter a submodule and run what the submodule's own config names.
ui_5_a_file_named_star_does_not_make_git_run_a_submodule_s_own_commands() {
  g() { GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_SYSTEM=/dev/null git "$@"; }
  id='-c user.name=sim -c user.email=sim@example.com'
  src="$CASE_DIR/src"
  mkdir -p "$src"
  g -C "$src" init -q
  printf 'one\n' > "$src/f"
  g -C "$src" add f
  g -C "$src" $id commit -q -m 1
  printf 'two\n' > "$src/f"
  g -C "$src" $id commit -q -a -m 2
  g -C "$W" init -q
  g -C "$W" -c protocol.file.allow=always submodule add -q "$src" sub
  g -C "$W/sub" checkout -q HEAD~1
  printf 'star\n' > "$W/*"
  g -C "$W" add -A
  g -C "$W" $id commit -q -m top
  g -C "$W/sub" checkout -q - # the submodule's commit is not the one the repository records
  printf '#!/bin/sh\ntouch "%s/witness"\n' "$CASE_DIR" > "$CASE_DIR/ext.sh"
  chmod +x "$CASE_DIR/ext.sh"
  g config -f "$W/.git/modules/sub/config" diff.external "$CASE_DIR/ext.sh"
  g -C "$W" config diff.submodule diff
  printf 'star\nchanged\n' > "$W/*"
  # the control: git as sim ran the diff before makes the submodule's own external diff run
  g -C "$W" -c core.fsmonitor=false --no-optional-locks diff --no-ext-diff --no-textconv --no-color -U0 HEAD -- '*' > /dev/null
  [ -e "$CASE_DIR/witness" ]
  rm -f "$CASE_DIR/witness"
  start '*'
  wait_for "*  1:1  NORMAL"
  sleep 1 # the diff is asked for in a thread of its own
  [ ! -e "$CASE_DIR/witness" ]
}

# GIT-2: a monitor, a filter and a hook of the repository's own never run, and git is not skipped for them.
git_2_a_monitor_a_filter_and_a_hook_of_the_repository_never_run() {
  gitproject
  printf '#!/bin/sh\ntouch "%s/witness-monitor"\n' "$CASE_DIR" > "$W/mon.sh"
  printf '#!/bin/sh\ntouch "%s/witness-filter"\ncat\n' "$CASE_DIR" > "$W/filt.sh"
  mkdir -p "$W/hooks"
  printf '#!/bin/sh\ntouch "%s/witness-hook"\n' "$CASE_DIR" > "$W/hooks/post-index-change"
  chmod +x "$W/mon.sh" "$W/filt.sh" "$W/hooks/post-index-change"
  git -C "$W" config core.fsmonitor "$W/mon.sh"
  git -C "$W" config filter.canary.clean "$W/filt.sh"
  git -C "$W" config core.hooksPath "$W/hooks"
  printf '* filter=canary\n' > "$W/.gitattributes"
  printf 'more\n' >> "$W/docs/guide.md"
  touch "$W/src/lib.rs" "$W/src/main.rs"
  # the control: git as the user runs it asks the monitor and runs the filter
  git -C "$W" status --porcelain > /dev/null
  [ -e "$CASE_DIR/witness-monitor" ] && [ -e "$CASE_DIR/witness-filter" ]
  rm -f "$CASE_DIR"/witness-*
  touch "$W/src/lib.rs" "$W/src/main.rs"
  start
  wait_for "▸ docs/ ~" # the marks are there: git ran
  gone "git skipped"
  [ ! -e "$CASE_DIR/witness-monitor" ] && [ ! -e "$CASE_DIR/witness-filter" ] && [ ! -e "$CASE_DIR/witness-hook" ]
}

# GIT-1: the directory that core.hooksPath is pointed to is in the state directory, private and empty.
git_1_the_hooks_directory_is_private_and_empty_in_the_state_directory() {
  gitproject
  start
  wait_for "▸ src/"
  dir="$XDG_STATE_HOME/sim/no-hooks"
  [ -d "$dir" ] && [ -z "$(ls -A "$dir")" ]
  ls -ld "$dir" | grep -q '^drwx------'
}

# DIFF-1, DIFF-2: <Space>ge shows only what differs from HEAD, with letters and numbers; again, it goes back.
diff_1_space_g_e_shows_what_differs_from_head_and_space_g_e_again_goes_back() {
  gitproject
  printf 'one\ntwo\n' >> "$W/src/lib.rs"
  printf 'x\ny\nz\n' > "$W/docs/new.md"
  rm "$W/src/main.rs"
  start
  wait_for "▸ src/ ~"
  typed " ge"
  wait_for "CHANGES"
  wait_for "new.md"
  screen | grep -F -q "+3"
  screen | grep -F -q "lib.rs"
  screen | grep -F -q "+2"
  screen | grep -F -q "−1"
  gone "guide.md"
  gone "README.md"
  typed " ge"
  wait_gone "CHANGES"
  wait_for "TREE"
  typed " ge"
  wait_for "CHANGES"
  typed q
  wait_for "TREE"
  screen | grep -F -q "▸ src/"
}

# DIFF-3: Enter on a deleted file opens HEAD's version, read-only.
diff_3_enter_on_a_deleted_file_opens_head_s_version_read_only() {
  gitproject
  rm "$W/src/main.rs"
  start
  wait_for "▸ src/ ~"
  typed " ge"
  wait_for "main.rs"
  typed j
  keys Enter
  wait_for "HEAD:src/main.rs (read-only)"
  wait_for "fn main() {}"
  typed dd
  screen | grep -F -q "fn main() {}"
  typed ":w"
  keys Enter
  wait_for "read-only"
  [ ! -e "$W/src/main.rs" ] && [ ! -e "$W/HEAD:src/main.rs" ]
}

# DIFF-5: in plain mode the key is refused and says why.
diff_5_in_plain_mode_space_g_e_is_refused_naming_plain_mode() {
  gitproject
  start --plain src/lib.rs
  wait_for "src/lib.rs  1:1  NORMAL"
  typed " ge"
  wait_for "plain mode runs no program: no changes view"
  gone "CHANGES"
}

# DIFF-6: the pause after <Space>g lists the key, and :help finds it.
diff_6_which_key_and_help_list_space_g_e() {
  gitproject
  start src/lib.rs
  wait_for "src/lib.rs  1:1  NORMAL"
  typed " g"
  wait_for "changes from HEAD"
  keys Escape
  wait_gone "changes from HEAD"
  typed ":help"
  keys Enter
  wait_for "Help · keys and commands"
  typed "HEAD"
  wait_for "<Space>ge"
}

main "$@"
