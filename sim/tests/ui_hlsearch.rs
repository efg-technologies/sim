//! UI-14: the matches of the last `/` search are lit in the text, as `hlsearch = true` does: the
//! matches on screen on the accent's block, the current one on its solid color; `:noh` and Esc in
//! Normal mode clear them.
#![allow(clippy::unwrap_used, clippy::panic, clippy::single_range_in_vec_init)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use si::core::app::Event;
use si::core::color::{Rgb, palette};
use sim::color::{Syntax, Tones};
use sim::layout::Skin;
use sim::search::Matcher;
use sim::sim::SimEvent;

const BG: Rgb = (0x1e, 0x1e, 0x1e);

fn rig_hl(name: &str, skin: Skin) -> Rig {
    let mut r = rig_env(
        name,
        &["a.txt=foo bar foo\nbaz Foo\nqux foo\n日本語 foo\n", "b.txt=foo here\nsecond\n"],
        &["a.txt"],
        (100, 20),
        |e| {
            e.skin = skin;
            e.syn = Syntax::new(true);
            e.chrome = true;
        },
    );
    r.send(SimEvent::Term(Event::Bg(BG)));
    r
}

/// The merged background of a cell.
fn bg(r: &Rig, x: u16, y: usize) -> Option<Rgb> {
    r.shot.paints[y].iter().find(|(c, p)| c.contains(&x) && p.bg.is_some()).and_then(|x| x.1.bg)
}

/// Where the `n`-th occurrence of `s` (counting the rows of the text) starts: its column and row.
fn find(r: &Rig, s: &str, n: usize) -> (u16, usize) {
    let g = r.sim.geo();
    let mut seen = 0;
    for (y, row) in r.screen.rows.iter().enumerate().skip(usize::from(g.y0)) {
        let cs: Vec<char> = row.text.chars().collect();
        let mut col = 0u16;
        for (i, c) in cs.iter().enumerate() {
            let tail: String = cs[i..].iter().collect();
            if col >= g.tx() + 4 && tail.starts_with(s) {
                if seen == n {
                    return (col, y);
                }
                seen += 1;
            }
            col += unicode_width::UnicodeWidthChar::width(*c).unwrap_or(0) as u16;
        }
    }
    panic!(
        "no occurrence {n} of {s:?} on\n{}",
        r.screen.rows.iter().map(|r| r.text.clone()).collect::<Vec<_>>().join("\n")
    );
}

fn lit_cells(r: &Rig) -> usize {
    let t = Tones::new(&palette(BG));
    let (soft, _) = t.accent_block();
    let (solid, _) = t.accent_solid();
    let g = r.sim.geo();
    (usize::from(g.y0)..r.screen.rows.len() - 1)
        .flat_map(|y| (g.tx()..g.tx() + g.ew).map(move |x| (x, y)))
        .filter(|&(x, y)| matches!(bg(r, x, y), Some(c) if c == soft || c == solid))
        .count()
}

#[test]
fn ui_14_the_matches_on_screen_are_on_the_accent_s_block_and_the_current_one_on_its_solid_color() {
    let mut r = rig_hl("hl-lit", Skin::Flat);
    assert_eq!(lit_cells(&r), 0, "nothing is lit before a search");
    r.keys("/foo\n");
    let t = Tones::new(&palette(BG));
    let ((soft, _), (solid, _)) = (t.accent_block(), t.accent_solid());
    let (x0, y0) = find(&r, "foo", 0);
    let (x1, y1) = find(&r, "foo", 1);
    let (x2, y2) = find(&r, "Foo", 0);
    for i in 0..3 {
        assert_eq!(
            bg(&r, x0 + i, y0),
            Some(soft),
            "the first foo, smartcase: all lowercase, so any case"
        );
        assert_eq!(bg(&r, x1 + i, y1), Some(solid), "the one the cursor is on is the current one");
        assert_eq!(bg(&r, x2 + i, y2), Some(soft), "Foo too");
    }
    let (xb, yb) = find(&r, "bar", 0);
    assert_eq!(bg(&r, xb, yb), None, "and nothing else");
    assert_eq!(bg(&r, x0 + 3, y0), None, "the blank after a match");
    assert_eq!(
        lit_cells(&r),
        5 * 3,
        "five matches of three cells: the file's four lines and the wide one"
    );
    r.keys("n");
    let (x2, y2) = find(&r, "Foo", 0);
    assert_eq!(bg(&r, x2, y2), Some(solid), "n moves the current one");
    assert_eq!(bg(&r, x1, y1), Some(soft));
}

