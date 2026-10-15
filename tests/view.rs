//! VIS-1, VIS-6, VIS-8, VIS-9, VIS-11 and SAFE-8: what the Frame shows.
mod common;
use common::*;
use si::core::app::Event;
use si::core::view::{Frame, Shape, Style};
use unicode_width::UnicodeWidthStr;

/// The text of a screen row without its gutter (marker, 3 digits, space).
fn body(f: &Frame, row: usize) -> &str {
    f.rows[row].text.get(5..).unwrap_or_default()
}

fn show(path: Option<&str>, text: &str, size: (u16, u16)) -> Frame {
    screen(&mut app_with(path, text.as_bytes(), size))
}

#[test]
fn vis_1_only_two_text_levels() {
    let mut a = app_with(Some("f.rs"), b"one\ntwo\n\tthree\n", (30, 6));
    for keys in ["", "jj", "ox", "\x1b", ":w", "\x1b", "dd", "u"] {
        feed(&mut a, keys);
        for row in &screen(&mut a).rows {
            assert!(row.underline.is_empty(), "{row:?}");
            assert!(row.spans.iter().all(|(_, s)| *s == Style::Secondary), "{row:?}");
        }
    }
    let f = show(Some("f.rs"), "one\ntwo", (30, 6));
    assert_eq!(f.rows[1].spans, [(1..4, Style::Secondary)], "only the line number is Secondary");
    assert!(f.rows[0].spans.is_empty(), "except the cursor line's (VIS-11)");
    assert!(f.rows[5].spans.is_empty(), "the status line is Primary");
}

#[test]
fn vis_6_prose_wraps_at_72_and_code_at_the_screen_width() {
    let words = "ab ".repeat(200);
    for (path, want) in [
        (Some("a.md"), 72),
        (Some("a.txt"), 72),
        (Some("COMMIT_EDITMSG"), 72),
        (Some("Makefile"), 72),
        (None, 72),
        (Some("a.rs"), 95),
        (Some("a.markdown"), 95),
    ] {
        let f = show(path, &words, (100, 10));
        let widths: Vec<_> = (0..5).map(|r| body(&f, r).len()).collect();
        assert!(widths.iter().all(|&w| w <= want && w + 3 > want), "{path:?}: {widths:?}");
    }
    let f = show(Some("a.md"), &words, (40, 10));
    assert!((0..5).all(|r| body(&f, r).len() <= 35), "a narrow screen wraps at its own width");
}

#[test]
fn vis_6_continuation_rows_have_no_line_number() {
    let f = show(Some("a.rs"), &format!("{}\nsecond", "word ".repeat(10)), (30, 8));
    assert_eq!(f.rows[0].text[..5], *"   1 ");
    assert_eq!((f.rows[1].text.trim(), f.rows[1].spans.len()), ("word word word word word", 0));
    assert_eq!(f.rows[1].text[..5], *"     ");
    assert_eq!(f.rows[2].text, "   2 second");
}

#[test]
fn vis_6_rows_break_after_a_space_or_at_the_edge() {
    let rows = |text: &str, cols: u16| -> Vec<String> {
        let f = show(Some("a.rs"), text, (5 + cols, 8));
        (0..6).map(|r| body(&f, r).to_string()).filter(|r| !r.is_empty()).collect()
    };
    assert_eq!(rows("aaaa bbbb cccc", 9), ["aaaa ", "bbbb cccc"]);
    assert_eq!(rows("abcdefghijkl", 5), ["abcde", "fghij", "kl"]);
    assert_eq!(rows("日本語の文章", 6), ["日本語", "の文章"]);
    assert_eq!(rows("日本語の文章", 5), ["日本", "語の", "文章"], "a wide char is never split");
    assert_eq!(rows("ab日本cd", 4), ["ab日", "本cd"]);
}

#[test]
fn vis_8_tabs_are_four_columns_with_stops_every_four() {
    let f = show(Some("a.rs"), "\tx\nab\tx\nabcd\tx\n\t\tx", (40, 8));
    let rows: Vec<_> = (0..4).map(|r| body(&f, r)).collect();
    assert_eq!(rows, ["    x", "ab  x", "abcd    x", "        x"]);
}

#[test]
fn vis_9_status_line_golden() {
    let mut out = String::new();
    let mut snap =
        |name: &str, a: &mut si::core::app::App| out += &format!("== {name}\n{}", dump(&screen(a)));
    let mut a = app_with(Some("src/main.rs"), b"fn main() {\n}\n", (36, 4));
    snap("open", &mut a);
    feed(&mut a, "jx");
    snap("modified", &mut a);
    feed(&mut a, "ihi");
    snap("insert", &mut a);
    feed(&mut a, "\x1b:wq");
    snap("command line", &mut a);
    feed(&mut a, "\nuuu");
    snap("message replaces the status line", &mut a);
    feed(&mut a, "l");
    snap("until the next key", &mut a);
    let mut a = app_with(None, b"", (36, 4));
    feed(&mut a, "l"); // the intro of CLI-5 goes with the first key
    snap("unnamed", &mut a);
    snap("read-only", &mut app_with(Some("blob"), &[0xff, 0xfe, b'a'], (60, 4)));
    let mut a =
        si::core::app::App::new(Some("dir".into()), Err("dir is a directory".into()), (60, 4));
    snap("unreadable", &mut a);
    golden("vis_9_status_line", &out);
}

