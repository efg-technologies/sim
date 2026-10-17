//! UI-6: indent guides, as indent-blankline draws them: the width a file indents by (pure), the
//! guides of each line and of the blank lines among them, the brighter guide of the scope around
//! the cursor, and that nothing of the text changes.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use si::core::app::Event;
use si::core::color::{Rgb, palette};
use sim::color::{Syntax, Tones};
use sim::indent::{Indent, detect, guides, lead};
use sim::layout::Skin;
use sim::paint::Paint;
use sim::sim::SimEvent;

const BG: Rgb = (0x1e, 0x1e, 0x1e);

#[test]
fn ui_6_the_leading_blanks_of_a_line_cover_columns_and_a_tab_is_to_the_next_multiple_of_four() {
    assert_eq!((lead("x").cols, lead("x").blank), (0, false));
    assert_eq!((lead("    x").cols, lead("    x").tab), (4, false));
    assert_eq!((lead("\tx").cols, lead("\tx").tab), (4, true));
    assert_eq!(lead("  \tx").cols, 4, "a tab goes to the next multiple of 4");
    assert_eq!(lead("      \tx").cols, 8);
    assert!(lead("").blank && lead("   ").blank && lead("\t\t").blank);
    assert!(!lead(" a ").blank);
}

#[test]
fn ui_6_a_file_is_indented_by_tabs_two_or_four_as_its_first_lines_show() {
    let four = ["fn a() {", "    if x {", "        y();", "    }", "}"];
    assert_eq!(detect(four.into_iter()), Indent { tabs: false, width: 4 });
    let two = ["a:", "  b:", "    c: 1", "  d:", "    e: 2", "      f: 3"];
    assert_eq!(detect(two.into_iter()), Indent { tabs: false, width: 2 });
    let tabs = ["fn a() {", "\tif x {", "\t\ty();", "\t}", "    // one stray", "}"];
    assert_eq!(detect(tabs.into_iter()), Indent { tabs: true, width: 4 });
    assert_eq!(detect([].into_iter()), Indent::default(), "an empty file: 4 by spaces");
    assert_eq!(
        detect(["x", "y"].into_iter()),
        Indent { tabs: false, width: 4 },
        "nothing is indented"
    );
    // Lines indented by 2 more than a multiple of 4 are a fifth of the indented lines, at least.
    let mixed = ["    a", "    b", "    c", "    d", "      e"];
    assert_eq!(detect(mixed.into_iter()).width, 2);
    let rare = ["    a", "    b", "    c", "    d", "    e", "      f"];
    assert_eq!(detect(rare.into_iter()).width, 4);
    // Only the first 1000 lines are looked at.
    let long: Vec<String> =
        (0..2000).map(|i| if i < 1000 { "    x".into() } else { "  y".into() }).collect();
    assert_eq!(detect(long.iter().map(String::as_str)).width, 4);
}

#[test]
fn ui_6_the_guides_of_a_line_are_at_every_level_its_blanks_cover() {
    let g = |c, w, f| guides(c, w, f, 200);
    assert_eq!(g(0, 4, 0), Vec::<u16>::new());
    assert_eq!(g(4, 4, 0), [0]);
    assert_eq!(g(8, 4, 0), [0, 4]);
    assert_eq!(g(6, 4, 0), [0, 4], "a level the blanks start");
    assert_eq!(g(6, 2, 0), [0, 2, 4]);
    assert_eq!(g(8, 4, 1), [4], "a blank line has none at 0");
    assert_eq!(g(4, 4, 1), Vec::<u16>::new());
}

// ---- on the screen ----

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

const CODE: &str = "fn a() {\n    if x {\n        y();\n\n        z();\n    }\n\n}\n";

/// The text of the row of the screen that shows `what`, from the text's first column.
fn row(r: &Rig, what: &str) -> String {
    let g = r.sim.geo();
    let y = (usize::from(g.y0)..r.shot.frame.rows.len())
        .find(|&y| r.shot.frame.rows[y].text.contains(what));
    assert!(y.is_some(), "no row with {what:?}:\n{}", dump(&r.shot.frame));
    let y = y.unwrap();
    let digits = 5; // the gutter: the symbol, three digits, a blank
    let text: String =
        r.shot.frame.rows[y].text.chars().skip(usize::from(g.tx()) + digits).collect();
    // (in depth the box's border follows the blanks that fill the row)
    text.trim_end_matches(['┃', '│', ' ', '↲', '⎺', '⎻', '⎼', '⎽']).to_string()
}