#[test]
fn ui_14_a_match_after_wide_chars_is_lit_in_its_own_cells() {
    let mut r = rig_hl("hl-wide", Skin::Flat);
    r.keys("/foo\nnnn");
    let t = Tones::new(&palette(BG));
    let (solid, _) = t.accent_solid();
    let (x, y) = find(&r, "foo", 3);
    assert!(r.screen.rows[y].text.contains("日本語 foo"));
    for i in 0..3 {
        assert_eq!(bg(&r, x + i, y), Some(solid), "cell {i}");
    }
    assert_eq!(bg(&r, x - 1, y), None, "the blank before it");
    assert_eq!(bg(&r, x - 2, y), None, "and the wide char");
}

#[test]
fn ui_14_noh_and_esc_in_normal_mode_clear_them_and_a_search_lights_them_again() {
    let mut r = rig_hl("hl-clear", Skin::Flat);
    r.keys("/foo\n");
    assert!(lit_cells(&r) > 0);
    r.keys(":noh\n");
    assert_eq!(lit_cells(&r), 0, ":noh");
    assert_eq!(r.app().vi.mode, si::core::vi::Mode::Normal);
    r.keys("n");
    assert!(lit_cells(&r) > 0, "n lights them again");
    r.keys("<Esc>");
    assert_eq!(lit_cells(&r), 0, "Esc in Normal mode");
    r.keys("*");
    assert!(lit_cells(&r) > 0, "* does");
    r.keys(":nohlsearch\n");
    assert_eq!(lit_cells(&r), 0, ":nohlsearch");
    r.keys("N");
    assert!(lit_cells(&r) > 0, "N does");
    r.keys(":nohl\n");
    assert_eq!(lit_cells(&r), 0, "and the abbreviation");
    r.keys("/\n");
    assert!(lit_cells(&r) > 0, "an empty pattern repeats the last search");
    r.keys("#");
    assert!(lit_cells(&r) > 0);
    // Esc that is not in Normal mode with nothing pending does not clear
    r.keys("i<Esc>");
    assert!(lit_cells(&r) > 0, "Esc that leaves Insert mode");
    r.keys("d<Esc>");
    assert!(lit_cells(&r) > 0, "Esc that cancels a pending operator");
    r.keys("<Esc>");
    assert_eq!(lit_cells(&r), 0);
}

#[test]
fn ui_14_noh_is_not_given_to_the_app_and_other_commands_do_not_light_anything() {
    let mut r = rig_hl("hl-cmd", Skin::Flat);
    r.keys(":noh\n");
    assert!(!r.status().contains("not an editor command"), "{}", r.status());
    assert_eq!(lit_cells(&r), 0);
    r.keys("/foo\n:noh\n");
    r.keys(":set\n");
    assert_eq!(lit_cells(&r), 0, "a command line that is not a search lights nothing");
    assert!(!r.text().is_empty());
}

