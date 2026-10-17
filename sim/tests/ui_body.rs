//! UI-9, UI-5: the body of the editor in flat and depth: wrapping at the pane's width, the line
//! numbers, the cursor's line, the sign column, driven through sim's state.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use sim::layout::Skin;

const LINE: &str = "word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word.";

#[test]
fn ui_9_prose_wraps_at_the_panes_width_in_every_look_and_a_narrow_pane_wraps_at_its_own() {
    for skin in [Skin::Calm, Skin::Flat, Skin::Depth] {
        for name in ["a.md", "a.txt", "README"] {
            let file = format!("{name}={LINE}\n");
            let wide = rig_env("ui9-wide", &[&file], &[name], (200, 12), |e| e.skin = skin);
            let texts = |r: &Rig| -> Vec<String> {
                let rows = r.screen.rows.iter().map(|x| x.text.clone());
                rows.filter(|t| t.contains("word")).collect()
            };
            // The line is 190 columns: one row in a pane of 190 or more, not two at 72.
            let one = texts(&wide);
            assert!(one.len() == 1 && one[0].contains(LINE), "{skin:?} {name}: {one:?}");
            let narrow = rig_env("ui9-narrow", &[&file], &[name], (60, 12), |e| e.skin = skin);
            let rows = texts(&narrow);
            assert!(rows.len() >= 4, "{skin:?} {name}: wraps at 60: {rows:?}");
            assert!(
                rows.iter().all(|r| unicode_width::UnicodeWidthStr::width(r.as_str()) <= 60),
                "{skin:?} {name}"
            );
        }
    }
}

#[test]
fn ui_9_si_keeps_its_72_columns() {
    use sim_core::core::app::App;
    use sim_core::core::view::render;
    let file = format!("{LINE}\n").into_bytes();
    let mut app = App::new(Some("a.md".into()), Ok(Some(file)), (200, 12));
    assert!(!app.wide, "si's default");
    let rows = render(&mut app).rows;
    assert!(rows[0].text.chars().count() <= 5 + 72, "{}", rows[0].text);
    app.wide = true;
    let rows = render(&mut app).rows;
    assert!(rows[0].text.contains(LINE), "wide: {}", rows[0].text);
}

// ---- UI-5: the numbers, the cursor's line and the sign column ----

use sim::color::{Syntax, Tones};
use sim::paint::Paint;
use sim::sim::SimEvent;
use sim_core::core::app::Event;
use sim_core::core::color::{Rgb, palette};

const BG: Rgb = (0x1e, 0x1e, 0x1e);

/// `text` open as `a.txt`... in the interface (`chrome`), flat or depth, with a palette.
fn body(name: &str, skin: Skin, text: &str, size: (u16, u16)) -> Rig {
    let file = format!("a.rs={text}");
    let mut r = rig_env(name, &[&file], &["a.rs"], size, |e| {
        e.skin = skin;
        e.syn = Syntax::new(true);
        e.chrome = true;
    });
    r.send(SimEvent::Term(Event::Bg(BG)));
    r
}

fn lines(n: usize) -> String {
    (1..=n).map(|i| format!("line {i}\n")).collect()
}

/// The merged paint of a cell.
fn paint(r: &Rig, y: usize, x: u16) -> Paint {
    let on = r.shot.paints[y].iter().filter(|(c, _)| c.contains(&x)).map(|p| p.1);
    on.fold(Paint::default(), |a, p| Paint {
        fg: a.fg.or(p.fg),
        bg: a.bg.or(p.bg),
        fade: if a.fade > 0 { a.fade } else { p.fade },
        bold: a.bold || p.bold,
        soft: a.soft || p.soft,
        sealed: a.sealed || p.sealed,
        cue: a.cue.max(p.cue),
        role: a.role,
    })
}

fn row_of(r: &Rig, what: &str) -> usize {
    let y = (usize::from(r.sim.geo().y0)..r.shot.frame.rows.len())
        .find(|&y| r.shot.frame.rows[y].text.contains(what));
    assert!(y.is_some(), "no row with {what:?}:\n{}", dump(&r.shot.frame));
    y.unwrap()
}

