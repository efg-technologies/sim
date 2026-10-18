# Read by core/scripts/e2e.sh and sim/scripts/e2e.sh (not run by itself): what a failing case says about
# where it failed.
#
# Under `set -e` a command of a case that fails says nothing (`grep -q`, `[ ... ]`, `cmp`), and the screen
# at failure is what sim showed, not which line of the case asked for it. So a case runs traced
# (`trace_on`: `set -x`, with PS4 holding LINENO), and the harness reads the trace after a failure: the last
# line of the case that ran is the one that failed (or, when a helper failed, the one that called it).
#
# A shell counts LINENO from the start of the function it is in (dash) or of the text it reads, so the
# case is not called as a function: its body is written to a file, below as many empty lines as come before
# the case in the harness, and read with `.` at the top level of the script. Its lines are then numbered
# as they are in the harness in bash (Linux, and macOS's `sh`, bash 3.2). Debian's and Ubuntu's dash (the
# `sh` of a GitHub runner) has no LINENO at all, and with `set -u` the bare `$LINENO` in PS4 stopped every
# case at its first command: PS4 says `${LINENO:-?}`, the trace has `?` for the line there, and the failure
# is told by the last command it ran instead of by its line (`where_failed`). scripts/e2e-lib-check.sh
# runs this path without tmux and without sim (check.sh), in the `sh` it is run by.

# trace_on: from here the shell traces what it runs, `+@@ LINE: command` (the line is `?` without LINENO).
# The one place that says it, so that the two harnesses and the check of this file cannot differ.
trace_on() {
  PS4='+@@ ${LINENO:-?}: '
  set -x
}

# The lines of a trace in a log, as `trace_lines` and the harnesses' filters read them.
trace_re='^\++@@ [0-9?]+: '

# case_span FILE NAME: the line of `NAME() {` and the line of its closing `}`.
case_span() { awk -v n="$2()" 'index($0, n) == 1 { s = NR } s && /^}/ { print s, NR; exit }' "$1"; }

# case_body FILE NAME: the body of the case with every other line of FILE empty.
case_body() {
  span=$(case_span "$1" "$2")
  awk -v s="${span% *}" -v e="${span#* }" 'NR > s && NR < e { print; next } NR <= e { print "" }' "$1"
}

# trace_lines LOG: the lines of the trace in LOG (the trace is `+@@ LINE: command`, with more `+` for
# a command in a subshell), with the line and the command apart.
trace_lines() { grep -a -E "$trace_re" "$1" | sed 's/^+*@@ \([0-9?][0-9]*\): /\1 /' || true; }

# where_failed LOG FILE NAME: the last command of the case NAME that LOG shows, as FILE has it and as it
# ran (its variables replaced by their values), cut to a line of 200 columns each.
where_failed() {
  span=$(case_span "$2" "$3")
  [ -n "$span" ] || return 0
  line=$(trace_lines "$1" | awk -v s="${span% *}" -v e="${span#* }" '$1 > s && $1 < e { last = $0 } END { print last }')
  if [ -z "$line" ]; then
    # (a trace with `?` for every line, from a shell without LINENO, has no line of the case in it: the
    # last command it ran is the one that failed, or the one a helper ran. The trap that ends a case
    # traces `status=$?` and then `: e2e-case-over`: the command before them is the one)
    line=$(trace_lines "$1" | awk '$1 != "?" { next } $2 == ":" && $3 == "e2e-case-over" { last = before; exit } { before = last; last = $0 } END { print last }')
    if [ -z "$line" ]; then
      echo "---- no command of the case ran ----"
    else
      echo "---- failed at an unknown line of ${2#"$ROOT"/}: this shell has no LINENO ----"
      echo "ran as: ${line#* }" | cut -c1-200
      echo "------------------------------------------"
    fi
    return 0
  fi
  echo "---- failed at line ${line%% *} of ${2#"$ROOT"/} ----"
  sed -n "${line%% *}p" "$2" | sed 's/^ *//' | cut -c1-200
  echo "ran as: ${line#* }" | cut -c1-200
  echo "------------------------------------------"
}

# keep_trace LOG FILE NAME: SIM_E2E_KEEP=DIR: where the case failed and the last commands it ran, in
# DIR/<case>.txt after the screens that the case itself kept (CI uploads the directory).
keep_trace() {
  [ -n "${SIM_E2E_KEEP:-}" ] || return 0
  mkdir -p "$SIM_E2E_KEEP" || return 0
  {
    where_failed "$1" "$2" "$3"
    echo "== $3: the last 200 commands of the trace (line, command)"
    trace_lines "$1" | tail -n 200
  } >> "$SIM_E2E_KEEP/$3.txt" 2>&1 || true
}

# until_gone PID...: waits, 5 s at most for each, until nothing runs in the session of these pane pids. sim saves
# its unsaved text, its place and its choices when tmux hangs it up (SAFE-6, CLI-6): the files are not
# there to be read, nor to be removed, before it has finished.
until_gone() {
  for pid in "$@"; do
    tries=100
    while { pgrep -s "$pid" > /dev/null 2>&1 || kill -0 -- "-$pid" 2> /dev/null; } && [ "$tries" -gt 0 ]; do
      sleep 0.05
      tries=$((tries - 1))
    done
  done
}
