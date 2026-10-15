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
#   colors             print the screen with its escape sequences (tmux gives colors as 38;2;R;G;B)
#   reply BODY         type a terminal's answer to si's OSC 11 query, e.g. reply rgb:1e1e/1e1e/1e1e
#   wait_colors TEXT   wait until `colors` contains TEXT (fixed string)
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
colors() { tm capture-pane -p -e -t "$SESSION"; }
# A detached tmux has no terminal to ask for its background, so the answer is typed into the pane.
reply() { tm send-keys -t "$SESSION" -l -- "$(printf '\033]11;%s\033\\' "$1")"; }

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

wait_colors() {
  tries=100
  while [ "$tries" -gt 0 ]; do
    if colors 2> /dev/null | grep -F -q -- "$1"; then return 0; fi
    sleep 0.05
    tries=$((tries - 1))
  done
  echo "timed out waiting for the colors to contain: $1" >&2
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
        sock=$(tm display-message -p "#{socket_path}" 2> /dev/null || true)
        tm kill-server 2> /dev/null || true
        # a killed server leaves its socket file behind: remove it, or /tmp/tmux-UID fills up
        rm -f "${sock:-${TMUX_TMPDIR:-/tmp}/tmux-$(id -u)/$SOCKET}"
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
  [ -z "$(ls -A "$XDG_STATE_HOME/si" | grep -v '^positions$')" ]
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

safe_6_a_file_changed_meanwhile_is_merged_with_the_snapshot() {
  printf 'one\ntwo\nthree\n' > "$CASE_DIR/f"
  wrap
  edit_and_kill_9 f
  printf 'one\ntwo\nTHREE\n' > "$CASE_DIR/f"
  start f
  wait_for "recovered changes from"
  wait_for "merged with disk"
  screen | grep -F -q "  2 TWO"
  screen | grep -F -q "~  3 THREE"
  printf 'one\ntwo\nTHREE\n' | cmp - "$CASE_DIR/f"
  typed ":q!"
  keys Enter
  wait_for "exit=0"
  # the stale snapshot went with the one of this session
  [ -z "$(ls -A "$XDG_STATE_HOME/si" | grep -v '^positions$')" ]
}

# Another process rewrites the open file the way an AI does: a new file moved over the old one.
prop_1_an_external_edit_appears_and_is_marked() {
  printf 'one\ntwo\nthree\n' > "$CASE_DIR/f"
  start f
  wait_for "f  1:1  NORMAL"
  printf 'one\nTWO\nthree\n' > "$CASE_DIR/f.new"
  t0=$(now_ms)
  mv "$CASE_DIR/f.new" "$CASE_DIR/f"
  wait_for "1 change on disk"
  t1=$(now_ms)
  # The 30 ms debounce, the read, the merge and the draw, plus tmux's own round trip and the
  # 50 ms steps of wait_for: about 55 ms measured, 100 ms in PROP-1, and slack for a busy machine.
  echo "rename to marked: $((t1 - t0)) ms"
  [ $((t1 - t0)) -le 200 ]
  screen | grep -F -q "~  2 TWO"
  gone modified
  # PROP-9: the mark stays until the cursor enters its line.
  typed j
  wait_for "   2 TWO"
  gone "1 change on disk"
}

prop_2_a_conflicting_external_edit_becomes_a_proposal_and_loses_nothing() {
  printf 'one\ntwo\nthree\n' > "$CASE_DIR/f"
  start f
  wait_for "f  1:1  NORMAL"
  typed "jcwMINE"
  keys Escape
  wait_for "modified"
  printf 'one\ntwo!\nthree\n' > "$CASE_DIR/f.new"
  mv "$CASE_DIR/f.new" "$CASE_DIR/f"
  wait_for "1 proposal"
  screen | grep -F -q "~  2 MINE"
  printf 'one\ntwo!\nthree\n' | cmp - "$CASE_DIR/f" # si wrote nothing
}

# Mine has three edits and the file is rewritten under them: three proposals. Going through them
# with ]c, y, n and u, :w is refused until they are decided, and :w! keeps mine.
prop_8_review_accepts_and_rejects_hunks() {
  printf 'a1\nb\na2\nc\na3\n' > "$CASE_DIR/f"
  start f
  wait_for "f  1:1  NORMAL"
  typed "rAjjrAjjrAgg"
  wait_for "modified"
  printf 'X1\nb\nX2\nc\nX3\n' > "$CASE_DIR/f.new"
  mv "$CASE_DIR/f.new" "$CASE_DIR/f"
  wait_for "3 proposals"
  # :w is refused and writes nothing.
  typed ":w"
  keys Enter
  wait_for "unresolved conflicts with the disk: ]c to review, :w! keeps yours"
  printf 'X1\nb\nX2\nc\nX3\n' | cmp - "$CASE_DIR/f"
  # ]c starts REVIEW on the first proposal: mine and theirs are woven into one line.
  typed "]c"
  wait_for "f  1:1  REVIEW 1/3  y accept · n reject · Y all · N none · Esc back"
  screen | grep -F -q "~  1 A1X1"
  screen | grep -F -q "   2 b"
  # y accepts it and goes on to the next; n rejects that one.
  typed y
  wait_for "REVIEW 1/2"
  screen | grep -F -q "   1 X1"
  typed n
  wait_for "REVIEW 1/1"
  screen | grep -F -q "   3 A2"
  # u leaves REVIEW and brings the rejected proposal back.
  typed u
  wait_for "2 proposals"
  gone REVIEW
  typed ":w"
  keys Enter
  wait_for "unresolved conflicts with the disk"
  printf 'X1\nb\nX2\nc\nX3\n' | cmp - "$CASE_DIR/f"
  # :w! saves mine as it is (X1 accepted, the others mine) and settles the rest.
  typed ":w!"
  keys Enter
  wait_for "saved f"
  printf 'X1\nb\nA2\nc\nA3\n' | cmp - "$CASE_DIR/f"
  gone "proposal"
}

# Y accepts every proposal, and then :w works like any save.
prop_8_capital_y_accepts_all_and_the_file_can_be_saved() {
  printf 'a1\nb\na2\n' > "$CASE_DIR/f"
  start f
  wait_for "f  1:1  NORMAL"
  typed "rAjjrA"
  wait_for "modified"
  printf 'X1\nb\nX2\n' > "$CASE_DIR/f.new"
  mv "$CASE_DIR/f.new" "$CASE_DIR/f"
  wait_for "2 proposals"
  typed "ggjjgg]cY"
  wait_for "2 proposals accepted"
  typed ":w"
  keys Enter
  wait_for "saved f"
  printf 'X1\nb\nX2\n' | cmp - "$CASE_DIR/f"
}

# Quitting with a proposal open is refused; :q! discards it.
safe_7_quit_is_refused_with_open_proposals() {
  printf 'a1\nb\n' > "$CASE_DIR/f"
  start f
  wait_for "f  1:1  NORMAL"
  typed "rA"
  wait_for "modified"
  printf 'X1\nb\n' > "$CASE_DIR/f.new"
  mv "$CASE_DIR/f.new" "$CASE_DIR/f"
  wait_for "1 proposal"
  typed "q"
  wait_for "unresolved proposals: ]c to review, :q! discards"
  typed ":q"
  keys Enter
  wait_for "unresolved proposals: ]c to review, :q! discards"
  typed "ZZ"
  wait_for "unresolved conflicts with the disk: ]c to review, :w! keeps yours"
  [ ! -s "$CASE_DIR/.exit_code" ] # still running
  printf 'X1\nb\n' | cmp - "$CASE_DIR/f"
  typed ":q!"
  keys Enter
  wait_exit
  [ "$(exit_code)" = 0 ]
  printf 'X1\nb\n' | cmp - "$CASE_DIR/f"
}

prop_1_a_deleted_file_is_told_and_save_makes_it_again() {
  printf 'one\n' > "$CASE_DIR/f"
  start f
  wait_for "f  1:1  NORMAL"
  rm "$CASE_DIR/f"
  wait_for "modified · deleted on disk"
  typed ":w"
  keys Enter
  wait_for "saved f"
  printf 'one\n' | cmp - "$CASE_DIR/f"
  gone "deleted on disk"
}

# The other process saves while we have unsaved changes, and we save right after. Typing the Enter
# after the rename wins against the 30 ms debounce of the watcher, so the save usually finds the
# file changed (SAFE-5); if the watcher is quicker, the change is merged first. Both must end
# with both edits in the file.
safe_5_external_change_before_save_is_merged_not_overwritten() {
  printf 'one\ntwo\nthree\n' > "$CASE_DIR/f"
  start f
  wait_for "f  1:1  NORMAL"
  typed "cwONE"
  keys Escape
  wait_for "modified"
  typed ":w"
  printf 'one\ntwo\nTHREE\n' > "$CASE_DIR/f.new"
  mv "$CASE_DIR/f.new" "$CASE_DIR/f"
  keys Enter
  wait_for "THREE"
  # Nothing was overwritten: the other process's edit is there, and so is ours on screen.
  grep -q THREE "$CASE_DIR/f"
  screen | grep -F -q "  1 ONE"
  typed ":w"
  keys Enter
  wait_for "saved f"
  printf 'ONE\ntwo\nTHREE\n' | cmp - "$CASE_DIR/f"
}

safe_6_a_snapshot_that_cannot_be_written_is_told() {
  printf 'one\n' > "$CASE_DIR/f"
  # The state directory cannot be made: a file stands where it should be.
  rm -rf "$XDG_STATE_HOME"
  : > "$XDG_STATE_HOME"
  start f
  wait_for "f  1:1  NORMAL"
  typed x
  wait_for "cannot write snapshots"
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

cli_5_the_intro_shows_until_the_first_key() {
  start
  wait_for "A vi for reading, where only the changes light up."
  for k in "si 0." "i / Esc" "Ctrl-s" "ZZ" "ZQ" "]c then y / n"; do
    screen | grep -F -q -- "$k"
  done
  [ "$(screen | grep -c .)" -le 12 ]
  typed l
  tries=100
  while screen | grep -F -q "A vi for reading"; do
    tries=$((tries - 1))
    [ "$tries" -gt 0 ] || { echo "the intro is still there" >&2; return 1; }
    sleep 0.05
  done
  screen | grep -F -q "[no name]  1:1  NORMAL"
  # With a file there is none.
  restart
  printf 'one\n' > "$CASE_DIR/f"
  start f
  wait_for "f  1:1  NORMAL"
  gone "A vi for reading"
}

# Without the kitty protocol (tmux does not pass it on) Esc and the next key arrive in one write,
# which looks like Alt+key: si reads it as Esc, then the key.
vi_9_esc_and_the_next_key_in_one_write_are_two_keys() {
  printf 'one\n' > "$CASE_DIR/f"
  start f
  wait_for "f  1:1  NORMAL"
  typed ix
  wait_for INSERT
  keys Escape :
  typed wq
  keys Enter
  wait_exit
  [ "$(exit_code)" = 0 ]
  printf 'xone\n' | cmp - "$CASE_DIR/f"
}

vi_1_japanese_words_split_where_the_script_changes() {
  printf '日本語のテスト abc\n' > "$CASE_DIR/f"
  start f
  wait_for "f  1:1  NORMAL"
  typed dw
  wait_for "  1 のテスト abc"
  typed 2dw
  wait_for "  1 abc"
  typed ":wq"
  keys Enter
  wait_exit
  printf 'abc\n' | cmp - "$CASE_DIR/f"
}

# The cell under tmux's cursor.
cursor_char() {
  x=$(tm display-message -p -t "$SESSION" '#{cursor_x}')
  y=$(tm display-message -p -t "$SESSION" '#{cursor_y}')
  screen | sed -n "$((y + 1))p" | cut -c"$((x + 1))"
}

# One line of 1,700 chars wraps into more rows than the screen has: the cursor is still on its char.
vis_12_the_cursor_is_visible_in_a_line_taller_than_the_screen() {
  i=0
  while [ "$i" -lt 600 ]; do printf 'word '; i=$((i + 1)); done > "$CASE_DIR/a.md"
  printf 'END\n' >> "$CASE_DIR/a.md"
  start a.md
  wait_for "a.md  1:1  NORMAL"
  typed '$'
  wait_for "a.md  1:3003"
  [ "$(cursor_char)" = D ]
  typed 70h
  wait_for "a.md  1:2933"
  [ "$(cursor_char)" = r ]
  typed 0
  wait_for "a.md  1:1  NORMAL"
  [ "$(cursor_char)" = w ]
}

safe_6_without_a_state_directory_snapshots_are_off_and_it_says_so() {
  printf 'one\ntwo\n' > "$CASE_DIR/f"
  ln "$CASE_DIR/f" "$CASE_DIR/g" # two links: a save goes in place, and needs a recovery copy
  real=$SI_BIN
  SI_BIN=$CASE_DIR/nostate.sh
  printf '#!/bin/sh\nunset HOME XDG_STATE_HOME\nexec %s "$@"\n' "$(shq "$real")" > "$SI_BIN"
  chmod +x "$SI_BIN"
  start f
  wait_for "f  1:1  NORMAL"
  typed x
  wait_for "cannot write snapshots: no state directory"
  # It says so once: the next ticks stay quiet.
  typed jx
  sleep 2.5
  gone "cannot write snapshots"
  typed ":w"
  keys Enter
  wait_for "no recovery copy possible"
  printf 'one\ntwo\n' | cmp - "$CASE_DIR/f"
}

# How many processes run `sleep N` (their command line starts with it; the shell that started them does not).
alive() { ps -A -o args= | grep -c "^sleep $1" || true; }

# `sleep` is started twice in one process group (one in the background): Ctrl-c on the filter stops
# both, and si goes on, with the text as it was. A number only this case uses names the sleeps.
prop_5_a_filter_runs_while_you_edit_and_can_be_cancelled() {
  printf 'one\ntwo\nthree\n' > "$CASE_DIR/f"
  start f
  wait_for "f  1:1  NORMAL"
  n=4$$
  typed ":%!sleep $n & sleep $n"
  keys Enter
  wait_for "filter 0s · Ctrl-c cancel"
  [ "$(alive "$n")" = 2 ]
  # Editing and moving go on meanwhile, and the seconds are counted.
  typed "jx"
  wait_for "modified · filter"
  wait_for "filter 2s · Ctrl-c cancel" 5
  screen | grep -F -q "   2 wo"
  keys C-c
  wait_for "filter cancelled"
  gone "Ctrl-c cancel"
  tries=100
  while [ "$(alive "$n")" != 0 ] && [ "$tries" -gt 0 ]; do
    sleep 0.05
    tries=$((tries - 1))
  done
  [ "$(alive "$n")" = 0 ]
  # Nothing came of it but the edit: the text is mine, and the file is as it was.
  screen | grep -F -q "   2 wo"
  gone "proposal"
  printf 'one\ntwo\nthree\n' | cmp - "$CASE_DIR/f"
  # Ctrl-c with nothing to cancel says what it always said.
  keys C-c
  wait_for "q to quit"
}

# Upper-casing the letters of a range comes back as three proposals, reviewed with ]c y n.
prop_6_a_filter_result_becomes_proposals() {
  printf 'one\n1\ntwo\n2\nthree\n' > "$CASE_DIR/f"
  start f
  wait_for "f  1:1  NORMAL"
  typed ":%!tr a-z A-Z"
  keys Enter
  wait_for "3 proposals from the filter"
  gone modified
  screen | grep -F -q "~  1 one"
  typed ":q"
  keys Enter
  wait_for "unresolved proposals: ]c to review, :q! discards"
  typed "]c"
  wait_for "f  1:1  REVIEW 1/3  y accept"
  screen | grep -F -q "~  1 oneONE"
  typed y
  wait_for "REVIEW 1/2"
  screen | grep -F -q "   1 ONE"
  typed n
  wait_for "REVIEW 1/1"
  typed y
  wait_for "1 proposal accepted"
  screen | grep -F -q "   5 THREE"
  screen | grep -F -q "   3 two" # rejected
  # The file is untouched until the user saves.
  printf 'one\n1\ntwo\n2\nthree\n' | cmp - "$CASE_DIR/f"
  typed ":w"
  keys Enter
  wait_for "saved f"
  printf 'ONE\n1\ntwo\n2\nTHREE\n' | cmp - "$CASE_DIR/f"
}

# A filter that fails shows the first line of its stderr and changes nothing.
prop_6_a_failing_filter_shows_its_stderr() {
  printf 'one\ntwo\n' > "$CASE_DIR/f"
  start f
  wait_for "f  1:1  NORMAL"
  typed ":%!echo boom >&2; echo ignored; exit 3"
  keys Enter
  wait_for "boom"
  gone "proposal"
  gone modified
  typed ":%!nosuchcommand_si_e2e"
  keys Enter
  wait_for "nosuchcommand_si_e2e"
  gone "proposal"
}

ai_1_ref_copies_the_location() {
  printf 'one\ntwo\nthree\nfour\n' > "$CASE_DIR/f"
  tm_clip() { tm show-buffer 2> /dev/null; }
  start f
  wait_for "f  1:1  NORMAL"
  tm set-option -sg set-clipboard on
  typed ":2,3ref"
  keys Enter
  wait_for "copied f:2-3"
  want_clip "f:2-3"
  typed "jVjj:ref"
  keys Enter
  wait_for "copied f:2-4"
  want_clip "f:2-4"
  typed "gg:ref"
  keys Enter
  wait_for "copied f:1-1"
  want_clip "f:1-1"
  # A file outside the current directory is given by its absolute path.
  restart
  outside=$(mktemp -d "${TMPDIR:-/tmp}/si-e2e-out.XXXXXX")
  printf 'x\ny\n' > "$outside/g"
  start "$outside/g"
  wait_for "g  1:1  NORMAL"
  tm set-option -sg set-clipboard on
  typed ":ref"
  keys Enter
  wait_for "copied $outside/g:1-1"
  want_clip "$outside/g:1-1"
  rm -rf "$outside"
}

# Waits until tmux's paste buffer holds TEXT.
want_clip() {
  tries=100
  until [ "$(tm show-buffer 2> /dev/null)" = "$1" ]; do
    tries=$((tries - 1))
    [ "$tries" -gt 0 ] || { echo "tmux's buffer holds: $(tm show-buffer 2>&1), not: $1" >&2; return 1; }
    sleep 0.05
  done
}

# The hook runs at the start and at every entry into Normal mode, leaves no zombie, and a failing
# one is silent.
vi_11_on_normal_runs_the_hook() {
  printf 'one\n' > "$CASE_DIR/f"
  real=$SI_BIN
  wrap "SI_ON_NORMAL=echo x >> $CASE_DIR/hook.log"
  start f
  wait_for "f  1:1  NORMAL"
  hook_lines 1
  typed i
  wait_for INSERT
  keys Escape
  hook_lines 2
  typed "ihello"
  keys Escape
  hook_lines 3
  typed ":s/h/H/"
  keys Enter
  hook_lines 4
  typed jjk # moving in Normal mode runs nothing
  sleep 0.3
  hook_lines 4
  # Reaped: no child of si is left as a zombie.
  pid=$(cat "$CASE_DIR/si.pid")
  [ -z "$(ps -A -o ppid=,stat= | awk -v p="$pid" '$1 == p && $2 ~ /^Z/')" ]
  typed ":q!"
  keys Enter
  wait_for "exit=0"
  # A hook that fails says nothing.
  restart
  rm -f "$CASE_DIR/hook.log"
  SI_BIN=$real
  wrap "SI_ON_NORMAL=nosuchcommand_si_e2e"
  start f
  wait_for "f  1:1  NORMAL"
  typed i
  keys Escape
  sleep 0.3
  screen | grep -F -q "NORMAL"
  gone "not found"
  gone "nosuchcommand"
}

hook_lines() {
  tries=100
  until [ "$(wc -l < "$CASE_DIR/hook.log" 2> /dev/null | tr -d ' ')" = "$1" ]; do
    tries=$((tries - 1))
    [ "$tries" -gt 0 ] || { echo "hook.log has $(wc -l < "$CASE_DIR/hook.log" 2>&1) lines, not $1" >&2; return 1; }
    sleep 0.05
  done
}

cli_6_reopening_returns_to_the_last_position() {
  i=1
  while [ "$i" -le 100 ]; do echo "line $i of the file"; i=$((i + 1)); done > "$CASE_DIR/f"
  cp "$CASE_DIR/f" "$CASE_DIR/COMMIT_EDITMSG"
  start f
  wait_for "f  1:1  NORMAL"
  typed "50Gll"
  wait_for "f  50:3  NORMAL"
  typed ":q"
  keys Enter
  wait_exit
  restart
  # The cursor is where it was, in the middle of the screen.
  start f
  wait_for "f  50:3  NORMAL"
  [ "$(screen | sed -n 12p | cut -c1-30)" = "  50 line 50 of the file" ]
  # `:e` away and back remembers too; ZZ and a signal are ways out as well.
  typed "20G"
  wait_for "f  20:1  NORMAL"
  typed ":e COMMIT_EDITMSG"
  keys Enter
  wait_for "COMMIT_EDITMSG  1:1  NORMAL"
  typed ":e f"
  keys Enter
  wait_for "f  20:1  NORMAL"
  typed "ZZ"
  wait_exit
  restart
  # +cmd and file:line say where to go; git's message files start at the top.
  start +3 f
  wait_for "f  3:1  NORMAL"
  typed ":q"
  keys Enter
  wait_exit
  restart
  start f:7
  wait_for "f  7:1  NORMAL"
  typed ":q"
  keys Enter
  wait_exit
  restart
  start COMMIT_EDITMSG
  wait_for "COMMIT_EDITMSG  1:1  NORMAL"
  typed "30G"
  wait_for "COMMIT_EDITMSG  30:1  NORMAL"
  typed ":q"
  keys Enter
  wait_exit
  restart
  start COMMIT_EDITMSG
  wait_for "COMMIT_EDITMSG  1:1  NORMAL"
  sleep 0.3
  screen | grep -F -q "COMMIT_EDITMSG  1:1  NORMAL"
  typed ":q"
  keys Enter
  wait_exit
  # A file nobody has opened starts at the top; the places are in the private state directory.
  restart
  printf 'a\nb\n' > "$CASE_DIR/new.txt"
  start new.txt
  wait_for "new.txt  1:1  NORMAL"
  [ -f "$XDG_STATE_HOME/si/positions" ]
  [ "$(stat -c %a "$XDG_STATE_HOME/si/positions" 2> /dev/null || stat -f %Lp "$XDG_STATE_HOME/si/positions")" = 600 ]
}

# The cursor goes to the first match while the pattern is typed; Esc gives the place back; Enter keeps it.
vi_14_search_moves_while_typing() {
  i=1
  while [ "$i" -le 100 ]; do echo "row $i"; i=$((i + 1)); done > "$CASE_DIR/f"
  start f
  wait_for "f  1:1  NORMAL"
  typed "/row 7"
  wait_for "/row 7"
  screen | grep -F -q "   7 row 7"
  typed "7"
  wait_for "  77 row 77"
  [ "$(screen | sed -n 12p | cut -c1-12)" = "  77 row 77  " ] || [ "$(screen | sed -n 12p | cut -c1-11)" = "  77 row 77" ]
  keys Escape
  wait_for "f  1:1  NORMAL"
  screen | grep -F -q "   1 row 1"
  gone "  77 row 77"
  typed "/row 77"
  keys Enter
  wait_for "f  77:1  NORMAL"
  [ "$(screen | sed -n 12p | cut -c1-11)" = "  77 row 77" ]
  # A pattern that is bad so far, or has no match, leaves the cursor where it started.
  typed "/(row"
  sleep 0.2
  screen | grep -F -q "  77 row 77"
  keys Escape
  wait_for "f  77:1  NORMAL"
}

# VIS-4: si asks the terminal for its background (OSC 11) and colors the screen from the answer: the
# text in the palette's primary gray, line numbers in its secondary, a change on the change
# background. No answer, no color (faint line numbers stand in); a late answer redraws everything.
# The answer's own keys never reach the editor.
vis_4_colors_follow_the_terminal_background() {
  printf 'one\ntwo\nthree\n' > "$CASE_DIR/f"
  start f
  wait_for "f  1:1  NORMAL"
  sleep 0.2
  ! colors | grep -F -q '38;2;'
  colors | grep -F -q '[2m'
  # A dark terminal (#1e1e1e) answers late: primary 230, secondary 180, change #005ea4.
  reply rgb:1e1e/1e1e/1e1e
  wait_colors '38;2;230;230;230'
  colors | grep -F -q '38;2;180;180;180'
  ! colors | grep -F -q '[2m'
  printf 'one\nTWO\nthree\n' > "$CASE_DIR/f"
  wait_for "1 change on disk"
  colors | grep -F -q '48;2;0;94;164'
  # Nothing of the answer was typed into the buffer: it is still unmodified and quits at once.
  typed ":q"
  keys Enter
  wait_exit
  [ "$(exit_code)" = 0 ]
  # A light terminal (white): dark text, a pale blue change.
  restart
  printf 'one\ntwo\nthree\n' > "$CASE_DIR/f"
  start f
  wait_for "f  1:1  NORMAL"
  reply rgb:ffff/ffff/ffff
  wait_colors '38;2;75;75;75'
  printf 'one\nTWO\nthree\n' > "$CASE_DIR/f"
  wait_for "1 change on disk"
  colors | grep -F -q '48;2;195;224;255'
  # An answer ended by BEL instead of ESC \ is the same: black gives 227 and a deeper change blue.
  tm send-keys -t "$SESSION" -l -- "$(printf '\033]11;rgb:0000/0000/0000\007')"
  wait_colors '38;2;227;227;227'
  colors | grep -F -q '48;2;0;90;157'
  # Esc and ] in one write look like the start of an answer: they are Esc, then `]c` (next change).
  typed "$(printf '\033]c')"
  wait_for "f  2:1  NORMAL"
  typed ":q"
  keys Enter
  wait_exit
  [ "$(exit_code)" = 0 ]
}

main "$@"
