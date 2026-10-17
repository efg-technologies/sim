//! UI-13: the feel of the author's Neovim: a yank is lit for 150 ms, and `n`, `N`, `*`, `#`, Ctrl-d
//! and Ctrl-u put the line they land on in the middle of the screen (si does that already: the tests
//! pin it, since sim relies on it).
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use si::core::app::Event;
use si::core::color::{Rgb, palette};
use sim::color::{Syntax, Tones};
use sim::layout::Skin;
use sim::sim::{FLASH_MS, SimEffect, SimEvent};

const BG: Rgb = (0x1e, 0x1e, 0x1e);

fn lines(n: usize, needle: &[usize]) -> String {
    (1..=n)
        .map(|i| if needle.contains(&i) { format!("needle {i}\n") } else { format!("line {i}\n") })
        .collect()
}

fn body(name: &str, skin: Skin, text: &str, chrome: bool) -> Rig {
    let file = format!("a.txt={text}");
    let mut r = rig_env(name, &[&file], &["a.txt"], (80, 24), |e| {
        e.skin = skin;
        e.syn = Syntax::new(true);
        e.chrome = chrome;
    });
    r.send(SimEvent::Term(Event::Bg(BG)));
    r
}

/// The row of the screen the cursor is on, counted from the first row of the text.
fn cursor_row(r: &Rig) -> i32 {
    i32::from(r.shot.frame.cursor.1) - i32::from(r.sim.geo().y0)
}

fn middle(r: &Rig) -> i32 {
    (i32::from(r.sim.geo().rows) - 1) / 2
}

#[test]
fn ui_13_a_match_of_n_big_n_star_and_hash_is_in_the_middle_of_the_screen_without_a_key_of_sim_s() {
    for (skin, chrome) in [(Skin::Calm, false), (Skin::Flat, true), (Skin::Depth, true)] {
        let mut r = body("ui13-n", skin, &lines(200, &[10, 14, 20, 80, 150]), chrome);
        r.keys("/needle 10\n");
        assert_eq!(r.app().buf.line_of(r.app().buf.cursor), 9);
        // A match a few lines on, which would be on the screen anyway, still goes to the middle.
        r.keys("/needle\nn");
        assert_eq!(r.app().buf.line_of(r.app().buf.cursor), 19, "{skin:?}");
        assert!(
            (cursor_row(&r) - middle(&r)).abs() <= 1,
            "{skin:?}: a near match: row {}",
            cursor_row(&r)
        );
        r.keys("nn");
        let l = r.app().buf.line_of(r.app().buf.cursor);
        assert_eq!(l, 149, "{skin:?}: n goes on to the next match");
        assert!(
            (cursor_row(&r) - middle(&r)).abs() <= 1,
            "{skin:?}: n: row {} of {}",
            cursor_row(&r),
            middle(&r)
        );
        r.keys("N");
        assert_eq!(r.app().buf.line_of(r.app().buf.cursor), 79);
        assert!((cursor_row(&r) - middle(&r)).abs() <= 1, "{skin:?}: N");
        r.keys("gg");
        r.keys("*"); // the word under the cursor: `line`, the next line that has it
        assert!(
            (cursor_row(&r) - middle(&r)).abs() <= 1
                || r.app().buf.line_of(r.app().buf.cursor) < 12,
            "{skin:?}: *"
        );
        r.keys("G#");
        assert!((cursor_row(&r) - middle(&r)).abs() <= 1, "{skin:?}: #: row {}", cursor_row(&r));
    }
}

#[test]
fn ui_13_a_half_page_down_or_up_ends_with_the_cursor_in_the_middle() {
    for (skin, chrome) in [(Skin::Calm, false), (Skin::Flat, true)] {
        let mut r = body("ui13-ctrld", skin, &lines(300, &[]), chrome);
        r.keys("<C-d>");
        assert!(
            (cursor_row(&r) - middle(&r)).abs() <= 1,
            "{skin:?}: <C-d>: row {} of {}",
            cursor_row(&r),
            middle(&r)
        );
        let l = r.app().buf.line_of(r.app().buf.cursor);
        assert!(l > 5, "{skin:?}: it went down: line {l}");
        r.keys("100G<C-u>");
        assert!(
            (cursor_row(&r) - middle(&r)).abs() <= 1,
            "{skin:?}: <C-u>: row {} of {}",
            cursor_row(&r),
            middle(&r)
        );
        assert!(r.app().buf.line_of(r.app().buf.cursor) < 99, "{skin:?}: it went up");
    }
}

