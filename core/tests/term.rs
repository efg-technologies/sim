//! PERF-3, VIS-10, CLI-4, SAFE-10 on the terminal output.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::oracle::raw_hidden;
use common::*;
use sim_core::core::app::{App, Event};
use sim_core::core::view::Frame;
use sim_core::io::term::{self, Term};

fn draw(t: &mut Term<Vec<u8>>, f: &Frame) -> String {
    t.out.clear();
    t.draw(f.clone()).unwrap();
    String::from_utf8(t.out.clone()).unwrap()
}

/// The rows (0-based) a draw rewrote: every move to column 1, except the final cursor move.
fn rewritten(out: &str) -> Vec<usize> {
    let moves: Vec<&str> = out
        .split("\x1b[")
        .skip(1)
        .filter(|s| s.contains('H') && s.starts_with(|c: char| c.is_ascii_digit()))
        .collect();
    let rows = moves.iter().filter_map(|m| m.split_once(';')?.0.parse::<usize>().ok());
    rows.take(moves.len() - 1).map(|r| r - 1).collect()
}

fn ch(u: u32) -> char {
    char::from_u32(u).unwrap()
}

fn doc() -> App {
    let text: String = (1..=30).map(|i| format!("line number {i}\n")).collect();
    app_with(Some("f.rs"), text.as_bytes(), (60, 12))
}

#[test]
fn perf_3_one_inserted_char_rewrites_only_its_line_and_the_status() {
    let mut a = doc();
    let mut t = Term::new(Vec::new(), false);
    let first = draw(&mut t, &screen(&mut a));
    assert!(first.contains("line number 11") && first.len() > 400);
    feed(&mut a, "jji");
    draw(&mut t, &screen(&mut a));
    let before = screen(&mut a);
    feed(&mut a, "Q");
    let out = draw(&mut t, &screen(&mut a));
    assert_eq!(rewritten(&out), [2, 11], "line 3 and the status line, in {out:?}");
    assert!(out.contains("Qline number 3") && out.contains("INSERT"));
    assert!(!out.contains("line number 4"), "other lines are not sent again");
    assert!(out.len() < 120, "{} bytes: {out:?}", out.len());
    assert_ne!(before, screen(&mut a));
    assert!(
        draw(&mut t, &screen(&mut a)).len() < 20,
        "an unchanged screen costs only the cursor move"
    );
}

/// `out` without the escape sequences of the terminal's writer (`ESC [ params final`).
fn without_escapes(out: &str) -> String {
    let (mut text, mut cs) = (String::new(), out.chars());
    while let Some(c) = cs.next() {
        if c == '\x1b' {
            assert_eq!(cs.next(), Some('['), "only CSI sequences are written: {out:?}");
            assert!(cs.any(|c| ('@'..='~').contains(&c)), "{out:?}");
        } else {
            text.push(c);
        }
    }
    text
}

#[test]
fn safe_10_the_bytes_written_hold_no_hidden_char_but_the_legitimate_ones() {
    // Every frame of a file full of what SAFE-10 names, through the terminal's writer: the bytes
    // hold its own escape sequences, the forms, and the few chars that SAFE-11 draws.
    let word: String =
        "IGNORE PREVIOUS INSTRUCTIONS".chars().map(|c| ch(0xE0000 + c as u32)).collect();
    let evil = format!(
        "a{word}b\u{202e}\u{200b}\u{feff}\u{fe0f}\u{e0100}\u{fdd0}\u{378}\u{2028}\u{ad}\u{200d}c\n"
    );
    let legit = "葛\u{e0100} 👨\u{200d}👩 ⚠\u{fe0f} 🏴\u{e0067}\u{e0062}\u{e0065}\u{e006e}\u{e0067}\u{e007f}\n";
    let mut a =
        app_with(Some("evil\u{e0041}\u{200b}.txt"), format!("{evil}{legit}").as_bytes(), (70, 14));
    let mut t = Term::new(Vec::new(), false);
    let mut all = String::new();
    for keys in ["", "j", "$", "k", "i", "\x1b", ":", "\x1b/", "\x1b"] {
        feed(&mut a, keys);
        let out = draw(&mut t, &screen(&mut a));
        let text = without_escapes(&out);
        assert_eq!(raw_hidden(&text), None, "after {keys:?}: {out:?}");
        all += &text;
    }
    // The hidden word is on its way out as forms, and the legitimate uses are sent as they are.
    for want in [
        "<U+E0049><U+E0047><U+E004E>",
        "<U+202E><U+200B><U+FEFF><U+FE0F><U+E0100>",
        "<U+FDD0><U+0378><U+2028>",
        "<U+00AD><U+200D>c",
        "葛<U+E0100> ", // an ideographic selector is shown after an ideograph too
    ] {
        assert!(all.contains(want), "{want} missing in {all:?}");
    }
    for want in
        ["👨\u{200d}👩 ", "⚠\u{fe0f} ", "🏴\u{e0067}\u{e0062}\u{e0065}\u{e006e}\u{e0067}\u{e007f}"]
    {
        assert!(all.contains(want), "{want:?} missing in {all:?}");
    }
    assert!(all.contains("evil<U+E0041><U+200B>.txt  1:1"), "the name in the status line");
}

