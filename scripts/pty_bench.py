#!/usr/bin/env python3
"""Benchmarks of the real si binary, which needs a terminal: every run gets its own pty.

  pty_bench.py startup BIN FILE [N]   wall time of `BIN +q FILE` (PERF-1), N runs (default 200),
                                      with the pty answering si's OSC 11 query like a terminal
                                      (`median_ms`) and without an answer (`median_unanswered_ms`,
                                      which includes the 3 ms wait of VIS-4), and of /bin/true in
                                      the same harness as the baseline. `si +q` quits before it
                                      draws, so it never waits for the color: `first_frame_ms` and
                                      `first_frame_unanswered_ms` are the time to the first frame
                                      of `BIN FILE`, with and without an answer
  pty_bench.py rss BIN FILE           VmRSS and VmHWM of `BIN FILE` after its first frame
                                      (PERF-5; Linux only)
  pty_bench.py firstframe BIN FILE    whether the first frame of `BIN FILE` is in color when the
                                      terminal's answer to the OSC 11 query is there in time
                                      (VIS-4): `colored` and `faint` (1 or 0), best of 10 starts

Prints key=value lines (times in ms, memory in kB) after a readable summary on stderr.
Used by scripts/bench.sh, tests/perf_1.rs and tests/perf_5.rs. Standard library only.
"""
import atexit
import fcntl
import os
import select
import signal
import statistics
import shutil
import struct
import subprocess
import sys
import tempfile
import termios
import time


# si's places and snapshots (SAFE-6, CLI-6) go to a directory of its own, never to the user's.
STATE = tempfile.mkdtemp(prefix="si-bench-state.")
ENV = dict(os.environ, HOME=STATE, XDG_STATE_HOME=STATE)
atexit.register(shutil.rmtree, STATE, True)


def open_pty():
    master, slave = os.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))
    return master, slave


QUERY = b"\x1b]11;?"  # si asks for the terminal's background color (VIS-4)
ANSWER = b"\x1b]11;rgb:1e1e/1e1e/1e1e\x1b\\"


def run_once(argv, answer=False):
    """Seconds from spawning argv, with a pty as stdin, stdout and stderr, to its exit. With
    `answer` the pty plays a terminal: it answers the OSC 11 query as soon as it sees it."""
    master, slave = open_pty()
    try:
        start = time.perf_counter()
        proc = subprocess.Popen(argv, stdin=slave, stdout=slave, stderr=slave, env=ENV)
        seen = b""
        while answer and QUERY not in seen and proc.poll() is None:
            if select.select([master], [], [], 0.0005)[0]:
                seen += os.read(master, 65536)
        if answer and QUERY in seen:
            os.write(master, ANSWER)
        # A blocking wait: Popen.wait(timeout=...) polls with sleeps of 0.5, 1, 2, 4 ms..., which
        # rounds every time up to the next of 3.5, 7.5, 15.5 ms (counted from the call).
        signal.alarm(10)
        code = proc.wait()
        signal.alarm(0)
        elapsed = time.perf_counter() - start
    finally:
        os.close(slave)
        os.close(master)
    if code != 0:
        sys.exit(f"pty_bench: {argv} exited with {code}")
    return elapsed


def median_ms(argv, runs, warmup=10, answer=False):
    for _ in range(warmup):
        run_once(argv, answer)
    times = sorted(run_once(argv, answer) * 1000 for _ in range(runs))
    return statistics.median(times), times[0], times[int(len(times) * 0.9)]


def frame_ms(binary, path, answer):
    """Seconds from spawning `binary path` to the status line of its first frame. With `answer` the
    pty answers the OSC 11 query as soon as it sees it; without, si waits the 3 ms out."""
    master, slave = open_pty()
    where, name = os.path.split(os.path.abspath(path))
    start = time.perf_counter()
    proc = subprocess.Popen(
        [os.path.abspath(binary), name], cwd=where, stdin=slave, stdout=slave, stderr=slave, env=ENV
    )
    try:
        seen, answered, deadline = b"", False, time.time() + 10
        while b"NORMAL" not in seen:
            if time.time() > deadline or proc.poll() is not None:
                sys.exit(f"pty_bench: no first frame from {binary}: {seen[-200:]!r}")
            if select.select([master], [], [], 0.0005)[0]:
                seen += os.read(master, 65536)
            if answer and not answered and QUERY in seen:
                os.write(master, ANSWER)
                answered = True
        took = time.perf_counter() - start
        os.write(master, b":q\r")
        proc.wait(timeout=10)
    finally:
        if proc.poll() is None:
            proc.kill()
        os.close(slave)
        os.close(master)
    return took


