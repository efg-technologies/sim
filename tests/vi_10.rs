//! VI-10: full-width ASCII (U+FF01–FF5E) is the half-width key in Normal, Visual and REVIEW mode
//! and after an operator, but not the char after `f F t T r`, nor in Insert mode or on the
//! command line. VI-11: `SI_ON_NORMAL` runs (as an Effect::Hook) once at startup and at every entry
//! into Normal mode. Rows are (before, keys, after); `|` is the cursor.
#![allow(clippy::unwrap_used)] // the helper `fw`
mod common;
use common::*;
use si::core::app::{App, Effect, Event, Key};
use si::core::vi::Mode;

/// The full-width spelling of ASCII `s` (U+0021–007E → U+FF01–FF5E).
fn fw(s: &str) -> String {
    s.chars()
        .map(
            |c| {
                if ('!'..='~').contains(&c) {
                    char::from_u32(c as u32 + 0xfee0).unwrap()
                } else {
                    c
                }
            },
        )
        .collect()
}

fn disk(a: &mut App, text: &str) {
    a.update(Event::Disk {
        bytes: Some(text.as_bytes().to_vec()),
        hash: None,
        deadline: far(),
        seq: 0,
    });
}

// ---------------------------------------------------------------------------------------------
// VI-10
// ---------------------------------------------------------------------------------------------

#[test]
fn vi_10_full_width_keys_work_in_normal_mode() {
    table(&[
        ("|abc def", "ｄｗ", "|def"),
        ("|abc def", "ｘ", "|bc def"),
        ("|a\nb\nc", "ｊｄｄ", "a\n|c"),
        ("|a\nb\nc", "Ｇ", "a\nb\n|c"),
        ("a\nb\n|c", "ｇｇ", "|a\nb\nc"),
        ("a b| c", "０", "|a b c"),
        ("|a b c", "＄", "a b |c"),
        ("|a b c", "ｗｗ", "a b |c"),
        ("|abc", "ｘｕ", "|abc"),
        ("|abcd", "２ｘ", "|cd"),
        ("|a b c d", "２ｄｗ", "|c d"),
        ("|a b c d", "ｄｗ．", "|c d"),
        ("|abcd", "ｘ２．", "|d"),
        ("|(a b) c", "％", "(a b|) c"),
        ("|a\nb", "Ｊ", "a| b"),
        ("|a,b,c", "ｆ,；", "a,b|,c"),
        ("|foo bar foo", "＊", "foo bar |foo"),
        ("|a b", "＞＞", "    |a b"),
        ("|a b", "ｙｙｐ", "a b\n|a b"),
        ("|a", "ｏx<Esc>", "a\n|x"),
    ]);
}

#[test]
fn vi_10_a_full_width_key_does_what_its_half_width_does() {
    // Compare with the half-width keys over many commands, on a text where each one does something.
    let text = "|alpha (beta gamma) delta\n  eps, zeta\nlast one";
    for keys in [
        "dw", "dd", "x", "p", "yyp", "ywP", "u", "J", "w", "b", "e", "W", "B", "E", "0", "$", "^",
        "gg", "G", "%", "diw", "daw", "d$", "D", "Y", ">>", "<<", "2dd", "3x", "dG", "dgg", "vjd",
        "Vjd", "vly", "zz", "n", "N", "*", "#", "wwdi(", "jjJ", "jj>>", "wwd%", "$.", "ww3x.",
        "VGd", "vGy", "ggVGyGp",
    ] {
        let want = marked(&typed(text, keys));
        assert_ne!(want, "", "{keys}");
        assert_eq!(marked(&typed(text, &fw(keys))), want, "{keys}");
    }
}

#[test]
fn vi_10_the_char_after_f_t_r_is_the_char_itself() {
    table(&[
        ("|aｂcｂd", "ｆｂ", "a|ｂcｂd"),
        ("|aｂcｂd", "ｆｂ;", "aｂc|ｂd"),
        ("|aｂcb", "fｂ", "a|ｂcb"),
        ("|aｂcb", "fb", "aｂc|b"),
        ("aｂcｂ|d", "Ｆｂ", "aｂc|ｂd"),
        ("|aｂcｂd", "ｔｂ", "|aｂcｂd"),
        ("|abｂd", "ｔｂ", "a|bｂd"),
        ("|abc", "ｒｘ", "|ｘbc"),
        ("|abc", "ｒx", "|xbc"),
        ("|abc", "2ｒｙ", "ｙ|ｙc"),
        ("|aｂc", "ｄｆｂ", "|c"),
        ("|aｂc", "ｄｔｂ", "|ｂc"),
    ]);
}