#[test]
fn ui_5_the_cursor_s_line_has_its_absolute_number_in_primary_bold_and_the_others_their_distance() {
    for skin in [Skin::Flat, Skin::Depth] {
        let mut r = body("ui5-rel", skin, &lines(12), (80, 24));
        r.keys("4j");
        let (g, t) = (r.sim.geo(), Tones::new(&palette(BG)));
        let tx = usize::from(g.tx());
        let at =
            |y: usize| -> String { r.shot.frame.rows[y].text.chars().skip(tx).take(5).collect() };
        // `[change symbol][number, 3 wide][blank]`: the cursor's is left-aligned, the others right.
        let (cur, above, below, far) = (
            row_of(&r, "line 5"),
            row_of(&r, "line 4"),
            row_of(&r, "line 6"),
            row_of(&r, "line 1"),
        );
        assert_eq!(at(cur), " 5    ".chars().take(5).collect::<String>(), "{skin:?}");
        assert_eq!(at(above), "   1 ", "{skin:?}");
        assert_eq!(at(below), "   1 ", "{skin:?}");
        assert_eq!(at(far), "   4 ", "{skin:?}");
        let x = g.tx() + 1; // the number's first column
        let p = paint(&r, cur, x);
        assert_eq!((p.fg, p.bold), (Some(t.primary), true), "{skin:?}: primary and bold, no hue");
        assert!(
            r.shot.frame.rows[above]
                .spans
                .iter()
                .any(|(c, s)| c.contains(&x) && *s == sim_core::core::view::Style::Secondary),
            "{skin:?}: the others are secondary"
        );
        assert_eq!(paint(&r, above, x).fg, None, "{skin:?}: no accent on the others");
        // The file's own text is untouched.
        assert!(r.text().starts_with("line 1\nline 2"));
    }
}

#[test]
fn ui_5_calm_keeps_si_s_absolute_numbers_and_no_sign_column() {
    let mut r = rig_env("ui5-calm", &[&format!("a.rs={}", lines(12))], &["a.rs"], (80, 24), |_| {});
    r.keys("4j");
    assert_eq!(r.screen.rows[3].text, "   4 line 4", "si's own gutter, the absolute number");
    assert_eq!(r.sim.geo().sign, 0);
}

#[test]
fn ui_5_only_the_number_is_lit_in_normal_mode_and_the_whole_line_in_insert_mode() {
    for skin in [Skin::Flat, Skin::Depth] {
        let mut r = body("ui5-insert", skin, &lines(8), (60, 24));
        let (t, g) = (Tones::new(&palette(BG)), r.sim.geo());
        r.keys("2j");
        let cur = row_of(&r, "line 3");
        for x in [g.ex, g.tx() + 6, g.ex + g.sign + g.ew - 1] {
            assert_eq!(paint(&r, cur, x).bg, None, "{skin:?}: normal mode: no surface (col {x})");
        }
        r.keys("i");
        let cur = row_of(&r, "line 3");
        for x in [g.ex, g.tx(), g.tx() + 6, g.ex + g.sign + g.ew - 1] {
            assert_eq!(
                paint(&r, cur, x).bg,
                Some(t.cursor),
                "{skin:?}: insert mode: the surface, sign cell and right edge included (col {x})"
            );
        }
        for other in ["line 2", "line 4"] {
            assert_eq!(paint(&r, row_of(&r, other), g.tx() + 6).bg, None, "{skin:?}: {other}");
        }
        r.keys("<Esc>");
        assert_eq!(
            paint(&r, row_of(&r, "line 3"), g.tx() + 6).bg,
            None,
            "{skin:?}: and gone again"
        );
    }
}

#[test]
fn ui_5_every_row_of_a_wrapped_line_has_the_surface_in_insert_mode_and_a_change_tint_wins() {
    let long = "word ".repeat(30);
    let mut r = body("ui5-wrap", Skin::Flat, &format!("a\n{long}\nb\n"), (60, 24));
    r.keys("ji");
    let (t, g) = (Tones::new(&palette(BG)), r.sim.geo());
    let rows: Vec<usize> = (0..r.shot.frame.rows.len())
        .filter(|&y| r.shot.frame.rows[y].text.contains("word"))
        .collect();
    assert!(rows.len() >= 3, "wrapped: {rows:?}");
    for y in rows {
        assert_eq!(paint(&r, y, g.tx() + 8).bg, Some(t.cursor), "row {y}");
    }
}

#[test]
fn ui_5_the_sign_column_is_two_cells_always_reserved_and_the_text_starts_after_it() {
    for skin in [Skin::Flat, Skin::Depth] {
        let mut r = body("ui5-sign", skin, &lines(5), (80, 24));
        let g = r.sim.geo();
        assert_eq!(g.sign, 1, "{skin:?}: one cell of ours and the App's change symbol");
        let y = row_of(&r, "line 1");
        let chars: Vec<char> = r.shot.frame.rows[y].text.chars().collect();
        assert_eq!(
            &chars[usize::from(g.ex)..usize::from(g.ex) + 2],
            &[' ', ' '],
            "{skin:?}: both cells blank"
        );
        // The terminal's cursor and a click count from where the text starts.
        let first = r.shot.frame.cursor;
        assert_eq!(first.0, g.tx() + 5, "{skin:?}: the cursor is on the first char");
        r.click(g.tx() + 5 + 5, y as u16);
        assert_eq!(r.app().buf.cursor, 5, "{skin:?}: a click on the sixth char");
        let before = r.app().buf.cursor;
        r.click(g.ex, y as u16 + 1); // the sign cell: nothing moves
        assert_eq!(r.app().buf.cursor, before, "{skin:?}: a click on a sign does nothing");
        // A pane too narrow for it keeps no sign column.
        let narrow = body("ui5-narrow", skin, &lines(5), (30, 24));
        assert_eq!(narrow.sim.geo().sign, u16::from(narrow.sim.geo().ew + 1 >= 25), "{skin:?}");
    }
}