#[test]
fn vis_9_column_counts_characters_from_one() {
    let mut a = app_with(Some("f"), "日本語\nabc".as_bytes(), (40, 4));
    feed(&mut a, "ll");
    assert_eq!(status(&screen(&mut a)), "f  1:3  NORMAL");
    feed(&mut a, "jx");
    assert_eq!(status(&screen(&mut a)), "f  2:2  NORMAL  modified");
}

#[test]
fn vis_11_no_decoration_golden() {
    let mut a = app_with(Some("notes.txt"), "alpha\nbeta\n\ngamma".as_bytes(), (30, 8));
    feed(&mut a, "j");
    let f = screen(&mut a);
    golden("vis_11_no_decoration", &dump(&f));
    let all: String = f.rows.iter().map(|r| r.text.as_str()).collect();
    assert!(!all.contains('~'), "no ~ past the end of the file");
    assert!(
        !all.chars().any(|c| ('\u{2500}'..='\u{257f}').contains(&c) || c == '\u{2022}'),
        "no box drawing or icons"
    );
    assert!(f.rows[1].spans.is_empty(), "the cursor line has no highlight, only a primary number");
    assert_eq!(f.rows[2].spans, [(1..4, Style::Secondary)], "and the others a secondary one");
    assert!(
        f.rows[1].underline.is_empty() && f.rows[4..7].iter().all(|r| *r == Default::default())
    );
}

/// VIS-11: the cue to where the cursor is: its line's number is primary, every other one secondary,
/// wherever the cursor goes, and a wrapped row (which has no number) never has one.
#[test]
fn vis_11_the_cursor_lines_number_is_primary_and_the_others_secondary() {
    let mut a = app_with(
        Some("f.rs"),
        b"one
two
three
four
",
        (30, 8),
    );
    for (keys, cursor_row) in [("", 0), ("j", 1), ("jj", 3), ("gg", 0), ("G", 3)] {
        feed(&mut a, keys);
        let f = screen(&mut a);
        for r in 0..4 {
            let number = f.rows[r].spans.contains(&(1..4, Style::Secondary));
            assert_eq!(number, r != cursor_row, "{keys:?} row {r}");
        }
    }
    let mut a = app_with(Some("f.rs"), "x".repeat(50).as_bytes(), (20, 6));
    let f = screen(&mut a);
    assert!(f.rows.iter().all(|r| r.spans.is_empty()), "wrapped rows have no number to dim");
    // The whole screen, with the cursor on the third line: only that number is not secondary.
    let mut a = app_with(Some("f.rs"), b"one\ntwo\nthree\nfour\n", (20, 6));
    feed(&mut a, "jj");
    golden("vis_11_cursor_line_number", &dump(&screen(&mut a)));
}

#[test]
fn safe_8_nothing_dangerous_reaches_the_terminal_raw() {
    let mut text = String::new();
    let controls = ('\0'..='\u{1f}').filter(|&c| c != '\n').chain('\u{7f}'..='\u{9f}');
    for c in controls.chain(['\u{200b}', '\u{200e}', '\u{200f}', '\u{feff}']) {
        text.extend([c, 'x']);
        text.push('\n');
    }
    text.extend(('\u{202a}'..='\u{202e}').chain('\u{2066}'..='\u{2069}'));
    // A BOM at the very start is not part of the text; a second one is.
    let bytes = format!("\u{feff}{text}\u{feff}");
    for width in [80, 24] {
        let f = show(Some("evil.txt"), &bytes, (width, 100));
        for row in &f.rows {
            let bad = |c: char| {
                c.is_control() && c != '\n' || "\u{200b}\u{200e}\u{200f}\u{feff}".contains(c)
            };
            assert!(
                !row.text.chars().any(bad)
                    && !row.text.chars().any(|c| ('\u{202a}'..='\u{202e}').contains(&c)
                        || ('\u{2066}'..='\u{2069}').contains(&c)),
                "{row:?}"
            );
            assert_eq!(
                row.text.matches('<').count(),
                row.text.matches('>').count(),
                "a token was split: {row:?}"
            );
        }
        let shown: String = f.rows.iter().map(|r| r.text.as_str()).collect();
        for want in [
            "^@", "^[", "^?", "^M", "<U+0085>", "<U+009F>", "<U+200B>", "<U+202E>", "<U+2069>",
            "<U+FEFF>", "<U+200E>",
        ] {
            assert!(shown.contains(want), "{want} missing at width {width}");
        }
    }
}