#[test]
fn vi_10_after_an_operator_and_in_visual_mode() {
    table(&[
        ("|ab cd", "ｄｗ", "|cd"),
        ("|ab cd", "ｃｗx<Esc>", "|x cd"),
        ("|ab cd", "ｙｗ$p", "ab cdab| "),
        ("a|b cd", "ｄｉｗ", "| cd"),
        ("|(ab) cd", "ｌｄｉ（", "(|) cd"),
        ("|(ab) cd", "ｌｃｉ（x<Esc>", "(|x) cd"),
        ("|ab cd", "ｖｌｌｄ", "|cd"),
        ("|ab cd", "ｖｗｙ$p", "ab cdab |c"),
        ("|a\nb\nc", "Ｖｊｄ", "|c"),
        ("|a\nb\nc", "ｖｊｏｄ", "|\nc"),
        ("|a b", "ｖｉｗｄ", "| b"),
        ("|a\nb\nc", "Ｖ＞", "    |a\nb\nc"),
        ("|a b c", "ｄ２ｗ", "|c"),
    ]);
    // `！` is the filter operator, and `：` opens the command line, in Visual mode too.
    let mut a = typed("|a\nb", "！！");
    assert_eq!((a.vi.mode, status(&screen(&mut a))), (Mode::Cmd, ":1,1!"));
    let mut a = typed("|a\nb", "Ｖｊ：");
    assert_eq!((a.vi.mode, status(&screen(&mut a))), (Mode::Cmd, ":'<,'>"));
}

#[test]
fn vi_10_insert_mode_and_the_command_line_take_full_width_as_it_is() {
    let a = typed("|", "ｉａｂｃ０<Esc>");
    assert_eq!(text(&a), "ａｂｃ０", "typed text is not converted");
    // In Normal mode `ａ` is the command `a`; what is typed after it is text.
    let a = typed("|x", "ａ１　ｙ<Esc>");
    assert_eq!(text(&a), "x１\u{3000}ｙ");
}

#[test]
fn vi_10_on_the_command_line_and_in_a_search_it_is_text() {
    let mut a = app_with(Some("f"), "xｂx b\n".as_bytes(), (60, 6));
    feed(&mut a, &fw(":")); // `：` opens the line, but what is typed on it is taken as typed
    feed(&mut a, "ｑ\n");
    assert_eq!(status(&screen(&mut a)), "bad range: ｑ");
    feed(&mut a, "／ｂ\n");
    assert_eq!(a.buf.cursor, 1, "the pattern ｂ finds the full-width one");
    feed(&mut a, "0／b\n");
    assert_eq!(a.buf.cursor, 4, "and b the half-width one");
    feed(&mut a, ":s/ｂ/B/\n");
    assert_eq!(text(&a), "xBx b");
}

#[test]
fn vi_10_only_the_range_of_full_width_ascii_is_read_as_half_width() {
    // Just outside U+FF01–FF5E, and other full-width chars: no command (the text stays).
    for c in ['\u{ff00}', '\u{ff5f}', '\u{3000}', 'あ', '。', '\u{ff61}'] {
        let mut a = typed("|abc\ndef", "");
        let before = marked(&a);
        a.update(Event::Key(Key::Char(c)));
        assert_eq!(marked(&a), before, "{c:?}");
        assert_eq!(a.vi.mode, Mode::Normal);
    }
    // The ends of the range are read: `！` is `!`, `～` is `~` (not a command).
    let mut a = typed("|a", "");
    a.update(Event::Key(Key::Char('\u{ff01}')));
    a.update(Event::Key(Key::Char('\u{ff01}')));
    assert_eq!(a.vi.mode, Mode::Cmd);
    let mut a = typed("|abc", "");
    a.update(Event::Key(Key::Char('\u{ff5e}')));
    assert_eq!(marked(&a), "|abc");
}