#[test]
fn ui_5_the_view_keeps_eight_lines_around_the_cursor_as_neovim_s_scrolloff_does() {
    let mut r = body("ui5-scrolloff", Skin::Flat, &lines(100), (80, 30));
    let g = r.sim.geo();
    r.keys("40j");
    let y = usize::from(r.shot.frame.cursor.1) - usize::from(g.y0);
    let rows = usize::from(g.rows);
    assert!(y >= 8 && y + 8 < rows, "the cursor is on row {y} of {rows}");
    r.keys("5k");
    let y = usize::from(r.shot.frame.cursor.1) - usize::from(g.y0);
    assert!(y >= 8 && y + 8 < rows, "{y} of {rows}");
}

#[test]
fn ui_5_the_other_numbers_fade_with_their_distance_in_depth_as_the_text_does_and_flat_is_uniform() {
    let mut r = body("ui5-fade", Skin::Depth, &lines(70), (80, 30));
    let g = r.sim.geo();
    // (a row that ends with `what`: `line 3` is not `line 30`; the scrollbar is after the text)
    let at = |r: &Rig, what: &str| {
        let ends = |s: &str| {
            s.trim_end_matches(|c: char| c == ' ' || "┃│⎺⎻⎼⎽".contains(c)).ends_with(what)
        };
        (usize::from(g.y0)..).find(|&y| ends(&r.shot.frame.rows[y].text)).unwrap()
    };
    let (near, far) = (at(&r, "line 3"), at(&r, "line 22"));
    let (num, text) = (g.tx() + 3, g.tx() + 8);
    assert_eq!(paint(&r, near, num).fade, 0, "next to the cursor nothing fades");
    assert!(paint(&r, far, num).fade > 0, "a number far from the cursor fades");
    assert_eq!(
        paint(&r, far, num).fade,
        paint(&r, far, text).fade,
        "by the same curve as the text of its row"
    );
    // ... down to its own floor: |Lc| 40 for a number, 60 for the text (LOOK-6)
    assert!(paint(&r, far, num).soft && !paint(&r, far, text).soft);
    r.keys("20j");
    let (near, far) = (at(&r, "line 21"), usize::from(g.y0)); // (the first row shown)
    assert_eq!(paint(&r, near, num).fade, 0);
    assert!(paint(&r, far, num).fade > 0, "from wherever the cursor is");
    // Flat is uniform.
    let flat = body("ui5-nofade", Skin::Flat, &lines(70), (80, 30));
    assert!((0..flat.shot.paints.len()).all(|y| paint(&flat, y, num).fade == 0));
}

// ---- UI-6: the colorcolumn ----

#[test]
fn ui_6_a_pale_band_is_at_the_hundredth_column_of_the_text_where_the_pane_has_it() {
    let long = format!("{}\n", "x".repeat(120));
    for skin in [Skin::Flat, Skin::Depth] {
        let mut r = body("ui6-cc", skin, &format!("short\n{long}last\n"), (140, 24));
        let (t, g) = (Tones::new(&palette(BG)), r.sim.geo());
        let col = g.tx() + 5 + 99; // the gutter, then column 100 of the text
        for y in usize::from(g.y0)..usize::from(g.y0 + g.rows) {
            assert_eq!(
                paint(&r, y, col).bg,
                Some(t.cc),
                "{skin:?}: row {y}, even where the file has ended"
            );
            assert_eq!(paint(&r, y, col - 1).bg, None, "{skin:?}: one cell, not a bar");
            assert_eq!(paint(&r, y, col + 1).bg, None, "{skin:?}");
        }
        assert_ne!(t.cc, BG, "a step from the page");
        assert!(t.cc.0 > BG.0, "lighter on a dark page: the page times 1.18");
        // The text is the file's: the band is a background, nothing typed.
        assert_eq!(r.text().lines().nth(1).unwrap().len(), 120);
        // The cursor's line in Insert mode lies over it, as the other surfaces do.
        r.keys("ji");
        let y = (usize::from(g.y0)..)
            .find(|&y| r.shot.frame.rows[y].text.contains(&"x".repeat(20)))
            .unwrap();
        assert_eq!(paint(&r, y, col).bg, Some(t.cursor), "{skin:?}: the cursor's line wins");
    }
    // A narrower pane has no column 100; calm has no band.
    let narrow = body("ui6-cc-narrow", Skin::Flat, &long, (100, 24));
    let g = narrow.sim.geo();
    assert!((0..narrow.shot.paints.len()).all(|y| paint(&narrow, y, g.tx() + 5 + 99).bg.is_none()));
    let calm = rig_env("ui6-cc-calm", &[&format!("a.rs={long}")], &["a.rs"], (140, 24), |_| {});
    assert!(calm.shot.paints.iter().all(Vec::is_empty));
}