def startup(binary, path, runs):
    si = median_ms([binary, "+q", path], runs, answer=True)
    mute = median_ms([binary, "+q", path], runs)
    base = median_ms(["/bin/true"], runs)
    print(
        f"startup: si +q median {si[0]:.2f} ms (min {si[1]:.2f}, p90 {si[2]:.2f}) with the terminal "
        f"answering OSC 11, {mute[0]:.2f} ms (min {mute[1]:.2f}, p90 {mute[2]:.2f}) with no answer; "
        f"/bin/true baseline median {base[0]:.2f} ms (min {base[1]:.2f}, p90 {base[2]:.2f}); N={runs}",
        file=sys.stderr,
    )
    frames = [
        sorted(frame_ms(binary, path, answer) * 1000 for _ in range(runs // 2)) for answer in (1, 0)
    ]
    print(
        f"first frame: median {frames[0][runs // 4]:.2f} ms with the terminal answering OSC 11, "
        f"{frames[1][runs // 4]:.2f} ms with no answer (N={runs // 2})",
        file=sys.stderr,
    )
    print(f"median_ms={si[0]:.3f}")
    print(f"median_unanswered_ms={mute[0]:.3f}")
    print(f"first_frame_ms={frames[0][runs // 4]:.3f}")
    print(f"first_frame_unanswered_ms={frames[1][runs // 4]:.3f}")
    print(f"baseline_ms={base[0]:.3f}")
    print(f"runs={runs}")


def rss(binary, path):
    if not os.path.exists("/proc/self/status"):
        sys.exit("pty_bench: rss needs /proc (Linux)")
    master, slave = open_pty()
    # Run next to the file, so that its short name leaves room for NORMAL on the status line.
    where, name = os.path.split(os.path.abspath(path))
    proc = subprocess.Popen(
        [os.path.abspath(binary), name], cwd=where, stdin=slave, stdout=slave, stderr=slave, env=ENV
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


def firstframe(binary, path):
    """Starts `binary path` in a pty whose input already holds the answer to the OSC 11 query (the
    terminal answered in time, whatever the speed of this script), and looks at everything si draws
    up to its status line, the end of the first frame. Best of 10, so a loaded machine that starts
    si's input thread late (past the 3 ms) does not fail the check."""
    best = None
    for _ in range(10):
        master, slave = open_pty()
        os.write(master, ANSWER)
        where, name = os.path.split(os.path.abspath(path))
        proc = subprocess.Popen(
            [os.path.abspath(binary), name], cwd=where, stdin=slave, stdout=slave, stderr=slave, env=ENV
        )
        try:
            seen, deadline = b"", time.time() + 10
            while b"NORMAL" not in seen:  # the status line of the first frame
                if time.time() > deadline or proc.poll() is not None:
                    sys.exit(f"pty_bench: no first frame from {binary}: {seen[-200:]!r}")
                if select.select([master], [], [], 0.05)[0]:
                    seen += os.read(master, 65536)
            os.write(master, b":q\r")
            proc.wait(timeout=10)
        finally:
            if proc.poll() is None:
                proc.kill()
            os.close(slave)
            os.close(master)
        frame = seen[seen.index(b"\x1b[?1049h"):]  # what echo of the answer came before is not it
        colored, faint = int(b"38;2;230;230;230" in frame), int(b"\x1b[0;2m" in frame)
        if best is None or (colored, -faint) > best:
            best = (colored, -faint)
        if (colored, faint) == (1, 0):
            break
    print(f"colored={best[0]}")
    print(f"faint={-best[1]}")


def main(argv):
    if len(argv) >= 4 and argv[1] == "startup":
        startup(argv[2], argv[3], int(argv[4]) if len(argv) > 4 else 200)
    elif len(argv) == 4 and argv[1] == "rss":
        rss(argv[2], argv[3])
    elif len(argv) == 4 and argv[1] == "firstframe":
        firstframe(argv[2], argv[3])
    else:
        sys.exit(__doc__)


main(sys.argv)
