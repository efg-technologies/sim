#!/usr/bin/env python3
"""Benchmarks of the real si binary, which needs a terminal: every run gets its own pty.

  pty_bench.py startup BIN FILE [N]   wall time of `BIN +q FILE` (PERF-1), N runs (default 200),
                                      and of /bin/true in the same harness as the baseline
  pty_bench.py rss BIN FILE           VmRSS and VmHWM of `BIN FILE` after its first frame
                                      (PERF-5; Linux only)

Prints key=value lines (times in ms, memory in kB) after a readable summary on stderr.
Used by scripts/bench.sh, tests/perf_1.rs and tests/perf_5.rs. Standard library only.
"""
import fcntl
import os
import select
import statistics
import struct
import subprocess
import sys
import termios
import time


def open_pty():
    master, slave = os.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))
    return master, slave


def run_once(argv):
    """Seconds from spawning argv, with a pty as stdin, stdout and stderr, to its exit."""
    master, slave = open_pty()
    try:
        start = time.perf_counter()
        proc = subprocess.Popen(argv, stdin=slave, stdout=slave, stderr=slave)
        code = proc.wait(timeout=10)
        elapsed = time.perf_counter() - start
    finally:
        os.close(slave)
        os.close(master)
    if code != 0:
        sys.exit(f"pty_bench: {argv} exited with {code}")
    return elapsed


def median_ms(argv, runs, warmup=10):
    for _ in range(warmup):
        run_once(argv)
    times = sorted(run_once(argv) * 1000 for _ in range(runs))
    return statistics.median(times), times[0], times[int(len(times) * 0.9)]


def startup(binary, path, runs):
    si = median_ms([binary, "+q", path], runs)
    base = median_ms(["/bin/true"], runs)
    print(
        f"startup: si +q median {si[0]:.2f} ms (min {si[1]:.2f}, p90 {si[2]:.2f}); "
        f"/bin/true baseline median {base[0]:.2f} ms (min {base[1]:.2f}, p90 {base[2]:.2f}); N={runs}",
        file=sys.stderr,
    )
    print(f"median_ms={si[0]:.3f}")
    print(f"baseline_ms={base[0]:.3f}")
    print(f"runs={runs}")


def rss(binary, path):
    if not os.path.exists("/proc/self/status"):
        sys.exit("pty_bench: rss needs /proc (Linux)")
    master, slave = open_pty()
    # Run next to the file, so that its short name leaves room for NORMAL on the status line.
    where, name = os.path.split(os.path.abspath(path))
    proc = subprocess.Popen(
        [os.path.abspath(binary), name], cwd=where, stdin=slave, stdout=slave, stderr=slave
    )
    try:
        seen, deadline = b"", time.time() + 10
        while b"NORMAL" not in seen:  # the status line of the first frame
            if time.time() > deadline or proc.poll() is not None:
                sys.exit(f"pty_bench: no first frame from {binary}: {seen[-200:]!r}")
            if select.select([master], [], [], 0.05)[0]:
                seen += os.read(master, 65536)
        status = dict(
            line.split(":", 1) for line in open(f"/proc/{proc.pid}/status").read().splitlines()
        )
        vm = {k: int(status[k].split()[0]) for k in ("VmRSS", "VmHWM")}
        os.write(master, b":q\r")
        if proc.wait(timeout=10) != 0:
            sys.exit("pty_bench: si did not quit with :q")
    finally:
        if proc.poll() is None:
            proc.kill()
        os.close(slave)
        os.close(master)
    size = os.path.getsize(path)
    print(
        f"rss: {size / 1e6:.1f} MB file, VmRSS {vm['VmRSS'] / 1024:.1f} MB, "
        f"VmHWM (peak) {vm['VmHWM'] / 1024:.1f} MB",
        file=sys.stderr,
    )
    print(f"vmrss_kb={vm['VmRSS']}")
    print(f"vmhwm_kb={vm['VmHWM']}")
    print(f"file_bytes={size}")


def main(argv):
    if len(argv) >= 4 and argv[1] == "startup":
        startup(argv[2], argv[3], int(argv[4]) if len(argv) > 4 else 200)
    elif len(argv) == 4 and argv[1] == "rss":
        rss(argv[2], argv[3])
    else:
        sys.exit(__doc__)


main(sys.argv)
