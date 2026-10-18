//! VIS-4, VI-9 with the real binary in a pty (scripts/pty_bench.py plays the terminal): the first
//! frame in color when the terminal's answer is in time, no query before a quit at startup, no answer
//! left for the shell, and Esc and the next key arriving in one write being two keys. The binary is
//! `sim --plain`, which is si as it was: calm, one file, no panel.
#![allow(clippy::unwrap_used)] // the pty helper; the tests themselves may unwrap
mod common;
use common::*;
use std::process::Command;

/// The binary as a command (`pty_bench.py` splits it): plain mode, one file.
const SIM: &str = concat!(env!("CARGO_BIN_EXE_sim"), " --plain");

/// The real binary in a pty (scripts/pty_bench.py firstframe), the terminal's answer to the query
/// already there when it starts, as it is when the terminal is quick: the first frame is in color,
/// with nothing of the faint stand-in before it.
#[test]
fn vis_4_an_answer_in_time_colors_the_first_frame() {
    if !cfg!(target_os = "linux") {
        return eprintln!("skipped: the pty is only trusted to hold the answer ready on Linux");
    }
    let dir = temp_dir("vis4");
    let file = dir.0.join("f.rs");
    std::fs::write(&file, code_text(200, 60)).unwrap();
    let args = ["firstframe", SIM, file.to_str().unwrap()];
    let Some(m) = pty_bench(&args) else { return };
    assert_eq!((m["colored"], m["faint"]), (1.0, 0.0), "the first frame was not in color");
}

/// The real binary in a pty (scripts/pty_bench.py asks): `sim +q` quits before it asks the terminal
/// anything, since the answer would come after it had gone, to be typed into the shell. A start
/// that stays does ask.
#[test]
fn vis_4_a_quit_at_startup_asks_for_no_color() {
    let dir = temp_dir("vis4-asks");
    let file = dir.0.join("f.rs");
    std::fs::write(&file, code_text(20, 40)).unwrap();
    let args = ["asks", SIM, file.to_str().unwrap()];
    let Some(m) = pty_bench(&args) else { return };
    assert_eq!((m["queried_quit"], m["queried_open"]), (0.0, 1.0), "+q asks, or a start does not");
}

/// COLOR-2 on the real binary in a pty (scripts/pty_bench.py --no-term): a start where TERM is unset
/// (a CI runner has none; sim does not guess one) cannot be told to do 8 colors, so it draws none and
/// asks the terminal for none (VIS-4's OSC 11 query), `+q` or not; a quit then waits for no answer, not
/// even the 100 ms it allows a terminal that was asked and said nothing.
#[test]
fn color_2_a_start_with_no_term_asks_the_terminal_nothing_and_a_quit_waits_for_none() {
    let dir = temp_dir("color2-noterm");
    let file = dir.0.join("f.rs");
    std::fs::write(&file, code_text(20, 40)).unwrap();
    let file = file.to_str().unwrap();
    let m = pty_bench(&["--no-term", "asks", SIM, file]).unwrap();
    assert_eq!((m["queried_quit"], m["queried_open"]), (0.0, 0.0), "it asked with TERM unset");
    if cfg!(target_os = "linux") {
        let m = pty_bench(&["--no-term", "firstframe", SIM, file]).unwrap();
        assert_eq!(m["colored"], 0.0, "the first frame is in color, though TERM is unset");
    }
    let m = pty_bench(&["--no-term", "quit", SIM, file, "silent"]).unwrap();
    let ms = m["quit_ms"];
    assert!(ms < 90.0, "a quit waited {ms:.1} ms for a terminal it did not ask");
}

/// The real binary in a pty (scripts/pty_bench.py quitnow): a quit at startup was never answered by
/// a terminal because it never asked, so it does not wait the 100 ms the bound allows (VIS-4). A
/// start that did ask and was not answered (the case above) is the one that waits.
#[test]
fn vis_4_a_quit_at_startup_waits_for_no_terminal_it_did_not_ask() {
    let dir = temp_dir("vis4-quitnow");
    let file = dir.0.join("f.rs");
    std::fs::write(&file, code_text(20, 40)).unwrap();
    let Some(m) = pty_bench(&["quitnow", SIM, file.to_str().unwrap()]) else { return };
    assert!(
        m["quitnow_ms"] < 60.0,
        "`+q` took {:.1} ms: it waited for a terminal",
        m["quitnow_ms"]
    );
}

