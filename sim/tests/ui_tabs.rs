//! UI-4: the tab bar, as bufferline draws it: the names, the window of tabs when they do not fit,
//! what a tab says about its file, and where a click lands (pure), and the bar in sim's state.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use sim::color::{Chg, Syntax, Tones};
use sim::layout::Skin;
use sim::sim::SimEvent;
use sim::tabs::{Hit, Tab, hit, layout, names, window};
use sim_core::core::app::Event;
use sim_core::core::color::{Rgb, palette};
use unicode_width::UnicodeWidthStr;

const BG: Rgb = (0x1e, 0x1e, 0x1e);

#[test]
fn ui_4_a_tab_has_the_name_of_its_file_and_the_directory_above_it_when_two_names_clash() {
    assert_eq!(names(&["a.rs", "b.rs"]), ["a.rs", "b.rs"]);
    assert_eq!(
        names(&["src/mod.rs", "tests/mod.rs", "lib.rs"]),
        ["src/mod.rs", "tests/mod.rs", "lib.rs"]
    );
    // As many components as it takes to tell them apart, for the ones that clash only.
    assert_eq!(
        names(&["a/x/mod.rs", "b/x/mod.rs", "c/y/mod.rs"]),
        ["a/x/mod.rs", "b/x/mod.rs", "y/mod.rs"]
    );
    assert_eq!(names(&["x/mod.rs", "/abs/y/mod.rs", "z.rs"]), ["x/mod.rs", "y/mod.rs", "z.rs"]);
    // The same path twice (two names of one file are one buffer, BUF-1) cannot be told apart: whole.
    assert_eq!(names(&["a.rs", "a.rs"]).len(), 2);
    assert_eq!(names(&[]), Vec::<String>::new());
    assert_eq!(names(&["[no name]"]), ["[no name]"]);
}

#[test]
fn ui_4_the_window_keeps_the_shown_tab_whole_and_adds_its_neighbors_while_they_fit() {
    let w = [10, 10, 10, 10, 10];
    assert_eq!(window(&w, 0, 100), 0..5, "all fit");
    // 25 columns: the shown tab, then to the right, then to the left; a column for each arrow.
    assert_eq!(window(&w, 0, 25), 0..2);
    assert_eq!(window(&w, 4, 25), 3..5);
    assert_eq!(window(&w, 2, 35), 2..5, "the right first, then the left if it still fits");
    assert_eq!(window(&w, 2, 45), 1..5, "and the left after it");
    for cur in 0..5 {
        for avail in 1..60 {
            let r = window(&w, cur, avail);
            assert!(r.contains(&cur), "{cur} {avail}: the shown tab is shown");
            let used: usize = w[r.clone()].iter().sum::<usize>()
                + usize::from(r.start > 0)
                + usize::from(r.end < 5);
            assert!(used <= avail || r == (cur..cur + 1), "{cur} {avail}: {r:?} uses {used}");
        }
    }
    assert_eq!(window(&[], 0, 10), 0..0);
    assert_eq!(window(&[30], 0, 10), 0..1, "a tab wider than the bar is still shown");
}

fn tab(name: &str) -> Tab {
    Tab { name: name.into(), ..Tab::default() }
}

