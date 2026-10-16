//! VI-12: the arrows, Home, End, PageUp, PageDown and Delete work in Normal, Visual and Insert
//! mode, and Left, Right, Home, End, BS and Delete on the command line. (Normal mode is also
//! covered with VI-1, Insert mode with VI-6.)
mod common;
use common::*;
use crossterm::event::{Event as Ev, KeyCode, KeyEvent, KeyModifiers};
use si::core::app::{Event, Key};
use si::core::vi::Mode;
use si::io::term;

fn numbered(n: usize) -> String {
    (1..=n).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n")
}

#[test]
fn vi_12_the_keys_reach_the_app_from_the_terminal() {
    for (code, want) in [
        (KeyCode::Up, Key::Up),
        (KeyCode::Down, Key::Down),
        (KeyCode::Left, Key::Left),
        (KeyCode::Right, Key::Right),
        (KeyCode::Home, Key::Home),
        (KeyCode::End, Key::End),
        (KeyCode::PageUp, Key::PageUp),
        (KeyCode::PageDown, Key::PageDown),
        (KeyCode::Delete, Key::Delete),
        (KeyCode::Backspace, Key::Backspace),
    ] {
        let ev = term::convert(Ev::Key(KeyEvent::new(code, KeyModifiers::NONE)));
        assert!(matches!(ev, Some(Event::Key(k)) if k == want), "{code:?}");
    }
    let ev = term::convert(Ev::Key(KeyEvent::new(KeyCode::Char('S'), KeyModifiers::CONTROL)));
    assert!(matches!(ev, Some(Event::Key(Key::Ctrl('s')))));
}

/// A terminal set to send ^H (byte 8) for its Backspace key (PuTTY's option, an ssh client, `stty
/// erase ^H`) would otherwise have no Backspace: crossterm reads that byte as Ctrl-h.
#[test]
fn vi_12_ctrl_h_is_backspace_as_in_vi() {
    let ev = term::convert(Ev::Key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::CONTROL)));
    assert!(matches!(ev, Some(Event::Key(Key::Backspace))), "{ev:?}");
    let mut a = app("abc");
    feed(&mut a, "A");
    a.update(ev.unwrap());
    assert_eq!(text(&a), "ab");
    let mut a = app("abc");
    feed(&mut a, ":xy");
    a.update(Event::Key(Key::Backspace));
    assert_eq!(a.vi.line, ":x");
}

#[test]
fn vi_12_visual_mode_moves_the_selection_with_the_keys() {
    table(&[
        ("a|bc", "v<Right>d", "|a"),
        ("a|bc", "v<Left>d", "|c"),
        ("a|bc\ndef", "v<Down>d", "a|f"),
        ("abc\nd|ef", "v<Up>d", "a|f"),
        ("ab|cd", "v<Home>d", "|d"),
        ("a|bcd", "v<End>d", "|a"),
        ("a|bcd", "v<Del>", "a|cd"),
        ("a|bcd", "vl<Del>", "a|d"),
        ("a|bc\ndef", "V<Down><Del>", "|"),
        ("a|b\ncd\nef", "V<Down>d", "|ef"),
    ]);
    let mut a = app(&numbered(100));
    feed(&mut a, "V<PageDown>");
    assert_eq!((a.buf.line_of(a.buf.cursor), a.vi.mode), (23, Mode::VLine));
    feed(&mut a, "<PageUp>");
    assert_eq!(a.buf.line_of(a.buf.cursor), 0);
    assert_eq!(a.selection(), Some((0, 6)));
}

#[test]
fn vi_12_the_page_keys_in_every_mode() {
    for (before, keys) in [("", "<PageDown>"), ("v", "<PageDown>"), ("i", "<PageDown>")] {
        let mut a = app(&numbered(100));
        feed(&mut a, before);
        feed(&mut a, keys);
        assert_eq!(a.buf.line_of(a.buf.cursor), 23, "{before}{keys}");
    }
}

#[test]
fn vi_12_command_line_editing_keys() {
    let line = |a: &mut si::core::app::App| status(&screen(a)).to_string();
    let mut a = typed("|abc", ":abcd");
    assert_eq!(line(&mut a), ":abcd");
    feed(&mut a, "<Left><Left>X");
    assert_eq!(line(&mut a), ":abXcd");
    assert_eq!(screen(&mut a).cursor, (4, 23), "the cursor sits after the typed X");
    feed(&mut a, "<Home>Y");
    assert_eq!(line(&mut a), ":YabXcd");
    feed(&mut a, "<End>Z");
    assert_eq!(line(&mut a), ":YabXcdZ");
    feed(&mut a, "<Left><Left><Del>");
    assert_eq!(line(&mut a), ":YabXcZ");
    feed(&mut a, "<BS><BS>");
    assert_eq!(line(&mut a), ":YabZ");
    feed(&mut a, "<Right><Right><Right><Del>");
    assert_eq!(line(&mut a), ":YabZ", "Right and Delete stop at the end");
    feed(&mut a, "<Home><BS><Left><Left>");
    assert_eq!(line(&mut a), ":YabZ", "BS at the start of a line with text does nothing");
    assert_eq!(screen(&mut a).cursor, (1, 23));
    assert_eq!(a.vi.mode, Mode::Cmd);
}

