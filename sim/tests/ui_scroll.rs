//! UI-11: the scrollbar, as satellite draws it: a handle for the lines on screen, a marker for the
//! cursor, and marks for the git signs, the changes on disk and the matches of a search, in flat
//! and depth; the rows (pure) and the bar in sim's state.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use si::core::app::Event;
use si::core::color::{Rgb, palette};
use sim::color::{Chg, Syntax, Tones};
use sim::layout::{ScrollCell, Skin, Tick, scrollbar};
use sim::sim::SimEvent;

const BG: Rgb = (0x1e, 0x1e, 0x1e);

#[test]
fn ui_11_a_row_has_the_strongest_mark_of_its_lines_the_thumb_and_the_cursor_in_quarters() {
    // 100 lines on 10 rows: 10 lines to a row.
    let bar = |marks: Vec<(std::ops::Range<usize>, Tick)>, shown, cursor| {
        scrollbar(10, 100, marks.into_iter(), shown, cursor)
    };
    let cells = bar(vec![], Some((20, 39)), Some(25));
    assert_eq!(
        cells.iter().map(|c| c.thumb).collect::<Vec<_>>(),
        [false, false, true, true, false, false, false, false, false, false]
    );
    assert_eq!(
        cells[2].cursor,
        Some(2),
        "line 25 is half way down row 2 (a quarter of 2: the third)"
    );
    assert!(cells.iter().enumerate().all(|(i, c)| (i == 2) == c.cursor.is_some()));
    // The quarters: the first line of a row is the first quarter, the last of it the fourth.
    assert_eq!(bar(vec![], None, Some(30))[3].cursor, Some(0));
    assert_eq!(bar(vec![], None, Some(39))[3].cursor, Some(3));
    // The strongest mark wins a row: an addition, then a change, then a deletion, then a match.
    let all = vec![
        (10..11, Tick::Hit),
        (10..11, Tick::Loss),
        (10..11, Tick::Change),
        (10..11, Tick::Gain),
        (30..31, Tick::Hit),
        (30..31, Tick::Loss),
        (50..51, Tick::Hit),
        (50..51, Tick::Change),
        (70..71, Tick::Hit),
    ];
    let marks: Vec<Tick> = bar(all, None, None).iter().map(|c| c.mark).collect();
    assert_eq!(
        marks,
        [
            Tick::Plain,
            Tick::Gain,
            Tick::Plain,
            Tick::Loss,
            Tick::Plain,
            Tick::Change,
            Tick::Plain,
            Tick::Hit,
            Tick::Plain,
            Tick::Plain
        ]
    );
    // A range of lines marks every row it touches; nothing breaks on an empty file or no rows.
    let long = bar(vec![(15..45, Tick::Gain)], None, None);
    assert_eq!(long.iter().filter(|c| c.mark == Tick::Gain).count(), 4);
    assert_eq!(
        scrollbar(5, 0, std::iter::empty(), Some((0, 0)), Some(0)),
        vec![ScrollCell::default(); 5]
    );
    assert!(scrollbar(0, 10, std::iter::empty(), None, None).is_empty());
    assert_eq!(scrollbar(4, 2, std::iter::empty(), Some((0, 1)), Some(1)).len(), 4);
}

