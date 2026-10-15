//! VI-9: the kitty keyboard protocol and bracketed paste are on from the start, Esc needs no
//! timeout, and one paste is one undo unit in every mode.
mod common;
use common::*;
use crossterm::event::{Event as Ev, KeyCode, KeyEvent, KeyModifiers};
use si::core::app::{Event, Key};
use si::core::vi::Mode;
use si::io::term::{self, ENTER};

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