#[test]
fn vi_10_the_keys_that_walk_the_changes_may_be_full_width_too() {
    let mut a = app_with(Some("f"), b"a1\nb\na2\nc\na3\n", (80, 14));
    feed(&mut a, "rAjjrAjjrAgg");
    disk(&mut a, "X1\nb\nX2\nc\nX3\n");
    let at = |a: &App| a.buf.line_of(a.buf.cursor);
    feed(&mut a, "］ｃ");
    assert_eq!(at(&a), 2);
    feed(&mut a, "］ｃ");
    assert_eq!(at(&a), 4);
    feed(&mut a, "［ｃ");
    assert_eq!(
        a.msg.as_deref(),
        Some("no previous change"),
        "the marks the cursor has seen are gone"
    );
    assert_eq!(a.vi.mode, Mode::Normal);
}

// ---------------------------------------------------------------------------------------------
// VI-11
// ---------------------------------------------------------------------------------------------

fn hooked(text: &str) -> App {
    let mut a = app_with(Some("f"), text.as_bytes(), (60, 8));
    a.hook = Some("fcitx5-remote -c".into());
    a
}

fn hooks(fx: &[Effect]) -> usize {
    fx.iter().filter(|e| matches!(e, Effect::Hook(c) if c == "fcitx5-remote -c")).count()
}

fn hooks_for(a: &mut App, keys: &str) -> usize {
    hooks(&feed(a, keys))
}

#[test]
fn vi_11_it_runs_once_at_startup() {
    let fx = hooked("a\n").start("");
    assert_eq!(hooks(&fx), 1);
    assert!(matches!(fx[0], Effect::Render(_)), "after the first frame");
    let fx = app_with(Some("f"), b"a\n", (60, 8)).start("");
    assert!(!fx.iter().any(|e| matches!(e, Effect::Hook(_))), "without SI_ON_NORMAL nothing runs");
}

#[test]
fn vi_11_it_runs_every_time_normal_mode_is_entered() {
    let mut a = hooked("a b\nc d\ne f\n");
    a.start("");
    for (keys, n) in [
        ("i<Esc>", 1),
        ("ihello world<Esc>", 1),
        ("A!<C-c>", 1),
        ("v<Esc>", 1),
        ("Vj<Esc>", 1),
        ("vld", 1),
        ("Vy", 1),
        (":w<CR>", 1),
        (":<Esc>", 1),
        ("/zz<CR>", 1),
        ("/zz<Esc>", 1),
        ("ox<Esc>", 1),
        ("cwx<Esc>", 1),
        ("ixy<Esc>ixy<Esc>", 2),
        (":s/x/y/<CR>:s/y/x/<CR>", 2),
    ] {
        assert_eq!(hooks_for(&mut a, keys), n, "{keys}");
    }
}

#[test]
fn vi_11_it_does_not_run_for_keys_that_do_not_enter_normal_mode() {
    let mut a = hooked("a b\nc d\ne f\n");
    a.start("");
    for keys in [
        "jjkkwb",
        "x",
        "dd",
        "u",
        "<Esc>",
        "<Esc><Esc>",
        "<C-c>",
        "5l",
        "d<Esc>",
        "2<Esc>",
        "yyp",
        "zz",
    ] {
        assert_eq!(hooks_for(&mut a, keys), 0, "{keys}");
    }
    // Leaving Normal mode is not entering it.
    for keys in ["i", "a", "v", "V", ":", "/", "o"] {
        assert_eq!(hooks_for(&mut a, keys), 0, "{keys}");
        hooks_for(&mut a, "<Esc>");
    }
}

#[test]
fn vi_11_it_does_not_run_on_the_way_out() {
    let mut a = hooked("a\n");
    a.start("");
    let fx = feed(&mut a, ":q<CR>");
    assert!(fx.iter().any(|e| matches!(e, Effect::Quit(0))));
    assert_eq!(hooks(&fx), 0);
}

#[test]
fn vi_11_a_command_given_on_the_command_line_of_si_enters_normal_mode_too() {
    // `si +3 f` types `:3<CR>`: startup, then the end of the command line.
    let fx = hooked("a\nb\nc\n").start(":3\n");
    assert_eq!(hooks(&fx), 2);
}
