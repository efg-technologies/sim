#!/bin/sh
# Runs every check that must pass before a commit that touches sim/ (see "Tests and checks" in
# sim/CLAUDE.md). si's own scripts/check.sh must keep passing too. Stops at the first failure.
set -eu

cd "$(dirname "$0")/.."

MAX_LINES=7000

step() {
  n=$1
  shift
  printf '\n[%s/5] %s\n' "$n" "$*"
  "$@" || {
    printf 'sim/scripts/check.sh: step %s failed: %s\n' "$n" "$*" >&2
    exit 1
  }
}

step 1 cargo fmt --check -p sim
step 2 cargo clippy -p sim --all-targets -- -D warnings
# The tests that run git never read the user's git configuration (and never write it).
step 3 env GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_SYSTEM=/dev/null XDG_CONFIG_HOME=/nonexistent cargo test -p sim
step 4 sh scripts/trace.sh

# 5. Size of sim/src: counted like si's (blank lines and lines that start with `//` don't count).
printf '\n[5/5] sim/src has at most %s lines\n' "$MAX_LINES"
lines=$(
  find src -type f -name '*.rs' | while read -r f; do
    cat "$f"
    echo
  done | awk '!/^[ \t]*$/ && !/^[ \t]*\/\// { n++ } END { print n + 0 }'
)
echo "sim/src: $lines lines (limit $MAX_LINES)"
if [ "$lines" -gt "$MAX_LINES" ]; then
  echo "sim/scripts/check.sh: step 5 failed: sim/src has $lines lines; propose features to cut" >&2
  exit 1
fi

printf '\nsim/scripts/check.sh: all checks passed\n'
