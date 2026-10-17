#!/bin/sh
# End-to-end tests of sim, in tmux: the core's cases (run against `sim --plain`) and the editor's.
#
#   sh scripts/e2e.sh [prefix]
#
# Runs both suites, or only the cases whose name starts with the prefix (a suite with no such case
# is skipped). The suites are core/scripts/e2e.sh and sim/scripts/e2e.sh; each says how it works.
set -eu

cd "$(dirname "$0")/.."

status=0
for suite in core/scripts/e2e.sh sim/scripts/e2e.sh; do
  printf '\n== %s\n' "$suite"
  # A prefix that no case of this suite has is not a failure of this suite.
  if [ -n "${1:-}" ] && ! grep -E "^${1}[a-z0-9_]*\(\)" "$suite" > /dev/null; then
    echo "no case starts with '$1'"
    continue
  fi
  sh "$suite" "$@" || status=1
done
exit "$status"