fn y_of(r: &Rig, what: &str) -> usize {
    (usize::from(r.sim.geo().y0)..r.shot.frame.rows.len())
        .find(|&y| r.shot.frame.rows[y].text.contains(what))
        .unwrap()
}

fn paint(r: &Rig, y: usize, x: u16) -> Paint {
    let on = r.shot.paints[y].iter().filter(|(c, _)| c.contains(&x)).map(|p| p.1);
    on.fold(Paint::default(), |a, p| Paint {
        fg: a.fg.or(p.fg),
        bg: a.bg.or(p.bg),
        fade: if a.fade > 0 { a.fade } else { p.fade },
        bold: a.bold || p.bold,
    })
}

#[test]
fn ui_6_every_level_of_a_lines_indent_has_a_guide_and_the_text_is_untouched() {
    for skin in [Skin::Flat, Skin::Depth] {
        let r = body("ui6-levels", skin, CODE, (80, 24));
        assert_eq!(row(&r, "fn a()"), "fn a() {", "{skin:?}");
        assert_eq!(row(&r, "if x"), "│   if x {", "{skin:?}");
        assert_eq!(row(&r, "y();"), "│   │   y();", "{skin:?}");
        assert_eq!(row(&r, "z();"), "│   │   z();", "{skin:?}");
        assert_eq!(row(&r, "│   }"), "│   }", "{skin:?}");
        // The buffer is what the file is: a guide is drawn over a blank, never typed.
        assert_eq!(r.text(), CODE.trim_end_matches('\n'));
        assert_eq!(r.app().buf.cursor, 0);
    }
}

#[test]
fn ui_6_a_blank_line_has_the_guides_of_the_fewer_of_the_lines_around_it_from_the_first_level() {
    let r = body("ui6-blank", Skin::Flat, CODE, (80, 24));
    let g = r.sim.geo();
    let guides_at = |y: usize| -> String {
        r.shot.frame.rows[y].text.chars().skip(usize::from(g.tx()) + 5).collect()
    };
    // Lines 3, 5 around the blank line 4: indent 8 and 8, so the guide at column 4 shows.
    let blank = y_of(&r, "y();") + 1;
    assert_eq!(guides_at(blank).trim_end(), "↲   │", "{:?}", guides_at(blank));
    // Lines 6 (indent 4) and 8 (none) around the blank line 7: fewer is 0: none.
    let blank2 = y_of(&r, "│   }") + 1;
    assert_eq!(guides_at(blank2).trim_end(), "↲");
}

#[test]
fn ui_6_the_guide_of_the_scope_around_the_cursor_is_brighter_than_the_others() {
    let mut r = body("ui6-scope", Skin::Flat, CODE, (80, 24));
    let (t, g) = (Tones::new(&palette(BG)), r.sim.geo());
    let col = |level: u16| g.tx() + 5 + level * 4;
    r.keys("jj"); // on `y();`, indented by 8: the innermost guide is at column 4
    let (y, z) = (y_of(&r, "y();"), y_of(&r, "z();"));
    assert_eq!(paint(&r, y, col(1)).fg, Some(t.focus), "the scope's guide on the cursor's line");
    assert_eq!(paint(&r, y, col(0)).fg, Some(t.line), "the outer guide is dim");
    assert_eq!(paint(&r, z, col(1)).fg, Some(t.focus), "and on the lines of the scope");
    assert_eq!(paint(&r, y + 1, col(1)).fg, Some(t.focus), "the blank line inside it too");
    let outer = y_of(&r, "if x");
    assert_eq!(paint(&r, outer, col(0)).fg, Some(t.line), "outside the scope nothing is bright");
    // Up a line, on `if x {` (indent 4): its own guide at column 0 is the scope, and the lines
    // inside the block (all indented by 4 or more) are in it.
    r.keys("k");
    let (y, deep) = (y_of(&r, "if x"), y_of(&r, "y();"));
    assert_eq!(paint(&r, y, col(0)).fg, Some(t.focus));
    assert_eq!(paint(&r, deep, col(0)).fg, Some(t.focus), "the same guide, down the block");
    assert_eq!(paint(&r, deep, col(1)).fg, Some(t.line));
    // A line with no indent has no scope.
    r.keys("gg");
    for lvl in 0..2 {
        assert_ne!(paint(&r, y_of(&r, "y();"), col(lvl)).fg, Some(t.focus), "level {lvl}");
    }
}