#[test]
fn ui_4_a_tab_says_if_its_file_is_modified_changed_on_disk_or_has_errors_and_the_shown_one_is_raised()
 {
    let t = Tones::new(&palette(BG));
    let tabs = [
        tab("a.rs"),
        Tab { dirty: true, ..tab("b.rs") },
        Tab { disk: true, ..tab("c.rs") },
        Tab { errors: 2, warns: 1, ..tab("d.rs") },
    ];
    let (line, hits) = layout(&tabs, true, 1, 120, Some(&t));
    let text = &line.row.text;
    assert_eq!(UnicodeWidthStr::width(text.as_str()), 120);
    for want in ["a.rs", "b.rs ●", "c.rs ↯", "d.rs ✘ 2 ▲ 1"] {
        assert!(text.contains(want), "{want:?} in {text:?}");
    }
    let col = |s: &str| text[..text.find(s).unwrap()].chars().count() as u16;
    let paint = |x: u16| {
        line.paints
            .iter()
            .filter(|(c, _)| c.contains(&x))
            .fold((None, None, false), |a, (_, p)| (a.0.or(p.fg), a.1.or(p.bg), a.2 || p.bold))
    };
    // The shown tab (b.rs) is raised, bold, and has the bar at its left in blue.
    let b = col("b.rs");
    assert_eq!(paint(b).1, Some(t.card));
    assert!(paint(b).2);
    let bar = text.chars().position(|c| c == '▎').unwrap() as u16;
    assert!(bar < b && b - bar <= 4, "the indicator is at the left of the shown tab");
    assert_eq!(paint(bar).0, Some(t.accent_fg()), "in the accent");
    assert_eq!(text.matches('▎').count(), 1, "only the shown tab has it");
    assert_eq!(paint(col("a.rs")).1, Some(t.bar), "the others are on the bar");
    // The marks: a bold dot for modified, a green arrow for changed on disk (COLOR-3: a shape each).
    assert_eq!(paint(col("b.rs ●") + 5).0, None, "unsaved: no color, bold");
    assert!(paint(col("b.rs ●") + 5).2);
    assert_eq!(
        paint(col("c.rs ↯") + 5).0,
        Some(t.change_fg(Chg::Add)),
        "a change on disk: a change's green"
    );
    // The counts are told by their shapes, in the text's own gray.
    assert_eq!(paint(col("✘")).0, None);
    assert_eq!(paint(col("▲")).0, None);
    // Thin separators between the tabs, not after the last.
    assert_eq!(text.matches('▏').count(), 3);
    // Every tab can be clicked, on its own columns.
    let tabs_hit: Vec<Hit> = hits.iter().map(|h| h.1).collect();
    assert_eq!(tabs_hit, [Hit::Tab(0), Hit::Tab(1), Hit::Tab(2), Hit::Tab(3)]);
    for (r, h) in &hits {
        assert_eq!(hit(&tabs, true, 1, 120, r.start), Some(*h));
        assert_eq!(hit(&tabs, true, 1, 120, r.end - 1), Some(*h));
    }
    assert_eq!(hit(&tabs, true, 1, 120, 119), None, "the bar past the tabs");
}

#[test]
fn ui_4_where_tabs_are_hidden_the_bar_has_arrows_that_switch_to_the_previous_and_the_next() {
    let t = Tones::new(&palette(BG));
    let tabs: Vec<Tab> = (0..8).map(|i| tab(&format!("file{i}.rs"))).collect();
    for cur in [0usize, 3, 7] {
        let (line, hits) = layout(&tabs, false, cur, 40, Some(&t));
        let text = &line.row.text;
        assert_eq!(UnicodeWidthStr::width(text.as_str()), 40, "{cur}: {text:?}");
        assert!(
            text.contains(&format!("file{cur}.rs")),
            "{cur}: the shown tab is shown whole: {text:?}"
        );
        assert_eq!(text.starts_with('‹'), cur > 0, "{cur}: {text:?}");
        assert_eq!(text.contains('›'), cur < 7, "{cur}: {text:?}");
        let arrows: Vec<Hit> =
            hits.iter().map(|h| h.1).filter(|h| !matches!(h, Hit::Tab(_))).collect();
        assert_eq!(arrows.contains(&Hit::Prev), cur > 0);
        assert_eq!(arrows.contains(&Hit::Next), cur < 7);
    }
    assert_eq!(hit(&tabs, false, 3, 40, 0), Some(Hit::Prev));
    assert_eq!(hit(&tabs, false, 3, 40, 39), Some(Hit::Next));
}

// ---- in sim's state ----

/// `a.rs`, `b.rs` and `sub/a.rs` open (in that order) in the interface.
fn open3(name: &str, size: (u16, u16)) -> Rig {
    let mut r = rig_env(name, &["a.rs", "b.rs", "sub/a.rs"], &["a.rs"], size, |e| {
        e.skin = Skin::Flat;
        e.syn = Syntax::new(true);
        e.chrome = true;
    });
    r.send(SimEvent::Term(Event::Bg(BG)));
    r.keys(":e b.rs\n:e sub/a.rs\n");
    assert_eq!(r.sim.bufs.len(), 3);
    r
}

fn bar_row(r: &Rig) -> (u16, String) {
    let g = r.sim.geo();
    let y = g.y0 - 1;
    (y, r.shot.frame.rows[usize::from(y)].text.clone())
}

#[test]
fn ui_4_every_open_file_has_a_tab_and_the_names_that_clash_have_their_directory() {
    let r = open3("ui4-names", (100, 24));
    let (_, text) = bar_row(&r);
    for want in ["a.rs", "b.rs", "sub/a.rs"] {
        assert!(text.contains(want), "{want:?} in {text:?}");
    }
    assert!(
        !text.contains("2/3"),
        "the tab bar says where you are, the status line does not: {}",
        r.status()
    );
    // The shown tab is the last opened: its indicator.
    let t = Tones::new(&palette(BG));
    let g = r.sim.geo();
    let x = text.find("sub/a.rs").unwrap();
    let col = text[..x].chars().count() as u16;
    let p = r.shot.paints[usize::from(g.y0 - 1)]
        .iter()
        .find(|(c, _)| c.contains(&(col + g.ex - 3)))
        .map(|p| p.1);
    assert!(p.is_some());
    let _ = t;
}