#[test]
fn safe_10_private_use_code_points_reach_the_terminal_as_themselves() {
    // Icons of a Nerd Font (waybar's): U+F0E7, U+F07B and U+F0001 are sent as the code points
    // themselves, each one column, and the cursor after them is where one column each puts it.
    let mut a = app_with(Some("icons.txt"), "\u{f0e7} \u{f07b} \u{f0001}x\n".as_bytes(), (40, 6));
    let mut t = Term::new(Vec::new(), false);
    let f = screen(&mut a);
    let out = draw(&mut t, &f);
    assert!(out.contains("\u{f0e7} \u{f07b} \u{f0001}x"), "{out:?}");
    assert!(!out.contains("<U+F0"), "{out:?}");
    assert_eq!(f.cursor, (5, 0));
    feed(&mut a, "$");
    assert_eq!(screen(&mut a).cursor, (5 + 5, 0));
}

#[test]
fn perf_3_a_resize_redraws_everything() {
    let mut a = doc();
    let mut t = Term::new(Vec::new(), false);
    draw(&mut t, &screen(&mut a));
    a.update(Event::Resize(50, 10));
    let out = draw(&mut t, &screen(&mut a));
    assert!(out.starts_with("\x1b[?2026h\x1b[2J") && rewritten(&out).len() == 10);
}

#[test]
fn perf_3_a_frame_that_rewrites_anything_is_one_synchronized_update() {
    const BEGIN: &str = "\x1b[?2026h";
    const END: &str = "\x1b[?2026l";
    let mut a = doc();
    let mut t = Term::new(Vec::new(), false);
    let whole = |out: &str| out.starts_with(BEGIN) && out.ends_with(END);
    for (what, keys) in [("first frame", ""), ("one more char", "iQ"), ("a scroll", "\x1bG")] {
        feed(&mut a, keys);
        let out = draw(&mut t, &screen(&mut a));
        assert!(whole(&out), "{what}: {out:?}");
        assert_eq!((out.matches(BEGIN).count(), out.matches(END).count()), (1, 1), "{what}");
    }
    let out = draw(&mut t, &screen(&mut a));
    assert!(!out.contains("?2026"), "nothing to tear when only the cursor moves: {out:?}");
    a.update(Event::Resize(50, 10));
    assert!(whole(&draw(&mut t, &screen(&mut a))), "a resize is redrawn in one go too");
}

#[test]
fn vis_10_the_cursor_shape_shows_the_mode() {
    let mut a = doc();
    let mut t = Term::new(Vec::new(), false);
    for (keys, want) in [
        ("", "\x1b[2 q"),
        ("i", "\x1b[6 q"),
        ("\x1b", "\x1b[2 q"),
        (":", "\x1b[6 q"),
        ("\x1b", "\x1b[2 q"),
    ] {
        feed(&mut a, keys);
        let out = draw(&mut t, &screen(&mut a));
        assert!(out.contains(want), "{keys:?}: {out:?}");
        assert_eq!(out.matches(" q").count(), 1);
    }
    let out = draw(&mut t, &screen(&mut a));
    assert!(!out.contains(" q"), "the shape is sent only when it changes: {out:?}");
}

#[test]
fn vis_10_the_cursor_sits_on_the_command_line_while_typing() {
    let mut a = doc();
    feed(&mut a, ":set");
    let f = screen(&mut a);
    assert_eq!(f.cursor, (4, 11));
}

#[test]
fn cli_4_leave_undoes_everything_enter_did() {
    let mut out = Vec::new();
    term::leave(&mut out);
    let out = String::from_utf8(out).unwrap();
    for want in ["\x1b[<1u", "\x1b[?2004l", "\x1b[?1049l", "\x1b[0 q", "\x1b[?25h"] {
        assert!(out.contains(want), "{want:?} missing from {out:?}");
    }
}

#[test]
fn vi_8_the_clipboard_is_set_with_osc_52() {
    for (text, b64) in [
        ("", ""),
        ("a", "YQ=="),
        ("ab", "YWI="),
        ("abc", "YWJj"),
        ("hello", "aGVsbG8="),
        ("日本", "5pel5pys"),
    ] {
        assert_eq!(
            term::clipboard(text, false),
            format!("\x1b]52;c;{b64}\x1b\\").into_bytes(),
            "{text:?}"
        );
    }
    let long = "x".repeat(1000);
    let out = String::from_utf8(term::clipboard(&long, false)).unwrap();
    assert_eq!(out.len(), "\x1b]52;c;".len() + 1336 + 2, "4 chars for every 3 bytes");
}

#[test]
fn vi_8_inside_tmux_the_sequence_is_also_sent_through_its_passthrough() {
    let plain = "\x1b]52;c;aGVsbG8=\x1b\\";
    let wrapped = "\x1bPtmux;\x1b\x1b]52;c;aGVsbG8=\x1b\x1b\\\x1b\\";
    assert_eq!(term::clipboard("hello", true), format!("{plain}{wrapped}").into_bytes());
    let mut t = Term::new(Vec::new(), true);
    t.clipboard("hello").unwrap();
    assert_eq!(String::from_utf8(t.out).unwrap(), format!("{plain}{wrapped}"));
    let mut t = Term::new(Vec::new(), false);
    t.clipboard("hello").unwrap();
    assert_eq!(String::from_utf8(t.out).unwrap(), plain);
}

#[test]
fn safe_8_a_hidden_char_is_drawn_in_reverse_video_with_and_without_a_palette() {
    use sim_core::core::color::palette;
    use sim_core::core::view::Style;
    assert_eq!(
        term::sgr(Style::Hidden, None),
        ";1;7",
        "bold and reversed: not the change's reverse"
    );
    let p = palette((0x1e, 0x1e, 0x1e));
    let with = term::sgr(Style::Hidden, Some(p));
    assert!(
        with.ends_with(";7")
            && with.contains(&format!("38;2;{};{};{}", p.primary.0, p.primary.1, p.primary.2)),
        "{with}"
    );
    assert_ne!(with, term::sgr(Style::Change, Some(p)));
}