#[test]
fn ui_6_a_file_that_indents_by_two_and_one_that_indents_by_tabs() {
    let two = "a:\n  b:\n    c: 1\n  d:\n    e: 2\n      f: 3\n";
    let r = body("ui6-two", Skin::Flat, two, (80, 24));
    assert_eq!(row(&r, "f: 3"), "│ │ │ f: 3");
    assert_eq!(row(&r, "c: 1"), "│ │ c: 1");
    let tabs = "fn a() {\n\tif x {\n\t\ty();\n\t}\n}\n";
    let r = body("ui6-tabs", Skin::Flat, tabs, (80, 24));
    assert!(
        !(0..r.shot.frame.rows.len()).any(|y| y >= usize::from(r.sim.geo().y0)
            && r.shot.frame.rows[y].text.contains('│')
            && !r.shot.frame.rows[y].text.starts_with('│')
            && r.shot.frame.rows[y]
                .text
                .chars()
                .skip(usize::from(r.sim.geo().tx()) + 5)
                .take(8)
                .any(|c| c == '│')),
        "a tab is the level: no guide"
    );
}

#[test]
fn ui_6_a_guide_is_no_char_for_the_mouse_and_calm_and_review_draw_none() {
    let mut r = body("ui6-click", Skin::Flat, CODE, (80, 24));
    let g = r.sim.geo();
    let y = y_of(&r, "y();") as u16;
    r.click(g.tx() + 5 + 4, y); // on the guide at column 4 of `        y();`
    assert_eq!(
        r.app().buf.cursor,
        "fn a() {\n    if x {\n".len() + 4,
        "the char under the guide is the blank"
    );
    // Calm and the preview have no guides.
    let calm = rig_env("ui6-calm", &[&format!("a.rs={CODE}")], &["a.rs"], (80, 24), |_| {});
    assert!(!calm.screen.rows.iter().any(|r| r.text.contains('│')));
}

#[test]
fn ui_6_a_wrapped_line_has_its_guides_on_its_first_row_only() {
    let long = format!("    {}\n", "word ".repeat(30));
    let r = body("ui6-wrap", Skin::Flat, &long, (50, 24));
    let rows: Vec<usize> = (0..r.shot.frame.rows.len())
        .filter(|&y| r.shot.frame.rows[y].text.contains("word"))
        .collect();
    assert!(rows.len() >= 3);
    let g = r.sim.geo();
    let has = |y: usize| {
        r.shot.frame.rows[y].text.chars().skip(usize::from(g.tx()) + 5).take(4).any(|c| c == '│')
    };
    assert!(has(rows[0]));
    assert!(rows[1..].iter().all(|&y| !has(y)));
}

#[test]
fn ui_6_a_huge_indent_or_a_huge_width_overflows_nothing_and_makes_few_guides() {
    // 70,000 blanks: the columns saturate, they never wrap or panic.
    let line = " ".repeat(70_000) + "x";
    assert_eq!(lead(&line).cols, u16::MAX);
    assert_eq!(lead(&("\t".repeat(30_000) + "x")).cols, u16::MAX);
    // A width near u16::MAX: the product of the level and the width does not overflow.
    for w in [u16::MAX, u16::MAX - 1, 40_000, 32_768, 1] {
        for from in [0, 1] {
            let gs = guides(u16::MAX, w, from, 200);
            assert!(gs.len() <= 200, "w {w}: {} guides", gs.len());
            assert!(gs.iter().all(|&c| c < 200), "w {w}");
        }
    }
    assert_eq!(
        guides(u16::MAX, u16::MAX, 0, u16::MAX),
        [0],
        "one level and the next is out of the line"
    );
    assert_eq!(guides(u16::MAX, 1, 0, 80).len(), 80, "no more than the pane has columns");
    assert_eq!(guides(u16::MAX, 0, 0, 10).len(), 10, "a width of 0 is read as 1");
    // The same through sim: a line of 70,000 blanks is drawn, with the guides the pane can show.
    let long = format!("{line}\nshort\n");
    for skin in [Skin::Flat, Skin::Depth] {
        let r = body("ui6-huge", skin, &long, (60, 12));
        assert_eq!(r.text(), long.trim_end_matches('\n'));
        let g = r.sim.geo();
        let y = usize::from(g.y0);
        let row = &r.shot.frame.rows[y];
        assert!(row.text.chars().filter(|&c| c == '│').count() <= 60, "{skin:?}");
        assert!(row.text.chars().count() <= 70, "{skin:?}: the row is a pane wide");
    }
}
