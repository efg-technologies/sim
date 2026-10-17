#!/bin/sh
# Runs every check that must pass before a commit (see "Checks" in CLAUDE.md).
# Stops at the first failure and says which step failed.
set -eu

cd "$(dirname "$0")/.."

# The core (core/src) is the part that must never lose text: its own hard cap and its own allowlist
# of dependencies. The editor (sim/src) has a cap of its own; the reasons for both are in CONCEPT.md.
MAX_CORE_LINES=3200
MAX_SIM_LINES=7980
CORE_DEPS="crossterm notify regex-lite ropey rustix similar unicode-width"
MAX_BYTES=$((5 * 1024 * 1024))

N=9
step() {
  n=$1
  shift
  printf '\n[%s/%s] %s\n' "$n" "$N" "$*"
  "$@" || {
    printf 'check.sh: step %s failed: %s\n' "$n" "$*" >&2
    exit 1
  }
}

# Lines of Rust under $1: blank lines and lines that start with `//` (`///` and `//!` too) don't count.
# Each file is followed by an empty line so that a missing final newline can't join two files.
count() {
  find "$1" -type f -name '*.rs' | while read -r f; do
    cat "$f"
    echo
  done | awk '!/^[ \t]*$/ && !/^[ \t]*\/\// { n++ } END { print n + 0 }'
}

step 1 cargo fmt --check
step 2 cargo clippy --workspace --all-targets -- -D warnings
# The tests that run git never read the user's git configuration (and never write it).
step 3 env GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_SYSTEM=/dev/null XDG_CONFIG_HOME=/nonexistent cargo test --workspace
step 4 sh scripts/trace.sh

printf '\n[5/%s] core/src has at most %s lines\n' "$N" "$MAX_CORE_LINES"
lines=$(count core/src)
echo "core/src: $lines lines (limit $MAX_CORE_LINES)"
if [ "$lines" -gt "$MAX_CORE_LINES" ]; then
  echo "check.sh: step 5 failed: core/src has $lines lines; propose features to cut" >&2
  exit 1
fi

printf '\n[6/%s] sim/src has at most %s lines\n' "$N" "$MAX_SIM_LINES"
lines=$(count sim/src)
echo "sim/src: $lines lines (limit $MAX_SIM_LINES)"
if [ "$lines" -gt "$MAX_SIM_LINES" ]; then
  echo "check.sh: step 6 failed: sim/src has $lines lines; propose features to cut" >&2
  exit 1
fi

# 7. The core depends on nothing but its allowlist (tests/safe_9.rs reads the manifest; this reads
# what cargo resolved).
printf '\n[7/%s] the core depends on %s only\n' "$N" "$CORE_DEPS"
deps=$(cargo tree -p sim-editor-core -e normal --depth 1 --prefix none | awk 'NR > 1 { print $1 }' | sort -u | tr '\n' ' ' | sed 's/ $//')
echo "core: $deps"
if [ "$deps" != "$CORE_DEPS" ]; then
  echo "check.sh: step 7 failed: the core depends on [$deps], not [$CORE_DEPS]; write the reason into SPEC first" >&2
  exit 1
fi

# 8. The name: sim is Secure vIM, and si, once a program, is only its core now. The crates are
# sim-editor and sim-editor-core. What this repository said before must not come back (this file
# names the patterns, so it is left out).
printf '\n[8/%s] the name is Secure vIM, the crates sim-editor and sim-editor-core\n' "$N"
old=$(grep -rIn -E 'SI iMproved|SIght|sim-core' . --exclude-dir=target --exclude-dir=.git --exclude=check.sh || true)
if [ -n "$old" ]; then
  printf '%s\n' "$old" >&2
  echo "check.sh: step 8 failed: the name sim was given before (SI iMproved, SIght) or the crate sim-core is back" >&2
  exit 1
fi

# 9. Size of the release binary.
step 9 cargo build --release
bin=${CARGO_TARGET_DIR:-target}/release/sim
bytes=$(wc -c < "$bin" | tr -d ' ')
echo "$bin: $bytes bytes (limit $MAX_BYTES)"
if [ "$bytes" -gt "$MAX_BYTES" ]; then
  echo "check.sh: step 9 failed: $bin is $bytes bytes, over $MAX_BYTES" >&2
  exit 1
fi

printf '\ncheck.sh: all checks passed\n'