#[test]
fn ui_14_a_pattern_is_si_s_regex_and_smartcase() {
    let mut r = rig_hl("hl-regex", Skin::Flat);
    r.keys("/fo+\n");
    assert!(lit_cells(&r) >= 5 * 3, "a regex: fo+ is foo, in every case");
    r.keys(":noh\n/Foo\n");
    assert_eq!(lit_cells(&r), 3, "an uppercase letter makes it exact: only `Foo`");
    r.keys(":noh\n/o{2}\n");
    assert_eq!(lit_cells(&r), 5 * 2, "o{{2}}");
    r.keys(":noh\n/\\bfoo\\b\n");
    assert!(lit_cells(&r) > 0, "{}", r.status());
    // a pattern that does not compile lights nothing, and is told by the App as before
    r.keys(":noh\n/(\n");
    assert_eq!(lit_cells(&r), 0);
    assert!(r.status().contains("bad pattern"), "{}", r.status());
    // and an empty match is not lit
    r.keys(":noh\n/k*\n");
    assert_eq!(lit_cells(&r), 0);
}

#[test]
fn ui_14_each_file_has_its_own_highlight_and_calm_has_none() {
    let mut r = rig_hl("hl-files", Skin::Flat);
    r.keys("/foo\n");
    assert!(lit_cells(&r) > 0);
    r.keys(":e b.txt\n");
    assert_eq!(lit_cells(&r), 0, "the other file: nothing was searched in it");
    r.keys("/foo\n");
    assert!(lit_cells(&r) > 0, "a search there lights its own");
    r.keys("<Tab>");
    assert!(lit_cells(&r) > 0, "the first file still has its");
    let mut r = rig_hl("hl-calm", Skin::Calm);
    r.keys("/foo\n");
    assert!(r.shot.paints.iter().all(Vec::is_empty), "calm draws none");
}

#[test]
fn ui_14_the_scrollbar_marks_of_a_search_go_with_the_highlight() {
    let text: String = (0..200)
        .map(|i| if i % 50 == 7 { "needle\n".to_string() } else { format!("l{i}\n") })
        .collect();
    let mut r = rig_env("hl-marks", &[&format!("n.txt={text}")], &["n.txt"], (100, 24), |e| {
        (e.skin, e.syn, e.chrome) = (Skin::Flat, Syntax::new(true), true);
    });
    r.send(SimEvent::Term(Event::Bg(BG)));
    let t = Tones::new(&palette(BG));
    let g = r.sim.geo();
    let marks = |r: &Rig| {
        let x = g.tx() + g.ew;
        (usize::from(g.y0)..r.screen.rows.len() - 1)
            .filter(|&y| {
                r.shot.paints[y].iter().any(|(c, p)| c.contains(&x) && p.fg == Some(t.accent_fg()))
            })
            .count()
    };
    assert_eq!(marks(&r), 0);
    r.keys("/needle\n");
    assert!(marks(&r) >= 3, "the matches of the file are marks: {}", marks(&r));
    r.keys(":noh\n");
    assert_eq!(marks(&r), 0, "and go with :noh");
}

// ---- the matcher ----

#[test]
fn ui_14_the_matcher_finds_char_ranges_with_si_s_case_rule() {
    let m = Matcher::new("foo", false).unwrap();
    assert_eq!(m.ranges("foo bar Foo FOO"), [0..3, 8..11, 12..15]);
    let exact = Matcher::new("Foo", false).unwrap();
    assert_eq!(exact.ranges("foo Foo"), [4..7], "an uppercase letter makes it exact");
    let asked = Matcher::new("foo", true).unwrap();
    assert_eq!(asked.ranges("foo Foo"), [0..3], "or when it was asked to be");
    assert_eq!(
        Matcher::new("o+", false).unwrap().ranges("日本 foo oo"),
        [4..6, 7..9],
        "char positions, not bytes"
    );
    assert!(Matcher::new("(", false).is_none());
    assert!(Matcher::new("x*", false).unwrap().ranges("abc").is_empty(), "an empty match is none");
    assert!(Matcher::new("foo", false).unwrap().ranges("").is_empty());
    // a line full of matches is bounded
    assert!(Matcher::new("a", false).unwrap().ranges(&"a".repeat(100_000)).len() <= 1000);
}