/// The real binary in a pty (scripts/pty_bench.py leak), on a terminal that answers 30 ms late, in
/// order, and `:q` at the first frame: sim does not leave before the terminal is done answering, so
/// nothing of it is left for the shell. A terminal that never answers costs a quit 100 ms at most.
#[test]
fn vis_4_a_quit_before_the_color_came_leaves_nothing_for_the_shell() {
    let dir = temp_dir("vis4-leak");
    let file = dir.0.join("f.rs");
    std::fs::write(&file, code_text(20, 40)).unwrap();
    let (si, file) = (SIM, file.to_str().unwrap());
    let Some(m) = pty_bench(&["leak", si, file, "30"]) else { return };
    assert_eq!(m["leaked"], 0.0, "the shell would have typed the terminal's answer");
    assert!(
        m["quit_ms"] >= m["owed_ms"] - 5.0,
        "it quit in {:.1} ms, before the answer ({:.1} ms from the `:q`) could be in",
        m["quit_ms"],
        m["owed_ms"]
    );
    for (kind, most) in [("answering", 80.0), ("no_osc", 80.0), ("silent", 400.0)] {
        let m = pty_bench(&["quit", si, file, kind]).unwrap();
        assert!(m["quit_ms"] < most, "a quit with a {kind} terminal took {:.1} ms", m["quit_ms"]);
        let waits = kind == "silent";
        assert_eq!(m["quit_ms"] >= 90.0, waits, "{kind}: {:.1} ms", m["quit_ms"]);
    }
}

const DRIVER: &str = r#"
import os, pty, sys, time, select, struct, fcntl, termios, signal
exe, f, payload = sys.argv[1], sys.argv[2], sys.argv[3].encode().decode('unicode_escape').encode('latin1')
pid, fd = pty.fork()
if pid == 0:
    # the child's own environment: nothing of the terminal this runs in (TERM_PROGRAM, COLORTERM, TMUX...)
    here = os.path.dirname(f)
    env = {'PATH': os.environ.get('PATH', '/usr/bin:/bin'), 'HOME': here, 'XDG_STATE_HOME': here,
           'TERM': 'xterm-256color', 'LANG': 'en_US.UTF-8',
           'GIT_CONFIG_GLOBAL': '/dev/null', 'GIT_CONFIG_SYSTEM': '/dev/null'}
    args = exe.split()
    os.execve(args[0], args + [f], env)
fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack('HHHH', 24, 80, 0, 0))
def drain(t):
    end = time.time() + t
    while time.time() < end:
        if select.select([fd], [], [], 0.05)[0]:
            try: os.read(fd, 65536)
            except OSError: return
drain(0.5)
os.write(fd, payload)
drain(1.0)
# The pty closes when si starts to exit, a moment before it can be reaped: ask for a few seconds.
wpid, end = 0, time.time() + 3
while wpid == 0 and time.time() < end:
    wpid, _ = os.waitpid(pid, os.WNOHANG)
    if wpid == 0: time.sleep(0.02)
if wpid == 0:
    os.kill(pid, signal.SIGKILL); os.waitpid(pid, 0)
print('alive' if wpid == 0 else 'exited')
"#;

/// Types `payload` (Python escapes) in one write into the real binary in a pty that ignores the
/// kitty protocol, editing a file that holds `before`. Returns whether si has quit, and what the
/// file holds. `None` without python3.
fn drive(before: &str, payload: &str) -> Option<(bool, String)> {
    let d = temp_dir("vi9-pty");
    let f = d.0.join("f.txt");
    std::fs::write(&f, before).unwrap();
    let exe = SIM;
    let out = Command::new("python3")
        .args(["-c", DRIVER, exe, f.to_str().unwrap(), payload])
        .output()
        .ok()?;
    let exited = String::from_utf8_lossy(&out.stdout).trim() == "exited";
    Some((exited, std::fs::read_to_string(&f).unwrap()))
}

#[test]
fn vi_9_esc_and_the_next_key_arriving_together_are_two_keys() {
    let Some((exited, file)) = drive("start\n", r"AX\x1b:wq\r") else { return };
    assert!(exited, "still in Insert mode: `wq` was typed into the text");
    assert_eq!(file, "startX\n");
    let Some((exited, file)) = drive("one\ntwo\n", r"jIX\x1bggdd:wq\r") else { return };
    assert_eq!((exited, file.as_str()), (true, "Xtwo\n"));
}