#[test]
fn ui_13_n_in_insert_mode_and_after_an_operator_keep_their_meaning_and_the_text_is_not_touched() {
    let mut r = body("ui13-keep", Skin::Flat, &lines(200, &[40, 120]), true);
    // In Insert mode `n` is a letter; after an operator it is a motion and the operator's: no zz.
    r.keys("50Gin<Esc>");
    assert!(r.text().contains("nline 50"), "typed: {}", &r.text()[..30]);
    r.keys("/needle 120\n");
    r.keys("gg");
    r.keys("dn");
    assert!(!r.text().starts_with("line 1\n"), "the operator took the text up to the match");
    let top = r.app().top;
    r.keys("u");
    assert_eq!(r.app().top, top.min(r.app().top), "(undo is undo)");
    // A count before n is the App's, and the search itself is unchanged.
    let mut r = body("ui13-count", Skin::Flat, &lines(200, &[10, 40, 120]), true);
    r.keys("/needle\n");
    let first = r.app().buf.line_of(r.app().buf.cursor);
    assert_eq!(first, 9);
    r.keys("n");
    assert_eq!(r.app().buf.line_of(r.app().buf.cursor), 39);
    assert_eq!(r.text(), lines(200, &[10, 40, 120]).trim_end_matches('\n'));
}

// ---- the flash ----

fn flashes(r: &Rig) -> Vec<u64> {
    r.log.iter().filter_map(|e| if let SimEffect::Flash(n) = e { Some(*n) } else { None }).collect()
}

/// The merged background of a cell.
fn bg(r: &Rig, y: usize, x: u16) -> Option<Rgb> {
    r.shot.paints[y].iter().find(|(c, p)| c.contains(&x) && p.bg.is_some()).and_then(|p| p.1.bg)
}

fn y_of(r: &Rig, what: &str) -> usize {
    (usize::from(r.sim.geo().y0)..).find(|&y| r.shot.frame.rows[y].text.contains(what)).unwrap()
}

#[test]
fn ui_13_a_yank_is_lit_in_the_accent_until_150_ms_have_gone_and_then_it_is_not() {
    assert_eq!(FLASH_MS, 150);
    for skin in [Skin::Flat, Skin::Depth] {
        let mut r = body("ui13-yank", skin, "alpha beta gamma\nsecond line\nthird\n", true);
        let (t, g) = (Tones::new(&palette(BG)), r.sim.geo());
        let (lit, _) = t.accent_block();
        let x0 = g.tx() + 5; // the first char of the text
        assert!(flashes(&r).is_empty());
        r.keys("wyw"); // `beta `
        let n = *flashes(&r).last().expect("asked for the end of the flash");
        let y = y_of(&r, "alpha");
        for k in 6..11 {
            assert_eq!(bg(&r, y, x0 + k), Some(lit), "{skin:?}: char {k} of what was taken");
        }
        assert_eq!(bg(&r, y, x0 + 5), None, "{skin:?}: not the char before it");
        assert_eq!(bg(&r, y, x0 + 11), None, "{skin:?}: nor after it");
        assert_eq!(bg(&r, y, x0), None);
        // The shell says 150 ms are over: it is not lit. A late word of an older flash does nothing.
        r.send(SimEvent::Unflash(n + 100));
        assert_eq!(bg(&r, y, x0 + 6), Some(lit), "{skin:?}: another flash's end is not this one's");
        r.send(SimEvent::Unflash(n));
        assert_eq!(bg(&r, y, x0 + 6), None, "{skin:?}: over");
        // The text and the register are as the yank left them.
        assert_eq!(r.text(), "alpha beta gamma\nsecond line\nthird");
    }
}

#[test]
fn ui_13_whole_lines_are_lit_and_an_edit_puts_the_flash_out() {
    let mut r = body("ui13-lines", Skin::Flat, "one\ntwo\nthree\nfour\n", true);
    let (t, g) = (Tones::new(&palette(BG)), r.sim.geo());
    let (lit, _) = t.accent_block();
    let x0 = g.tx() + 5;
    r.keys("2yy");
    for (what, on) in [("one", true), ("two", true), ("three", false)] {
        assert_eq!(bg(&r, y_of(&r, what), x0) == Some(lit), on, "{what}");
    }
    // Typing is an edit of the text the flash was of: it is gone at once, not at the 150 ms.
    r.keys("x");
    assert!(!(0..r.shot.paints.len()).any(|y| bg(&r, y, x0) == Some(lit)));
    // Two yanks: the first one's end does not put the second out.
    r.keys("yyjyy");
    let ns = flashes(&r);
    let (a, b) = (ns[ns.len() - 2], ns[ns.len() - 1]);
    assert!(b > a);
    r.send(SimEvent::Unflash(a));
    assert!(
        (0..r.shot.paints.len()).any(|y| bg(&r, y, x0) == Some(lit)),
        "the second is still lit"
    );
    r.send(SimEvent::Unflash(b));
    assert!(!(0..r.shot.paints.len()).any(|y| bg(&r, y, x0) == Some(lit)));
}

#[test]
fn ui_13_a_copied_path_and_calm_are_not_lit() {
    // `:copy`-like path copying puts a path on the clipboard, not text of the file's.
    let mut r = body("ui13-path", Skin::Flat, "one\ntwo\n", true);
    r.keys(":copy\n");
    assert!(flashes(&r).is_empty(), "{:?}", r.status());
    // Calm has no accent: nothing is lit (the effect may be asked for, the screen has no paint).
    let mut calm = body("ui13-calm", Skin::Calm, "one two\n", false);
    calm.keys("yw");
    assert!(calm.shot.paints.iter().all(Vec::is_empty));
}
