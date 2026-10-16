#!/bin/sh
# Retry failed jobs without repeating successful jobs on the same commit.
set -eu

if [ "$#" -ne 1 ]; then
  echo 'usage: scripts/rerun-failed.sh RUN_ID (one numeric run ID)' >&2
  exit 2
fi
case $1 in
  ''|*[!0-9]*)
    echo 'usage: scripts/rerun-failed.sh RUN_ID (one numeric run ID)' >&2
    exit 2
    ;;
esac

exec gh run rerun "$1" --failed --repo efg-technologies/sim
