#!/bin/sh
# Benchmarks of the real binary in a terminal (PERF-1, PERF-5). All four benchmarks, in-process
# ones (PERF-2, PERF-4) included, also run as Rust tests:
#
#   cargo test --release -- --ignored bench --nocapture
#
# This script prints:
#   PERF-1  median time of `si +q` on a 10,000-line file, with hyperfine if it is installed
#           (inside a private tmux server, because si needs a terminal), and with
#           scripts/pty_bench.py (python3 and a pty; /bin/true timed the same way is the baseline).
#           `+q` quits before the first frame, so it never waits for the terminal's color (VIS-4);
#           pty_bench.py also prints the time to the first frame, with the pty answering the OSC 11
#           query like a terminal and with it silent (the 3 ms wait is in that one)
#   PERF-5  VmRSS and peak RSS of si with a 10 MB file open (python3 and Linux)
set -eu

cd "$(dirname "$0")/.."

cargo build --release --quiet
bin=${CARGO_TARGET_DIR:-target}/release/si
case $bin in /*) ;; *) bin=$PWD/$bin ;; esac

tmp=$(mktemp -d "${TMPDIR:-/tmp}/si-bench.XXXXXX")
trap 'tmux -L si-bench kill-server 2> /dev/null || true; rm -rf "$tmp"' EXIT HUP INT TERM

# $1 lines of about $2 bytes each: code-like text, no randomness needed.
make_file() {
  awk -v n="$1" -v w="$2" 'BEGIN {
    for (i = 0; i < w - 32; i++) pad = pad "x"
    for (i = 1; i <= n; i++) printf "    let value_%d = compute(%d) + %s;\n", i, i, pad
  }' > "$3"
}

make_file 10000 70 "$tmp/ten_thousand_lines.rs"
make_file 110000 100 "$tmp/ten_mb.rs"

if command -v hyperfine > /dev/null 2>&1 && command -v tmux > /dev/null 2>&1; then
  # hyperfine's output must stay on the terminal: --output=inherit hands it to si, which needs one.
  tmux -L si-bench -f /dev/null new-session -d -x 80 -y 24 \
    "hyperfine -N --warmup 10 --output=inherit --export-json '$tmp/h.json' '$bin +q $tmp/ten_thousand_lines.rs'; touch '$tmp/done'" \
    \; set-option -w remain-on-exit on
  tries=600
  while [ ! -e "$tmp/done" ] && [ "$tries" -gt 0 ]; do
    sleep 0.1
    tries=$((tries - 1))
  done
  [ -e "$tmp/done" ] || { echo "bench: hyperfine did not finish" >&2; exit 1; }
  [ -s "$tmp/h.json" ] || { tmux -L si-bench capture-pane -p >&2; echo "bench: hyperfine failed" >&2; exit 1; }
  awk -F'[:,]' '/"median"/ { printf "PERF-1 startup (hyperfine): median %.2f ms\n", $2 * 1000; exit }' "$tmp/h.json"
fi
if command -v python3 > /dev/null 2>&1; then
  echo "PERF-1 (scripts/pty_bench.py)"
  python3 scripts/pty_bench.py startup "$bin" "$tmp/ten_thousand_lines.rs" > /dev/null
elif ! command -v hyperfine > /dev/null 2>&1; then
  echo "bench: PERF-1 needs hyperfine (and tmux) or python3" >&2
fi

if [ "$(uname -s)" = Linux ] && command -v python3 > /dev/null 2>&1; then
  python3 scripts/pty_bench.py rss "$bin" "$tmp/ten_mb.rs" > /dev/null
else
  echo "bench: PERF-5 (RSS) needs Linux and python3; skipped"
fi
