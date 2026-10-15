//! VI-9: the kitty keyboard protocol and bracketed paste are on from the start, Esc needs no
//! timeout, and one paste is one undo unit in every mode.
#![allow(clippy::unwrap_used)] // the pty helper; the tests themselves may unwrap
mod common;
use common::*;
use crossterm::event::{Event as Ev, KeyCode, KeyEvent, KeyModifiers};
use si::core::app::{Event, Key};
use si::core::vi::Mode;
use si::io::term::{self, ENTER};
use std::process::Command;

fn has(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

fn key(code: KeyCode, mods: KeyModifiers) -> Option<Event> {
    term::convert(Ev::Key(KeyEvent::new(code, mods)))
}

fn paste(a: &mut si::core::app::App, s: &str) {
    a.update(Event::Paste(s.into()));
}

#[test]
fn vi_9_setup_enables_bracketed_paste_and_pushes_the_kitty_flags_without_asking() {
    assert!(has(ENTER, b"\x1b[?2004h"), "bracketed paste");
    assert!(has(ENTER, b"\x1b[>1u"), "push the kitty flags (disambiguate escape codes)");
    for query in [&b"\x1b[?u"[..], b"\x1b[c", b"\x1b[>c", b"\x1b[?1u"] {
        assert!(!has(ENTER, query), "setup must not wait for a reply to {query:?}");
    }
    assert!(has(term::LEAVE, b"\x1b[<1u") && has(term::LEAVE, b"\x1b[?2004l"));
}

#[test]
fn vi_9_a_lone_esc_is_a_key_at_once() {
    // crossterm turns a lone 0x1b (nothing behind it) into Esc without a timeout, and the kitty
    // flags make the terminal send `CSI 27 u`; both reach `convert` as KeyCode::Esc.
    assert!(matches!(key(KeyCode::Esc, KeyModifiers::NONE), Some(Event::Key(Key::Esc))));
    assert!(matches!(key(KeyCode::Char('['), KeyModifiers::CONTROL), Some(Event::Key(Key::Esc))));
    assert!(matches!(
        term::convert(Ev::Paste("a\nb".into())),
        Some(Event::Paste(s)) if s == "a\nb"
    ));
    check("ab|c", "i<Esc>", "a|bc");
}

#[test]
fn vi_9_a_paste_is_text_in_every_mode() {
    let mut a = typed("a|b", "");
    paste(&mut a, "xy");
    assert_eq!(marked(&a), "ax|yb", "Normal: at the cursor, resting on its last char");
    let mut a = typed("a|b", "i");
    paste(&mut a, "xy");
    assert_eq!((marked(&a), a.vi.mode), ("axy|b".to_string(), Mode::Insert));
    let mut a = typed("a|bcd", "vl");
    paste(&mut a, "xy");
    assert_eq!(
        (a.vi.mode, text(&a).as_str()),
        (Mode::Normal, "abxycd"),
        "Visual ends first, nothing replaced"
    );
    let mut a = typed("|abc", "V");
    paste(&mut a, "x");
    assert_eq!((a.vi.mode, text(&a).as_str()), (Mode::Normal, "xabc"));
    let mut a = typed("|ab", "");
    paste(&mut a, "x:q!\n");
    assert_eq!(text(&a), "x:q!\nab", "a paste is never run as keys");
    paste(&mut a, "");
    assert_eq!(text(&a), "x:q!\nab");
}

#[test]
fn vi_9_a_paste_on_the_command_line_turns_line_breaks_into_spaces() {
    let mut a = typed("|abc", ":");
    paste(&mut a, "s/a/\nb\r\nc\rd/");
    assert_eq!(status(&screen(&mut a)), ":s/a/ b c d/");
    feed(&mut a, "<Left><Left>X");
    assert_eq!(status(&screen(&mut a)), ":s/a/ b c Xd/");
    paste(&mut a, "Y");
    assert_eq!(status(&screen(&mut a)), ":s/a/ b c XYd/");
    let mut a = typed("|abc", "/");
    paste(&mut a, "b\n");
    assert_eq!(status(&screen(&mut a)), "/b ");
}

#[test]
fn vi_9_one_paste_is_one_undo_unit() {
    let big = "one\ntwo\nthree";
    let mut a = typed("|a", "");
    paste(&mut a, big);
    paste(&mut a, big);
    feed(&mut a, "u");
    assert_eq!(text(&a), format!("{big}a"));
    feed(&mut a, "u");
    assert_eq!(text(&a), "a");
    feed(&mut a, "<C-r>");
    assert_eq!(text(&a), format!("{big}a"));

    // Inside an insert session the paste belongs to the session's unit.
    let mut a = typed("|a", "ix");
    paste(&mut a, big);
    feed(&mut a, "y<Esc>");
    assert_eq!(text(&a), format!("x{big}ya"));
    feed(&mut a, "u");
    assert_eq!(text(&a), "a");

    // Visual mode: the paste is its own unit, after the selection was left.
    let mut a = typed("|abcd", "xvl");
    paste(&mut a, "P");
    feed(&mut a, "u");
    assert_eq!(text(&a), "bcd");
    feed(&mut a, "u");
    assert_eq!(text(&a), "abcd");
}

#[test]
fn vi_9_a_paste_into_a_read_only_buffer_is_refused() {
    let mut a = app_with(Some("f"), &[0xff], (40, 6));
    paste(&mut a, "x");
    assert!(
        status(&screen(&mut a)).starts_with("f  ")
            || status(&screen(&mut a)).starts_with("read-only")
    );
    assert!(!a.buf.dirty());
}

/// A paste ends a half-typed command: a pending count or operator would otherwise turn the
/// next key into something the user did not type.
#[test]
fn vi_9_a_paste_ends_a_pending_count_or_operator() {
    let mut a = typed("|abc def", "d");
    paste(&mut a, "X");
    feed(&mut a, "w");
    assert_eq!((text(&a).as_str(), a.buf.cursor), ("Xabc def", 5), "`w` moved, `dw` did not run");
    let mut a = typed("|abcdef", "3");
    paste(&mut a, "X");
    feed(&mut a, "x");
    assert_eq!(text(&a), "abcdef", "a plain `x` deleted the pasted X, not three chars");
    let mut a = typed("a|bc", "2d");
    paste(&mut a, "XY");
    feed(&mut a, "dd");
    assert_eq!(text(&a), "", "`dd` is one line, not a continuation of `2d`");
    let mut a = typed("|abc", "g");
    paste(&mut a, "X");
    feed(&mut a, "g");
    assert_eq!(text(&a), "Xabc");
}

/// A paste leaves the Normal cursor on a char, also when the text ends with a line break.
#[test]
fn vi_9_a_paste_leaves_the_normal_cursor_on_a_char() {
    let mut a = typed("|x", "");
    paste(&mut a, "abc\n");
    assert_eq!(marked(&a), "ab|c\nx", "on the last char of the line, not on its break");
    feed(&mut a, "aZ<Esc>");
    assert_eq!(text(&a), "abcZ\nx");
}

fn alt(code: KeyCode) -> Vec<String> {
    let ev = Ev::Key(KeyEvent::new(code, KeyModifiers::ALT));
    term::convert_all(ev).iter().map(|e| format!("{e:?}")).collect()
}

/// Without the kitty protocol Esc and the next key can arrive together, which looks like
/// Alt+key. si has no Alt commands: it is Esc, then the key.
#[test]
fn vi_9_alt_plus_a_key_is_esc_then_the_key() {
    for (code, then) in [
        (KeyCode::Char(':'), "Key(Char(':'))"),
        (KeyCode::Char('j'), "Key(Char('j'))"),
        (KeyCode::Char('Z'), "Key(Char('Z'))"),
        (KeyCode::Enter, "Key(Enter)"),
        (KeyCode::Backspace, "Key(Backspace)"),
        (KeyCode::Tab, "Key(Tab)"),
    ] {
        assert_eq!(alt(code), ["Key(Esc)", then], "{code:?}");
    }
    let ctrl_alt = KeyEvent::new(KeyCode::Char('d'), KeyModifiers::ALT | KeyModifiers::CONTROL);
    let got: Vec<_> =
        term::convert_all(Ev::Key(ctrl_alt)).iter().map(|e| format!("{e:?}")).collect();
    assert_eq!(got, ["Key(Esc)", "Key(Ctrl('d'))"]);
    // The other keys are as `convert` has them, and the Alt arrows stay dropped.
    assert_eq!(
        term::convert_all(Ev::Key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE))).len(),
        1
    );
    assert!(alt(KeyCode::Left).is_empty());
    assert_eq!(alt(KeyCode::Esc).len(), 0);
    // And the keys do what they would have done typed one after the other.
    let mut a = typed("a|b", "i");
    for k in [KeyCode::Char(':'), KeyCode::Char('s')] {
        assert_eq!(alt(k).len(), 2);
    }
    feed(&mut a, "<Esc>:s/b/c/\n");
    assert_eq!(text(&a), "ac");
}

const DRIVER: &str = r#"
import os, pty, sys, time, select, struct, fcntl, termios, signal
exe, f, payload = sys.argv[1], sys.argv[2], sys.argv[3].encode().decode('unicode_escape').encode('latin1')
pid, fd = pty.fork()
if pid == 0:
    os.environ['TERM'] = 'xterm-256color'
    os.environ['XDG_STATE_HOME'] = os.path.dirname(f)
    os.execv(exe, [exe, f])
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
wpid, _ = os.waitpid(pid, os.WNOHANG)
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
    let exe = env!("CARGO_BIN_EXE_si");
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