fn body(
    name: &str,
    skin: Skin,
    text: &str,
    size: (u16, u16),
    tweak: impl FnOnce(&std::path::Path),
) -> Rig {
    let file = format!("a.rs={text}");
    let mut r = rig_env(name, &[&file], &["a.rs"], size, |e| {
        git(&e.cwd, &["init", "-q"]);
        git(&e.cwd, &["add", "-A"]);
        git(&e.cwd, &["commit", "-q", "-m", "init"]);
        tweak(&e.cwd);
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

/// The scrollbar's cells, one for each body row: the character and the foreground and background.
fn bar(r: &Rig) -> Vec<(char, Option<Rgb>, Option<Rgb>)> {
    let g = r.sim.geo();
    let x = g.tx() + g.ew; // the cell after the text: the border in depth, the reserved column in flat
    (usize::from(g.y0)..usize::from(g.y0 + g.rows))
        .map(|y| {
            let ch = r.shot.frame.rows[y].text.chars().nth(usize::from(x)).unwrap();
            let on = r.shot.paints[y].iter().filter(|(c, _)| c.contains(&x)).map(|p| p.1);
            let (fg, bg) = on.fold((None, None), |a, p| (a.0.or(p.fg), a.1.or(p.bg)));
            (ch, fg, bg)
        })
        .collect()
}

#[test]
fn ui_11_the_thumb_is_a_surface_behind_the_lines_on_screen_and_the_cursor_has_a_marker() {
    for skin in [Skin::Flat, Skin::Depth] {
        let r = body("ui11-thumb", skin, &lines(300), (80, 24), |_| {});
        let (t, cells) = (Tones::new(&palette(BG)), bar(&r));
        // The lines on screen are the first ~20 of 300: the first row or two of the bar.
        let thumb: Vec<bool> = cells.iter().map(|c| c.2 == Some(t.thumb)).collect();
        assert!(thumb[0] && !thumb[thumb.len() - 1], "{skin:?}: {thumb:?}");
        // Where the cursor is, one of the four marks of satellite, in the secondary gray.
        assert!(['⎺', '⎻', '⎼', '⎽'].contains(&cells[0].0), "{skin:?}: {:?}", cells[0]);
        assert_eq!(cells[0].1, Some(t.focus), "{skin:?}");
        // The cursor goes down, and the thumb with it.
        let mut r = r;
        r.keys("250G");
        let cells = bar(&r);
        let thumb: Vec<bool> = cells.iter().map(|c| c.2 == Some(t.thumb)).collect();
        let first = thumb.iter().position(|&x| x).unwrap();
        assert!(!thumb[0] && first >= 12, "{skin:?}: the thumb is low in the bar: {thumb:?}");
        assert!(
            cells.iter().filter(|c| ['⎺', '⎻', '⎼', '⎽'].contains(&c.0)).count() == 1,
            "{skin:?}: one marker"
        );
        // The handle is a step from the page, neutral.
        assert_ne!(t.thumb, BG);
    }
}

#[test]
fn ui_11_the_git_signs_and_the_changes_on_disk_are_marks_in_the_colors_of_changes() {
    for skin in [Skin::Flat, Skin::Depth] {
        let mut r = body("ui11-marks", skin, &lines(300), (80, 24), |dir| {
            let mut text = lines(300);
            text = text.replacen("line 150\n", "CHANGED\n", 1); // line 150: changed
            text += "added\n"; // after the last line: added
            std::fs::write(dir.join("a.rs"), text).unwrap();
        });
        let t = Tones::new(&palette(BG));
        r.keys("G"); // the last lines on screen, where the added one is
        let g = r.sim.geo();
        let cells = bar(&r);
        let mark = |c: Chg| cells.iter().filter(|x| x.1 == Some(t.change_fg(c))).count();
        assert!(mark(Chg::Add) >= 1, "{skin:?}: an added line: {cells:?}");
        assert!(mark(Chg::Mod) >= 1, "{skin:?}: a changed line");
        // The marks are on the rows the lines are in: line 150 of 301 is about half way down.
        let rows = usize::from(g.rows);
        let at = cells.iter().position(|x| x.1 == Some(t.change_fg(Chg::Mod))).unwrap();
        assert!((rows / 2).abs_diff(at) <= 1, "{skin:?}: row {at} of {rows}");
    }
}

#[test]
fn ui_11_the_matches_of_a_plain_search_are_marks_in_the_accent_and_a_regex_has_none() {
    let text: String = (1..=300)
        .map(|i| if i % 100 == 0 { "needle\n".into() } else { format!("line {i}\n") })
        .collect();
    for skin in [Skin::Flat, Skin::Depth] {
        let mut r = body("ui11-hits", skin, &text, (80, 24), |_| {});
        let t = Tones::new(&palette(BG));
        let hits =
            |r: &Rig| bar(r).iter().filter(|x| x.0 == '┃' && x.1 == Some(t.accent_fg())).count();
        assert_eq!(hits(&r), 0, "{skin:?}: no search, no marks");
        r.keys("/needle\n");
        assert!(hits(&r) >= 2, "{skin:?}: the matches of the search: {}", hits(&r));
        // Smartcase, as the search is: a capital makes it exact.
        r.keys("/Needle\n");
        assert_eq!(hits(&r), 0, "{skin:?}: exact: no `Needle` in the file");
        // A pattern that is a regex is si's to run, and has no marks.
        r.keys("/nee.le\n");
        assert_eq!(hits(&r), 0, "{skin:?}");
        // The marks follow the text: a match typed or deleted is counted when the text is back in Normal mode.
        r.keys("/needle\n");
        let before = hits(&r);
        r.keys("ggOneedle<Esc>");
        assert!(hits(&r) >= before, "{skin:?}");
    }
}

#[test]
fn ui_11_a_long_file_has_no_marks_for_a_search_and_nothing_is_marked_in_calm_or_without_the_interface()
 {
    let long: String = (1..=6000)
        .map(|i| if i % 1000 == 0 { "needle\n".into() } else { "x\n".to_string() })
        .collect();
    let mut r = body("ui11-long", Skin::Flat, &long, (80, 24), |_| {});
    r.keys("/needle\n");
    let t = Tones::new(&palette(BG));
    assert!(
        bar(&r).iter().all(|x| x.1 != Some(t.accent_fg())),
        "over 5,000 lines the search is not scanned for marks"
    );
    // Without the interface the ruler is LOOK-4's, as it was.
    let calm = rig_env("ui11-calm", &[&format!("a.rs={}", lines(30))], &["a.rs"], (80, 24), |_| {});
    assert_eq!(calm.sim.geo().rule, 0);
    let depth =
        rig_env("ui11-depth", &[&format!("a.rs={}", lines(30))], &["a.rs"], (80, 24), |e| {
            e.skin = Skin::Depth
        });
    assert!(depth.shot.frame.rows.iter().all(|x| !x.text.contains(['⎺', '⎻', '⎼', '⎽'])));
}

#[test]
fn ui_11_a_click_on_the_scrollbar_changes_nothing() {
    let mut r = body("ui11-click", Skin::Flat, &lines(300), (80, 24), |_| {});
    let g = r.sim.geo();
    let before = (r.app().buf.cursor, r.app().top);
    r.click(g.tx() + g.ew, g.y0 + 5);
    assert_eq!((r.app().buf.cursor, r.app().top), before);
    assert_eq!(g.rule, 1, "the column is reserved in flat");
}
