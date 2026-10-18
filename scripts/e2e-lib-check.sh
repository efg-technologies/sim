#!/bin/sh
# The traced path of the e2e harnesses (scripts/e2e-lib.sh) on cases of a harness of its own, with no tmux
# and no sim: a case that passes passes, a case that fails leaves a trace that says where, and the shell
# that runs this reads it. check.sh runs it, so on a GitHub runner it is Ubuntu's dash (no LINENO) that
# runs it, and on macOS bash 3.2 as sh. (`set -u` and a bare `$LINENO` once stopped every e2e case there.)
set -eu

cd "$(dirname "$0")/.."
ROOT=$PWD
tmp=$(mktemp -d "${TMPDIR:-/tmp}/sim-e2e-lib.XXXXXX")
trap 'rm -rf "$tmp"' EXIT HUP INT TERM

# A harness shaped as the end of core/scripts/e2e.sh and sim/scripts/e2e.sh are, with a trap that traces
# what it does at the end of the case, as theirs do.
cat > "$tmp/harness.sh" << 'HARNESS'
set -eu
ROOT=$E2E_ROOT
SELF=$E2E_SELF
. "$ROOT/scripts/e2e-lib.sh"

helper() { [ "$1" = "$2" ]; }

passing_case() {
  x=1
  helper "$x" 1
}

failing_case() {
  x=1
  [ "$x" = 2 ]
  x=3
}

if [ "${1:-}" = "--run-one" ]; then
  trap 'status=$?
        : e2e-case-over
        set +x
        echo cleaning up
        exit "$status"' EXIT
  case_body "$SELF" "$2" > "$E2E_TMP/case"
  trace_on
  . "$E2E_TMP/case"
  exit 0
fi
HARNESS

run() { E2E_ROOT=$ROOT E2E_SELF=$tmp/harness.sh E2E_TMP=$tmp sh "$tmp/harness.sh" --run-one "$1" > "$tmp/$1.log" 2>&1; }
fail() { echo "e2e-lib-check: $*" >&2; sed 's/^/  | /' "$tmp/$name.log" >&2; exit 1; }

name=passing_case
run $name || fail "a case that passes did not (the trace must not stop it)"
grep -a -E -q '^\++@@ [0-9?]+: ' "$tmp/$name.log" || fail "the case ran with no trace"

name=failing_case
if run $name; then fail "a case that fails passed"; fi
. "$ROOT/scripts/e2e-lib.sh"
told=$(where_failed "$tmp/$name.log" "$tmp/harness.sh" $name)
case $told in
  *"1 = 2"*) ;;
  *) echo "$told" >&2; fail "where_failed did not say the command that failed" ;;
esac
if [ -n "${LINENO:-}" ]; then
  want=$(grep -n '^  \[ "\$x" = 2 \]$' "$tmp/harness.sh" | cut -d: -f1)
  case $told in
    *"failed at line $want of "*) shape="with LINENO" ;;
    *) echo "$told" >&2; fail "where_failed did not say line $want" ;;
  esac
else
  case $told in
    *"unknown line"*) shape="without LINENO" ;;
    *) echo "$told" >&2; fail "where_failed did not say that the line is unknown" ;;
  esac
fi
echo "e2e-lib-check: ok, a failing case is told by its command ($shape in this shell)"