#[test]
fn ui_4_a_click_on_a_tab_shows_that_file_and_the_arrows_go_one_file_back_or_on() {
    let mut r = open3("ui4-click", (100, 24));
    let (y, text) = bar_row(&r);
    let g = r.sim.geo();
    let at = |s: &str| g.ex + text[..text.find(s).unwrap()].chars().count() as u16;
    assert_eq!(r.app().path.as_deref(), Some("sub/a.rs"));
    r.click(at("a.rs"), y);
    assert_eq!(r.app().path.as_deref(), Some("a.rs"), "the first tab");
    r.click(at("b.rs"), y);
    assert_eq!(r.app().path.as_deref(), Some("b.rs"));
    // A click is only a switch: nothing was edited, nothing saved.
    assert!(r.sim.bufs.iter().all(|b| !b.app.buf.dirty()));
    assert!(r.log.iter().all(|e| !matches!(
        e,
        sim::sim::SimEffect::App(_, sim_core::core::app::Effect::Write { .. })
    )));
    // The bar below the tabs and the text's rows are not the bar.
    r.click(at("b.rs"), y + 1);
    assert_eq!(r.app().path.as_deref(), Some("b.rs"));
    // A click on the bar's empty part does nothing.
    r.click(g.ex + g.ew + g.sign - 2, y);
    assert_eq!(r.app().path.as_deref(), Some("b.rs"));
}

#[test]
fn ui_4_a_click_ends_what_the_file_it_leaves_had_pending_and_never_leaves_a_command_line() {
    let mut r = open3("ui4-pending", (100, 24));
    let (y, text) = bar_row(&r);
    let g = r.sim.geo();
    let first = g.ex + text.find("a.rs").unwrap() as u16;
    r.keys("v"); // a selection in the file the click leaves
    assert!(r.app().vi.mode.visual());
    r.click(first, y);
    assert_eq!(r.app().path.as_deref(), Some("a.rs"));
    assert!(r.sim.bufs[2].app.vi.mode == sim_core::core::vi::Mode::Normal, "the selection ended");
    r.keys(":");
    r.click(first + 6, y);
    assert_eq!(r.app().vi.mode, sim_core::core::vi::Mode::Cmd, "the command line stays");
    assert_eq!(r.app().path.as_deref(), Some("a.rs"));
}

#[test]
fn ui_4_a_tab_shows_the_dot_of_its_file_and_the_green_arrow_of_a_change_on_disk() {
    let mut r = open3("ui4-dots", (100, 24));
    let (_, text) = bar_row(&r);
    assert!(!text.contains('●'), "{text}");
    r.keys("x");
    assert!(bar_row(&r).1.contains("sub/a.rs ●"), "modified: {}", bar_row(&r).1);
    let id = r.sim.bufs[1].id;
    r.send(SimEvent::App(id, disk(b"changed\n")));
    let t = bar_row(&r).1;
    assert!(t.contains("b.rs ↯"), "changed on disk: {t}");
    let tones = Tones::new(&palette(BG));
    let g = r.sim.geo();
    let x = g.ex + t[..t.find("b.rs ↯").unwrap()].chars().count() as u16 + 5;
    let p = r.shot.paints[usize::from(g.y0 - 1)].iter().find(|(c, _)| c.contains(&x)).unwrap().1;
    assert_eq!(p.fg, Some(tones.change_fg(Chg::Add)));
}

#[test]
fn ui_4_the_slot_for_diagnostics_shows_a_server_s_counts_and_nothing_without_them() {
    let mut r = open3("ui4-diag", (100, 24));
    assert!(!bar_row(&r).1.contains('✘') && !bar_row(&r).1.contains('▲'));
    r.sim.bufs[0].diag = (3, 0);
    r.sim.bufs[1].diag = (0, 2);
    r.keys("j");
    let t = bar_row(&r).1;
    assert!(t.contains("a.rs ✘ 3") && t.contains("b.rs ▲ 2"), "{t}");
}

#[test]
fn ui_4_a_screen_too_short_for_the_interface_has_no_tab_bar_and_the_status_line_counts_the_files() {
    let r = open3("ui4-short", (100, 9));
    assert_eq!(r.sim.geo().head, 0);
    assert!(r.status().contains("3/3"), "{}", r.status());
}