#[test]
fn safe_8_the_status_line_and_command_line_are_safe_too() {
    let mut a = app_with(Some("\x1b[2Jevil\u{202e}.txt"), b"x", (60, 4));
    let f = screen(&mut a);
    assert_eq!(status(&f), "^[[2Jevil<U+202E>.txt  1:1  NORMAL");
    feed(&mut a, ":");
    a.update(Event::Paste("a\u{1b}b".into()));
    let f = screen(&mut a);
    assert_eq!(status(&f), ":a^[b");
    assert_eq!((f.cursor, f.shape), ((5, 3), Shape::Bar));
}

#[test]
fn safe_8_the_arabic_letter_mark_is_shown_like_the_other_bidi_controls() {
    let f = show(Some("a.txt"), "a\u{61c}b\n", (30, 4));
    assert_eq!(body(&f, 0), "a<U+061C>b");
    assert!(f.rows.iter().all(|r| !r.text.contains('\u{61c}')));
}

/// No row is ever wider than the screen, even where the gutter, a tab, a wide char or a
/// `<U+202E>` token is wider than the text area.
#[test]
fn vis_6_rows_never_exceed_the_screen_width() {
    for w in 1..=40u16 {
        for path in ["f.rs", "f.md"] {
            for text in [
                "a\tb\n",
                "日本語\n",
                "a\u{202e}b\n",
                "plain text here\n",
                "⚠\u{fe0f}x\n",
                "\x01\x02\n",
            ] {
                let mut a = app_with(Some(path), text.as_bytes(), (w, 4));
                for keys in ["", "$", "i", "\x1bA", "\x1b:abc日本語", "\x1b"] {
                    feed(&mut a, keys);
                    let f = screen(&mut a);
                    for (i, r) in f.rows.iter().enumerate() {
                        assert!(
                            r.text.width() <= usize::from(w),
                            "w={w} {text:?} {keys:?}: row {i} {:?}",
                            r.text
                        );
                    }
                    assert!(f.cursor.0 < w && f.cursor.1 < 4);
                }
            }
        }
    }
}

/// Where the cursor and the text after a cluster are, in columns (the gutter is 5).
fn cursor_after(text: &str, keys: &str, size: (u16, u16)) -> (u16, u16) {
    let mut a = app_with(Some("f.md"), text.as_bytes(), size);
    feed(&mut a, keys);
    screen(&mut a).cursor
}

#[test]
fn vis_7_an_emoji_presentation_sequence_takes_two_columns() {
    // `a` is the char after the cluster: its column is the gutter plus the cluster plus a space.
    for (text, cols) in [
        ("⚠\u{fe0f} abc\n", 3),
        ("✔\u{fe0f}✔\u{fe0f} abc\n", 5),
        ("👨\u{200d}👩\u{200d}👧 abc\n", 3),
        ("❤\u{fe0f}\u{200d}🔥 abc\n", 3),
        ("1\u{fe0f}\u{20e3} abc\n", 3),
        ("😀 abc\n", 3),
        ("⚠ abc\n", 2),
        // Only an emoji joins an emoji, as `unicode-width` reads such a string (never less).
        ("⚠\u{200d}⚠ abc\n", 3),
        ("👨\u{fe0f}\u{200d}😀 abc\n", 5),
        ("❤\u{200d}🔥 abc\n", 4),
        ("a\u{200d}👩 abc\n", 4),
    ] {
        assert_eq!(cursor_after(text, "fa", (40, 6)), (5 + cols, 0), "{text:?}");
    }
    // A ZWJ joins the next char into the cluster, and only when it is there.
    assert_eq!(cursor_after("a\u{200d}b\n", "$", (40, 6)), (5 + 1, 0), "joined chars have width 0");
    assert_eq!(cursor_after("a\u{200d}\n", "$", (40, 6)), (5 + 1, 0));
}

#[test]
fn vis_7_a_row_never_breaks_inside_a_cluster() {
    let rows = |text: &str, cols: u16| -> Vec<String> {
        let f = show(Some("a.rs"), text, (5 + cols, 8));
        (0..6).map(|r| body(&f, r).to_string()).filter(|r| !r.is_empty()).collect()
    };
    assert_eq!(rows("ab⚠\u{fe0f}cd", 3), ["ab", "⚠\u{fe0f}c", "d"]);
    assert_eq!(rows("👨\u{200d}👩ab", 3), ["👨\u{200d}👩a", "b"]);
    assert_eq!(rows("a👨\u{200d}👩\u{200d}👧b", 3), ["a👨\u{200d}👩\u{200d}👧", "b"]);
    assert_eq!(rows("ab👨\u{200d}👩\u{200d}👧c", 3), ["ab", "👨\u{200d}👩\u{200d}👧c"]);
    for f in [
        show(Some("a.rs"), "⚠\u{fe0f}⚠\u{fe0f}⚠\u{fe0f}\n", (9, 6)),
        show(Some("a.rs"), "a⚠\u{fe0f}\n", (7, 6)),
    ] {
        assert!(f.rows.iter().all(|r| r.text.width() <= usize::from(f.width)));
    }
}
