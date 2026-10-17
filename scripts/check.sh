#!/bin/sh
# Runs every check that must pass before a commit (see "Checks" in CLAUDE.md).
# Stops at the first failure and says which step failed.
set -eu

cd "$(dirname "$0")/.."

MAX_LINES=3603
MAX_BYTES=$((2 * 1024 * 1024))

step() {
  n=$1
  shift
  printf '\n[%s/6] %s\n' "$n" "$*"
  "$@" || {
    printf 'check.sh: step %s failed: %s\n' "$n" "$*" >&2
    exit 1
  }
}

step 1 cargo fmt --check
step 2 cargo clippy --all-targets -- -D warnings
step 3 cargo test
step 4 sh scripts/trace.sh

# 5. Size of src/: blank lines and lines that start with `//` (`///` and `//!` too) don't count.
# Each file is followed by an empty line so that a missing final newline can't join two files.
printf '\n[5/6] src/ has at most %s lines\n' "$MAX_LINES"
lines=$(
  find src -type f -name '*.rs' | while read -r f; do
    cat "$f"
    echo
  done | awk '!/^[ \t]*$/ && !/^[ \t]*\/\// { n++ } END { print n + 0 }'
)
echo "src/: $lines lines (limit $MAX_LINES)"
if [ "$lines" -gt "$MAX_LINES" ]; then
  echo "check.sh: step 5 failed: src/ has $lines lines; propose features to cut" >&2
  exit 1
fi

# 6. Size of the release binary.
step 6 cargo build --release
bin=${CARGO_TARGET_DIR:-target}/release/si
bytes=$(wc -c < "$bin" | tr -d ' ')
echo "$bin: $bytes bytes (limit $MAX_BYTES)"
if [ "$bytes" -gt "$MAX_BYTES" ]; then
  echo "check.sh: step 6 failed: $bin is $bytes bytes, over $MAX_BYTES" >&2
  exit 1
fi

printf '\ncheck.sh: all checks passed\n'