#[test]
fn vi_12_bs_on_an_empty_command_line_cancels_it() {
    for prefix in [":", "/", "?"] {
        let mut a = typed("|abc", prefix);
        assert_eq!(a.vi.mode, Mode::Cmd);
        feed(&mut a, "x<BS>");
        assert_eq!(a.vi.mode, Mode::Cmd, "the prefix is not text");
        feed(&mut a, "<BS>");
        assert_eq!(a.vi.mode, Mode::Normal, "{prefix}");
    }
}

#[test]
fn vi_12_enter_submits_from_the_middle_and_the_cancel_keys_cancel() {
    let a = typed("|a\nb\nc", ":3<Left><CR>");
    assert_eq!((a.buf.line_of(a.buf.cursor), a.vi.mode), (2, Mode::Normal));
    for cancel in ["<Esc>", "<C-c>"] {
        let mut a = typed("|abc", &format!(":q!x{cancel}"));
        assert_eq!(a.vi.mode, Mode::Normal, "{cancel}");
        assert_eq!(text(&a), "abc");
        assert_eq!(status(&screen(&mut a)), "[no name]  1:1  NORMAL");
    }
    let key = term::convert(Ev::Key(KeyEvent::new(KeyCode::Char('['), KeyModifiers::CONTROL)));
    assert!(matches!(key, Some(Event::Key(Key::Esc))), "Ctrl-[ is Esc");
}

#[test]
fn vi_12_the_command_line_cursor_follows_wide_characters() {
    let mut a = typed("|abc", ":日本");
    assert_eq!(screen(&mut a).cursor, (5, 23));
    feed(&mut a, "<Left>");
    assert_eq!(screen(&mut a).cursor, (3, 23));
}

/// A command line longer than the screen scrolls to keep its cursor on it; the part before is
/// not shown, and moving left or home brings it back.
#[test]
fn vi_12_a_long_command_line_scrolls_with_its_cursor() {
    let mut a = app_with(Some("f.rs"), b"x\n", (40, 6));
    feed(&mut a, &format!(":s/{}/Z", "a".repeat(60)));
    let f = screen(&mut a);
    assert!(status(&f).ends_with("aa/Z"), "{:?}", status(&f));
    assert_eq!(f.cursor, (status(&f).chars().count() as u16, 5), "just after the last char");
    assert!(status(&f).chars().count() < 40);
    // Home shows the start again, End the end.
    feed(&mut a, "<Home>");
    let f = screen(&mut a);
    assert!(
        status(&f).starts_with(":s/aaa") && f.cursor == (1, 5),
        "{:?} {:?}",
        status(&f),
        f.cursor
    );
    feed(&mut a, "<End>");
    assert!(status(&screen(&mut a)).ends_with("aa/Z"));
    // Moving left inside the scrolled part moves the cursor, not the text, until the edge.
    let before = screen(&mut a).cursor.0;
    feed(&mut a, "<Left><Left><Left>");
    let f = screen(&mut a);
    assert!(
        status(&f).ends_with("aa/Z") && f.cursor.0 == before - 3,
        "{:?} {:?}",
        status(&f),
        f.cursor
    );
    // The cursor is on the char it stands for when typing in the middle of a scrolled line.
    feed(&mut a, "<C-c>");
    let mut a = app_with(Some("f.rs"), b"x\n", (10, 6));
    feed(&mut a, ":0123456789abcdef");
    let f = screen(&mut a);
    assert_eq!((status(&f), f.cursor.0), ("789abcdef", 9));
    feed(&mut a, "<Left><Left><Left><Left><Left><Left><Left><Left><Left><Left>");
    let f = screen(&mut a);
    assert_eq!(f.cursor.0, 1, "{:?}", status(&f));
    assert_eq!(status(&f).chars().next(), Some('5'), "the char before the cursor stays on screen");
    // Wide chars are never cut in half.
    let mut a = app_with(Some("f.rs"), b"x\n", (10, 6));
    feed(&mut a, ":日本語日本語日本語");
    let f = screen(&mut a);
    assert!(unicode_width::UnicodeWidthStr::width(status(&f)) <= 9, "{:?}", status(&f));
    assert_eq!(f.cursor.0 as usize, unicode_width::UnicodeWidthStr::width(status(&f)));
}
