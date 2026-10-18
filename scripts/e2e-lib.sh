# Read by core/scripts/e2e.sh and sim/scripts/e2e.sh (not run by itself): what a failing case says about
# where it failed.
#
# Under `set -e` a command of a case that fails says nothing (`grep -q`, `[ ... ]`, `cmp`), and the screen
# at failure is what sim showed, not which line of the case asked for it. So a case runs traced
# (`set -x`, with PS4 holding LINENO), and the harness reads the trace after a failure: the last line of
# the case that ran is the one that failed (or, when a helper failed, the one that called it).
#
# A shell counts LINENO from the start of the function it is in (dash) or of the text it reads, so the
# case is not called as a function: its body is written to a file, below as many empty lines as come before
# the case in the harness, and read with `.` at the top level of the script. Its lines are then numbered
# as they are in the harness whichever shell runs it (dash on Linux, bash 3.2 as sh on macOS).

# case_span FILE NAME: the line of `NAME() {` and the line of its closing `}`.
case_span() { awk -v n="$2()" 'index($0, n) == 1 { s = NR } s && /^}/ { print s, NR; exit }' "$1"; }

# case_body FILE NAME: the body of the case with every other line of FILE empty.
case_body() {
  span=$(case_span "$1" "$2")
  awk -v s="${span% *}" -v e="${span#* }" 'NR > s && NR < e { print; next } NR <= e { print "" }' "$1"
}

# trace_lines LOG: the lines of the trace in LOG (the trace is `+@@ LINE: command`, with more `+` for
# a command in a subshell), with the line and the command apart.
trace_lines() { grep -a -E '^\++@@ [0-9]+: ' "$1" | sed 's/^+*@@ \([0-9][0-9]*\): /\1 /' || true; }

# where_failed LOG FILE NAME: the last command of the case NAME that LOG shows, as FILE has it and as it
# ran (its variables replaced by their values), cut to a line of 200 columns each.
where_failed() {
  span=$(case_span "$2" "$3")
  [ -n "$span" ] || return 0
  line=$(trace_lines "$1" | awk -v s="${span% *}" -v e="${span#* }" '$1 > s && $1 < e { last = $0 } END { print last }')
  if [ -z "$line" ]; then
    echo "---- no command of the case ran ----"
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
