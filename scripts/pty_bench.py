#!/usr/bin/env python3
"""Benchmarks of the real si binary, which needs a terminal: every run gets its own pty, and the
script plays the terminal on the other side of it (class Terminal).

  pty_bench.py startup BIN FILE [N]   wall time of `BIN +q FILE` (PERF-1), N runs (default 200), and
                                      of /bin/true in the same harness as the baseline. `si +q` quits
                                      before it draws and asks the terminal nothing (VIS-4), so the
                                      terminal does not matter to it. `first_frame_ms` is the time to
                                      the first frame of `BIN FILE` and `quit_ms` the time from `:q`
                                      to the exit after it, with the terminal answering at once, with
                                      one that does not know OSC 11 but answers the rest
                                      (`_no_osc_ms`) and with one that answers nothing
                                      (`_unanswered_ms`: the first frame waits its 3 ms out, and the
                                      quit waits the 100 ms it allows the terminal)
  pty_bench.py rss BIN FILE           VmRSS and VmHWM of `BIN FILE` after its first frame
                                      (PERF-5; Linux only)
  pty_bench.py firstframe BIN FILE    whether the first frame of `BIN FILE` is in color when the
                                      terminal's answer to the OSC 11 query is there in time
                                      (VIS-4): `colored` and `faint` (1 or 0), best of 10 starts
  pty_bench.py asks BIN FILE          whether `BIN +q FILE` (`queried_quit`) and `BIN FILE`
                                      (`queried_open`) send the OSC 11 query (1 or 0) (VIS-4)
  pty_bench.py quit BIN FILE KIND     the time from `:q` to the exit after the first frame (`quit_ms`,
                                      median of 3) with a terminal of KIND `answering`, `no_osc` or
                                      `silent`, to see that a quit never waits long (VIS-4)
  pty_bench.py leak BIN FILE MS       a terminal that answers every query MS milliseconds late, in
                                      order; `:q` as soon as the first frame is up. `leaked` is the
                                      number of bytes of its answers that were still unread when si
                                      had gone, i.e. what the shell would have typed (VIS-4);
                                      `quit_ms` the time from `:q` to the exit, `owed_ms` the time from
                                      `:q` to the last answer that was due (what a quit has to wait for)

Prints key=value lines (times in ms, memory in kB) after a readable summary on stderr.
Used by scripts/bench.sh, tests/perf_1.rs, tests/perf_5.rs and tests/vis_4.rs. Standard library only.
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
DA1 = b"\x1b[c"  # and, when it quits before the color came, whether the terminal is done answering
ATTRIBUTES = b"\x1b[?62;c"


class Terminal:
    """The terminal's side of a pty. It reads what si writes and answers the queries it knows the
    way a terminal does: in the order they were sent, each `delay` seconds after it was seen (and
    not before the answer to the one before). `osc=False` is a terminal that does not know OSC 11
    and so never answers it, `silent` one that answers nothing."""

    def __init__(self, master, delay=0.0, osc=True, silent=False):
        self.master, self.delay, self.osc, self.silent = master, delay, osc, silent
        self.seen, self.scanned, self.due, self.last = b"", 0, [], 0.0
        self.queried = set()
        self.owed = 0.0  # seconds the answers still owed were due after the `:q` (see `session`)

    def pump(self, wait=0.0005):
        """Reads what is there (for at most `wait` seconds), schedules the answers to the queries
        in it and sends those that are due."""
        if select.select([self.master], [], [], wait)[0]:
            self.seen += os.read(self.master, 65536)
        while True:
            hits = sorted((self.seen.find(q, self.scanned), q) for q in (QUERY, DA1))
            hits = [h for h in hits if h[0] >= 0]
            if not hits:
                break
            at, query = hits[0]
            self.scanned = at + len(query)
            self.queried.add(query)
            answer = {QUERY: ANSWER, DA1: ATTRIBUTES}[query]
            if not self.silent and (self.osc or query != QUERY):
                self.last = max(self.last, time.perf_counter() + self.delay)
                self.due.append((self.last, answer))
        now = time.perf_counter()
        for item in [d for d in self.due if d[0] <= now]:
            self.due.remove(item)
            os.write(self.master, item[1])


def run_once(argv):
    """Seconds from spawning argv, with a pty as stdin, stdout and stderr, to its exit."""
    master, slave = open_pty()
    try:
        start = time.perf_counter()
        proc = subprocess.Popen(argv, stdin=slave, stdout=slave, stderr=slave, env=ENV)
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


def median_ms(argv, runs, warmup=10):
    for _ in range(warmup):
        run_once(argv)
    times = sorted(run_once(argv) * 1000 for _ in range(runs))
    return statistics.median(times), times[0], times[int(len(times) * 0.9)]


def session(binary, path, **terminal):
    """Runs `binary path` against a Terminal until its first frame, types `:q` at once and waits for
    the exit. Gives back the seconds to the first frame, the seconds from `:q` to the exit, and the
    Terminal (after si has gone, with the pty's slave end still open for whoever looks at it)."""
    master, slave = open_pty()
    where, name = os.path.split(os.path.abspath(path))
    start = time.perf_counter()
    proc = subprocess.Popen(
        [os.path.abspath(binary), name], cwd=where, stdin=slave, stdout=slave, stderr=slave, env=ENV
    )
    term = Terminal(master, **terminal)
    try:
        deadline = time.time() + 10
        while b"NORMAL" not in term.seen:
            if time.time() > deadline or proc.poll() is not None:
                sys.exit(f"pty_bench: no first frame from {binary}: {term.seen[-200:]!r}")
            term.pump()
        took = time.perf_counter() - start
        os.write(master, b":q\r")
        typed = time.perf_counter()
        term.owed = max((d[0] for d in term.due), default=typed) - typed
        while proc.poll() is None and time.time() < deadline:
            term.pump(0.0002)
        quit = time.perf_counter() - typed
        if proc.poll() != 0:
            sys.exit(f"pty_bench: {binary} did not quit with :q ({proc.poll()})")
    except BaseException:
        proc.kill()
        raise
    return took, quit, term, slave, master


def frames(binary, path, runs, **terminal):
    """Median first frame and median quit, in ms, over `runs` sessions."""
    firsts, quits = [], []
    for _ in range(runs):
        took, quit, _, slave, master = session(binary, path, **terminal)
        os.close(slave)
        os.close(master)
        firsts.append(took * 1000)
        quits.append(quit * 1000)
    return statistics.median(firsts), statistics.median(quits)


def startup(binary, path, runs):
    si = median_ms([binary, "+q", path], runs)
    base = median_ms(["/bin/true"], runs)
    print(
        f"startup: si +q median {si[0]:.2f} ms (min {si[1]:.2f}, p90 {si[2]:.2f}); /bin/true "
        f"baseline median {base[0]:.2f} ms (min {base[1]:.2f}, p90 {base[2]:.2f}); N={runs}",
        file=sys.stderr,
    )
    # Silent terminals make every quit wait the 100 ms allowed to the terminal: fewer runs.
    n = max(runs // 4, 5)
    kinds = {
        "": frames(binary, path, runs // 2),
        "_no_osc": frames(binary, path, n, osc=False),
        "_unanswered": frames(binary, path, n, silent=True),
    }
    names = {"": "answering everything", "_no_osc": "answering all but OSC 11", "_unanswered": "answering nothing"}
    for kind, (first, quit) in kinds.items():
        print(
            f"terminal {names[kind]}: first frame median {first:.2f} ms, then :q to exit median {quit:.2f} ms",
            file=sys.stderr,
        )
    print(f"median_ms={si[0]:.3f}")
    for kind, (first, quit) in kinds.items():
        if kind != "_no_osc":
            print(f"first_frame{kind}_ms={first:.3f}")
        print(f"quit{kind}_ms={quit:.3f}")
    print(f"baseline_ms={base[0]:.3f}")
    print(f"runs={runs}")


def asks(binary, path):
    """Whether `+q` and a plain start send the query for the background (VIS-4)."""
    master, slave = open_pty()
    where, name = os.path.split(os.path.abspath(path))
    try:
        proc = subprocess.Popen(
            [os.path.abspath(binary), "+q", name],
            cwd=where, stdin=slave, stdout=slave, stderr=slave, env=ENV,
        )
        term = Terminal(master)
        deadline = time.time() + 10
        while proc.poll() is None and time.time() < deadline:
            term.pump()
        quit_asked = QUERY in term.queried
    finally:
        os.close(slave)
        os.close(master)
    took, quit, term, slave, master = session(binary, path)
    os.close(slave)
    os.close(master)
    print(f"queried_quit={int(quit_asked)}")
    print(f"queried_open={int(QUERY in term.queried)}")


def leak(binary, path, delay_ms):
    """A terminal MS milliseconds late with every answer; si is quit at its first frame. Whatever
    reaches the pty after si has restored it is typed into the shell: look at what is left."""
    took, quit, term, slave, master = session(binary, path, delay=delay_ms / 1000)
    try:
        # Everything the terminal still owes arrives now, after si has gone, as it would.
        until = time.perf_counter() + delay_ms / 1000 + 0.05
        while time.perf_counter() < until or term.due:
            term.pump(0.001)
        attrs = termios.tcgetattr(slave)
        attrs[3] &= ~termios.ICANON  # a line without its end is not readable otherwise
        termios.tcsetattr(slave, termios.TCSANOW, attrs)
        os.set_blocking(slave, False)
        try:
            left = os.read(slave, 4096)
        except BlockingIOError:
            left = b""
    finally:
        os.close(slave)
        os.close(master)
    print(f"leak: {len(left)} bytes left for the shell: {left!r}; :q to exit {quit * 1000:.1f} ms", file=sys.stderr)
    print(f"leaked={len(left)}")
    print(f"quit_ms={quit * 1000:.3f}")
    print(f"owed_ms={term.owed * 1000:.3f}")


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
    elif len(argv) == 4 and argv[1] == "asks":
        asks(argv[2], argv[3])
    elif len(argv) == 5 and argv[1] == "quit":
        kinds = {"answering": {}, "no_osc": {"osc": False}, "silent": {"silent": True}}
        print(f"quit_ms={frames(argv[2], argv[3], 3, **kinds[argv[4]])[1]:.3f}")
    elif len(argv) == 5 and argv[1] == "leak":
        leak(argv[2], argv[3], float(argv[4]))
    else:
        sys.exit(__doc__)


main(sys.argv)
