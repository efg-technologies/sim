#!/bin/sh
# Checks that sim's SPEC, code and tests line up: si's scripts/trace.sh, pointed at sim/.
# Milestones are S0-S6 and the test-name families are sim's.
set -eu

here=$(cd "$(dirname "$0")/.." && pwd)
TRACE_DIR=$here TRACE_PREFIX=S TRACE_FAMILIES='tree|buf|find|grep|mark|syn|ptr|look|speed|ui'
export TRACE_DIR TRACE_PREFIX TRACE_FAMILIES
exec sh "$here/../scripts/trace.sh" "$@"
