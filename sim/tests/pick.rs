//! PICK-1 to PICK-5: the picker, Ctrl-p and Ctrl-f's floating window: its geometry, the rows of its
//! list, how it is put over the screen, and (driven through `Sim::update`) its keys, its preview
//! and its count.
#![allow(clippy::unwrap_used, clippy::single_range_in_vec_init)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use si::core::app::Event;
use si::core::color::{Rgb, palette};
use si::core::view::{Row, Shape, Style};
use sim::color::{Syntax, Tones};
use sim::layout::{Line, Skin};
use sim::paint::{Look, Paint};
use sim::pick::{self, Rect, SPIN, Win};
use sim::sim::{SimEffect, SimEvent};

const BG: Rgb = (0x1e, 0x1e, 0x1e);

// ---- PICK-1: the geometry ----

#[test]
fn pick_1_the_window_is_80_percent_of_the_screen_centered() {
    let w = Win::new((200, 56)).unwrap();
    assert_eq!(
        (w.x, w.y, w.w, w.h),
        (20, 5, 160, 44),
        "80% of 200 and of the 55 rows above the status line"
    );
    assert!(!w.stacked);
    // the prompt, a rule, the list; the preview beside them, the divider between: 62 + 95 + 3
    assert_eq!(w.prompt, Rect { x: 21, y: 6, w: 62, h: 1 });
    assert_eq!(w.list, Rect { x: 21, y: 8, w: 62, h: 40 });
    assert_eq!(w.pane, Rect { x: 84, y: 6, w: 95, h: 42 });
}

#[test]
fn pick_1_under_100_columns_the_preview_goes_under_the_list() {
    let w = Win::new((80, 24)).unwrap();
    assert_eq!((w.x, w.y, w.w, w.h), (8, 2, 64, 18));
    assert!(w.stacked);
    assert_eq!(w.prompt, Rect { x: 9, y: 3, w: 62, h: 1 });
    assert_eq!(
        w.list,
        Rect { x: 9, y: 5, w: 62, h: 5 },
        "two fifths of the 13 rows between the borders"
    );
    assert_eq!(w.pane, Rect { x: 9, y: 11, w: 62, h: 8 }, "after a row of border with the title");
    // 100 columns is the first width that puts the preview beside the list
    assert!(!Win::new((125, 30)).unwrap().stacked && Win::new((124, 30)).unwrap().stacked);
}

#[test]
fn pick_1_a_screen_too_small_has_no_window() {
    assert!(Win::new((19, 24)).is_none());
    assert!(Win::new((80, 7)).is_none());
    let w = Win::new((20, 8)).unwrap();
    assert!(w.x + w.w <= 20 && w.y + w.h < 8, "{w:?}");
    for size in [(20, 8), (21, 9), (40, 10), (65535, 65535), (100, 8), (1000, 12)] {
        let w = Win::new(size).unwrap();
        assert!(u32::from(w.x) + u32::from(w.w) <= u32::from(size.0), "{size:?} {w:?}");
        assert!(u32::from(w.y) + u32::from(w.h) < u32::from(size.1), "{size:?} {w:?}");
        for r in [w.prompt, w.list, w.pane] {
            assert!(
                r.x >= w.x && u32::from(r.x) + u32::from(r.w) <= u32::from(w.x) + u32::from(w.w),
                "{size:?} {w:?}"
            );
            assert!(
                r.y >= w.y && u32::from(r.y) + u32::from(r.h) <= u32::from(w.y) + u32::from(w.h),
                "{size:?} {w:?}"
            );
        }
    }
}

// ---- PICK-5: the count ----

#[test]
fn pick_5_a_count_has_thousands_separators() {
    assert_eq!(pick::thousands(0), "0");
    assert_eq!(pick::thousands(37), "37");
    assert_eq!(pick::thousands(999), "999");
    assert_eq!(pick::thousands(1000), "1,000");
    assert_eq!(pick::thousands(2104), "2,104");
    assert_eq!(pick::thousands(1_234_567), "1,234,567");
    assert_eq!(SPIN.len(), 10);
}

// ---- PICK-1: the window over the rows of the screen ----

fn row(text: &str) -> Row {
    Row { text: text.into(), ..Row::default() }
}

fn put(l: &mut Line, s: &str) {
    l.put(s, Paint::default());
}

#[test]
fn pick_1_the_window_covers_the_cells_it_is_over_and_leaves_the_others() {
    let mut rows = [row("0123456789abcdef")];
    let (mut looks, mut paints) = (
        vec![vec![(0..16, Look::Kw)]],
        vec![vec![(0..16, Paint { fade: 30, ..Paint::default() })]],
    );
    let mut win = Line::default();
    put(&mut win, "XYZ");
    win.looks.push((0..1, Look::Bold));
    win.row.spans.push((1..2, Style::Secondary));
    win.paints.push((0..3, Paint { bold: true, ..Paint::default() }));
    pick::splice(&mut rows[0], &mut looks[0], &mut paints[0], 4, win);
    assert_eq!(rows[0].text, "0123XYZ789abcdef");
    // what was under the window is gone from the looks and paints; what was beside it stays
    assert_eq!(looks[0], [(0..4, Look::Kw), (7..16, Look::Kw), (4..5, Look::Bold)]);
    assert_eq!(rows[0].spans, [(5..6, Style::Secondary)]);
    assert_eq!(paints[0].len(), 3);
    assert_eq!(paints[0][0].0, 0..4);
    assert_eq!(paints[0][1].0, 7..16);
    assert_eq!(paints[0][2], (4..7, Paint { bold: true, ..Paint::default() }));
}

#[test]
fn pick_1_a_wide_char_cut_by_the_edge_of_the_window_is_a_blank() {
    // 日 本 語 take two columns each: the window covers columns 1..5, cutting 日 and 語
    let mut r = row("日本語ab");
    let (mut l, mut p) = (vec![], vec![]);
    let mut win = Line::default();
    put(&mut win, "wxyz");
    pick::splice(&mut r, &mut l, &mut p, 1, win);
    assert_eq!(r.text, " wxyz ab", "the halves of 日 and of 語 are blanks");
    assert_eq!(width(&r.text), 8);
    // a row shorter than the window's start is padded up to it
    let mut r = row("ab");
    pick::splice(&mut r, &mut l, &mut p, 5, {
        let mut w = Line::default();
        put(&mut w, "Q");
        w
    });
    assert_eq!(r.text, "ab   Q");
}

#[test]
fn pick_1_a_combining_char_of_a_covered_cell_goes_with_it() {
    let mut r = row("ae\u{301}b");
    let (mut l, mut p) = (vec![], vec![]);
    let mut win = Line::default();
    put(&mut win, "Z");
    pick::splice(&mut r, &mut l, &mut p, 1, win);
    assert_eq!(r.text, "aZb", "the accent of the é under the window is not left over b");
}

// ---- driving the picker through the state ----

const FILES: [&str; 5] = [
    "a.txt=alpha needle\nbeta\n",
    "sub/b.txt=gamma\nneedle again and Needle\n",
    "c.txt=nothing here\n",
    "src/main.rs=fn main() {\n    let needle = 1;\n}\n",
    "src/lib.rs=pub fn lib() {}\n",
];

fn pick_rig(name: &str, skin: Skin, size: (u16, u16)) -> Rig {
    let mut r = rig_env(name, &FILES, &["a.txt"], size, |e| {
        e.skin = skin;
        e.syn = Syntax::new(true);
        e.chrome = true;
    });
    r.send(SimEvent::Term(Event::Bg(BG)));
    r
}

/// The merged paint of a cell: each field from the first range that has one.
fn cell(r: &Rig, y: u16, x: u16) -> Paint {
    let on = r.shot.paints[usize::from(y)].iter().filter(|(c, _)| c.contains(&x)).map(|p| p.1);
    on.fold(Paint::default(), |a, p| Paint {
        fg: a.fg.or(p.fg),
        bg: a.bg.or(p.bg),
        fade: if a.fade > 0 { a.fade } else { p.fade },
        bold: a.bold || p.bold,
        soft: a.soft || p.soft,
        sealed: a.sealed || p.sealed,
    })
}

#[test]
fn pick_1_ctrl_f_and_ctrl_p_open_a_window_with_a_rounded_border_and_a_title() {
    let mut r = pick_rig("p1-open", Skin::Flat, (130, 40));
    assert!(r.float().is_empty(), "no window before");
    r.keys("<C-f>");
    let w = r.win().unwrap();
    assert_eq!((w.x, w.y, w.w, w.h), (13, 4, 104, 31));
    let f = r.float();
    assert_eq!(f.len(), 31);
    assert!(f[0].starts_with("╭─ Live grep") && f[0].ends_with("─╮"), "{}", f[0]);
    assert!(f[0].contains('┬'), "the divider of the preview starts here");
    assert!(f[30].starts_with("╰─") && f[30].ends_with("─╯") && f[30].contains('┴'), "{}", f[30]);
    assert!(f[1].starts_with("│ ❯ "), "the prompt: {}", f[1]);
    assert!(f[2].starts_with('├') && f[2].contains('┤'), "the rule under it: {}", f[2]);
    r.keys("<Esc><C-p>");
    assert!(r.float()[0].starts_with("╭─ Find files"), "{}", r.float()[0]);
    assert_eq!(r.screen.shape, Shape::Bar, "the cursor is at the end of the query");
    assert_eq!(r.screen.cursor, (13 + 1 + 3, 4 + 1));
    r.keys("ab");
    assert_eq!(r.screen.cursor, (13 + 1 + 3 + 2, 4 + 1));
}

#[test]
fn pick_1_nothing_behind_the_window_changes() {
    let mut r = pick_rig("p1-behind", Skin::Depth, (120, 40));
    r.keys("<C-n>"); // the tree panel opens
    let before = (r.screen.rows.clone(), r.shot.paints.clone());
    r.keys("<C-f>");
    let w = r.win().unwrap();
    let (rows, paints) = (&r.screen.rows, &r.shot.paints);
    for y in 0..39usize {
        for x in 0..120u16 {
            let inside =
                x >= w.x && x < w.x + w.w && y >= usize::from(w.y) && y < usize::from(w.y + w.h);
            let shadow = (x == w.x + w.w && y > usize::from(w.y) && y <= usize::from(w.y + w.h))
                || (y == usize::from(w.y + w.h) && x > w.x && x <= w.x + w.w);
            if inside || shadow {
                continue;
            }
            let at = |rows: &[Row]| -> Option<char> { rows[y].text.chars().nth(usize::from(x)) };
            // (the status line and the cursor's place may differ; the body does not)
            assert_eq!(at(rows), at(&before.0), "({x},{y})");
            let own = |p: &[Vec<(std::ops::Range<u16>, Paint)>]| -> Paint {
                let on = p[y].iter().filter(|(c, _)| c.contains(&x)).map(|p| p.1);
                on.fold(Paint::default(), |a, p| Paint {
                    fg: a.fg.or(p.fg),
                    bg: a.bg.or(p.bg),
                    fade: a.fade.max(p.fade),
                    bold: a.bold || p.bold,
                    soft: a.soft || p.soft,
                    sealed: a.sealed || p.sealed,
                })
            };
            assert_eq!(own(paints), own(&before.1), "paint ({x},{y})");
        }
    }
    assert!(r.sim.panel.open, "the panel was not touched");
}

#[test]
fn pick_1_depth_has_a_shadow_at_the_right_and_below_and_flat_has_none() {
    for (skin, shadow) in [(Skin::Depth, true), (Skin::Flat, false), (Skin::Calm, false)] {
        let mut r = pick_rig("p1-shadow", skin, (120, 40));
        r.keys("<C-f>");
        let (w, t) = (r.win().unwrap(), Tones::new(&palette(BG)));
        let right = cell(&r, w.y + 5, w.x + w.w);
        let below = cell(&r, w.y + w.h, w.x + 5);
        let corner = cell(&r, w.y + w.h, w.x + w.w);
        let clear = cell(&r, w.y + w.h + 1, w.x + w.w + 1);
        if shadow {
            for (c, name) in [(right, "right"), (below, "below"), (corner, "corner")] {
                assert_eq!(c.bg, Some(t.shadow), "{name}");
                assert_eq!(c.fade, 70, "{name}");
            }
            assert_ne!(clear.bg, Some(t.shadow), "past the shadow");
            // the shadow is shifted by one cell: the window's own first row and column have none
            assert_ne!(cell(&r, w.y, w.x + w.w).bg, Some(t.shadow));
            assert_ne!(cell(&r, w.y + w.h, w.x).bg, Some(t.shadow));
        } else {
            assert_ne!(right.bg, Some(t.shadow), "{skin:?}");
            assert_ne!(below.bg, Some(t.shadow), "{skin:?}");
        }
    }
}

#[test]
fn pick_1_the_window_is_on_a_neutral_surface_and_calm_has_no_colors() {
    let mut r = pick_rig("p1-surface", Skin::Flat, (120, 40));
    r.keys("<C-f>");
    let (w, t) = (r.win().unwrap(), Tones::new(&palette(BG)));
    for (x, y) in [(w.x + 1, w.y + 1), (w.x + 40, w.y + 20), (w.x + w.w - 2, w.y + w.h - 2)] {
        assert_eq!(cell(&r, y, x).bg, Some(t.cursor), "({x},{y})");
    }
    let mut r = pick_rig("p1-calm", Skin::Calm, (120, 40));
    r.keys("<C-f>needle");
    assert!(r.shot.paints.iter().all(Vec::is_empty), "calm paints nothing");
    assert!(r.selected_in_float().is_some(), "the selected row is underlined");
}

#[test]
fn pick_1_a_screen_under_20_columns_draws_no_window_but_keys_still_work() {
    let mut r = pick_rig("p1-tiny", Skin::Flat, (19, 12));
    r.keys("<C-f>x");
    assert!(r.win().is_none());
    assert!(r.sim.picker_open(), "the picker is open");
    r.keys("<Esc>");
    assert!(!r.sim.picker_open());
}

// ---- PICK-2: the keys ----

#[test]
fn pick_2_typing_never_reaches_a_buffer() {
    for (mode, name) in [("", "normal"), ("i", "insert"), ("v", "visual"), (":", "command")] {
        let mut r = pick_rig(&format!("p2-leak-{name}"), Skin::Flat, (100, 30));
        let text = r.text();
        r.keys(mode);
        r.keys("<C-f>dd:wq\niXx<BS><Del><Tab><CR>yyp");
        assert!(r.sim.picker_open());
        r.keys("<Esc><Esc>");
        assert_eq!(r.sim.bufs[0].app.buf.text.to_string(), text, "{name}: nothing was typed");
        assert!(!r.app().buf.dirty(), "{name}");
        assert!(r.quit.is_none(), "{name}: `:wq` was a char of the query");
    }
}

#[test]
fn pick_2_a_paste_with_the_panel_closed_does_not_reach_the_text() {
    let mut r = pick_rig("p2-paste", Skin::Flat, (100, 30));
    let text = r.text();
    r.keys("<C-f>");
    r.send(SimEvent::Term(Event::Paste("zzz".into())));
    r.keys("<Esc>");
    assert_eq!(r.text(), text);
}

#[test]
fn pick_2_ctrl_n_and_ctrl_p_the_arrows_and_ctrl_j_and_ctrl_k_move_the_selection() {
    let mut r = pick_rig("p2-move", Skin::Flat, (120, 40));
    r.keys("<C-f>needle");
    let sel = |r: &Rig| r.list().iter().position(|l| l.starts_with('▎')).unwrap();
    assert_eq!(sel(&r), 0);
    r.keys("<C-n>");
    assert_eq!(sel(&r), 1);
    r.keys("<Down>");
    assert_eq!(sel(&r), 2, "three matching lines");
    r.keys("<C-j><C-n>");
    assert_eq!(sel(&r), 2, "stops at the last");
    r.keys("<C-p>");
    assert_eq!(sel(&r), 1);
    r.keys("<Up><C-k>");
    assert_eq!(sel(&r), 0, "stops at the first");
    r.keys("<C-p><Up>");
    assert_eq!(sel(&r), 0);
    assert!(r.sim.picker_open(), "Ctrl-n and Ctrl-p are not the panel's and FIND-4's now");
    assert!(!r.sim.panel.open);
    r.keys("<PageDown>");
    assert_eq!(sel(&r), 2);
    r.keys("<PageUp>");
    assert_eq!(sel(&r), 0);
}

#[test]
fn pick_2_ctrl_u_clears_the_query_and_ctrl_w_takes_off_a_word_and_backspace_a_char() {
    let mut r = pick_rig("p2-edit", Skin::Flat, (120, 40));
    r.keys("<C-f>alpha needle");
    assert_eq!(r.query(), "alpha needle");
    r.keys("<C-w>x");
    assert_eq!(r.query(), "alpha x", "the word went, the blank before it stayed");
    r.keys("<BS><BS><BS>");
    assert_eq!(r.query(), "alph");
    r.keys("<C-u>");
    assert_eq!(r.query(), "");
    assert!(r.sim.picker_open());
    assert!(r.list().is_empty(), "an empty query searches nothing");
    r.keys("needle<C-u>");
    assert_eq!(r.query(), "");
    assert!(r.list().is_empty());
}

#[test]
fn pick_2_esc_and_backspace_on_an_empty_query_close_it_and_everything_is_as_before() {
    let mut r = pick_rig("p2-esc", Skin::Flat, (120, 40));
    r.keys("<C-n>l"); // the panel is open with the focus, a directory open
    let before = (
        r.sim.panel.open,
        r.sim.panel.focus,
        r.sim.panel.cur,
        r.panel(),
        r.screen.rows.last().unwrap().text.clone(),
        r.sim.bufs.len(),
        r.text(),
    );
    let saved = r.jobs.iter().filter(|j| **j == "history").count();
    r.keys("<C-f>needle<C-n><C-n><Esc>");
    assert!(!r.sim.picker_open());
    let after = (
        r.sim.panel.open,
        r.sim.panel.focus,
        r.sim.panel.cur,
        r.panel(),
        r.screen.rows.last().unwrap().text.clone(),
        r.sim.bufs.len(),
        r.text(),
    );
    assert_eq!(after, before);
    assert_eq!(r.jobs.iter().filter(|j| **j == "history").count(), saved, "no history");
    r.keys("<C-p>x<BS><BS>");
    assert!(!r.sim.picker_open(), "Backspace on an empty query closes the picker");
    assert_eq!(r.sim.panel.cur, before.2);
}

#[test]
fn pick_2_tab_the_mouse_and_other_keys_do_nothing_while_it_is_open() {
    let mut r = pick_rig("p2-inert", Skin::Flat, (120, 40));
    r.keys(":e c.txt\n");
    assert_eq!(r.sim.cur, 1);
    r.keys("<C-f>needle");
    let (rows, sel) = (r.float(), r.sim.bufs.len());
    r.keys("<Tab><Left><Right><Home><End><Del>");
    r.click(60, 20);
    r.click(3, 3);
    r.wheel(true, 60, 20);
    assert_eq!((r.float(), r.sim.bufs.len(), r.sim.cur), (rows, sel, 1));
}

#[test]
fn pick_2_ctrl_f_in_the_file_finder_starts_the_search_of_contents_and_ctrl_p_in_it_is_up() {
    let mut r = pick_rig("p2-switch", Skin::Flat, (120, 40));
    r.keys("<C-p>txt");
    assert!(r.float()[0].contains("Find files"));
    r.keys("<C-f>");
    assert!(r.float()[0].contains("Live grep") && r.query().is_empty());
    r.keys("needle<C-p>");
    assert!(r.float()[0].contains("Live grep"), "Ctrl-p moves up in a search of contents");
}

#[test]
fn pick_2_enter_closes_it_and_opens_the_result() {
    let mut r = pick_rig("p2-enter", Skin::Flat, (120, 40));
    r.keys("<C-p>c.t<CR>");
    assert!(!r.sim.picker_open());
    assert_eq!(r.app().path.as_deref(), Some("c.txt"));
    assert!(!r.sim.panel.open, "the panel stays as it was: closed");
    r.keys("<C-f>again<CR>");
    assert_eq!(r.app().path.as_deref(), Some("sub/b.txt"));
    assert_eq!(r.app().buf.line_of(r.app().buf.cursor), 1, "at the line of the match");
}

// ---- PICK-3: the list ----

fn sec_at(r: &Rig, y: u16, cols: std::ops::Range<u16>) -> bool {
    r.screen.rows[usize::from(y)]
        .spans
        .iter()
        .any(|(c, s)| *s == Style::Secondary && c.start <= cols.start && c.end >= cols.end)
}

fn bold_at(r: &Rig, y: u16, cols: std::ops::Range<u16>) -> bool {
    r.shot.looks[usize::from(y)]
        .iter()
        .any(|(c, l)| *l == Look::Bold && c.start <= cols.start && c.end >= cols.end)
}

#[test]
fn pick_3_a_result_of_a_search_is_path_line_and_text_with_the_matches_bold() {
    let mut r = pick_rig("p3-rows", Skin::Flat, (130, 40));
    r.keys("<C-f>needle");
    assert_eq!(
        r.list(),
        [
            "▎a.txt:1  alpha needle",
            " src/main.rs:2  let needle = 1;",
            " sub/b.txt:2  needle again and Needle",
        ],
        "the likelier file first (it is open), then by path; the selected row starts with the bar"
    );
    let (w, t) = (r.win().unwrap(), Tones::new(&palette(BG)));
    let y = w.list.y;
    let x = w.list.x;
    // the path and the number are secondary, the match is bold and in the accent
    assert!(sec_at(&r, y, x + 1..x + 8) && !sec_at(&r, y, x + 10..x + 22));
    assert!(bold_at(&r, y, x + 16..x + 22) && !bold_at(&r, y, x + 10..x + 15));
    assert_eq!(cell(&r, y, x + 17).fg, Some(t.accent_fg()));
    assert_eq!(cell(&r, y, x + 11).fg, None);
    // the second match of a line: `Needle` is not a match (smartcase: the query is lowercase, so it is)
    assert!(bold_at(&r, y + 2, x + 1 + 11 + 2..x + 1 + 11 + 8));
}

#[test]
fn pick_3_the_selected_row_has_the_accent_s_block_and_a_bar_and_calm_underlines_it() {
    let mut r = pick_rig("p3-sel", Skin::Flat, (130, 40));
    r.keys("<C-f>needle");
    let (w, t) = (r.win().unwrap(), Tones::new(&palette(BG)));
    let (back, _) = t.accent_block();
    for x in [w.list.x, w.list.x + 10, w.list.x + w.list.w - 1] {
        assert_eq!(cell(&r, w.list.y, x).bg, Some(back), "across the selected row, x={x}");
        assert_ne!(cell(&r, w.list.y + 1, x).bg, Some(back), "not the next row, x={x}");
    }
    assert_eq!(cell(&r, w.list.y, w.list.x).fg, Some(t.accent_fg()), "the bar");
    r.keys("<C-n>");
    assert_eq!(cell(&r, w.list.y + 1, w.list.x + 10).bg, Some(back));
    assert_ne!(cell(&r, w.list.y, w.list.x + 10).bg, Some(back));
    let mut r = pick_rig("p3-sel-calm", Skin::Calm, (130, 40));
    r.keys("<C-f>needle<C-n>");
    assert_eq!(r.selected_in_float(), Some(1));
}

#[test]
fn pick_3_the_file_finder_lists_the_best_first_with_the_directories_secondary() {
    let mut r = pick_rig("p3-files", Skin::Flat, (130, 40));
    r.keys("<C-p>txt");
    assert_eq!(
        r.list(),
        ["▎a.txt", " c.txt", " sub/b.txt"],
        "a.txt is open, so likelier; then the shorter path"
    );
    let w = r.win().unwrap();
    let (x, y) = (w.list.x, w.list.y);
    assert!(sec_at(&r, y + 2, x + 1..x + 5), "the directory `sub/` is secondary");
    assert!(!sec_at(&r, y + 2, x + 5..x + 10), "the name is not");
    assert!(bold_at(&r, y + 2, x + 7..x + 10), "`txt` of b.txt is bold");
    assert!(!bold_at(&r, y + 2, x + 5..x + 6));
    r.keys("<C-u>c.t");
    assert_eq!(r.list(), ["▎c.txt"]);
}

#[test]
fn pick_3_an_empty_query_lists_the_files_the_history_likes_then_the_rest_in_order() {
    let mut r = pick_rig("p3-empty", Skin::Flat, (130, 40));
    r.keys("<C-p>");
    assert_eq!(
        r.list(),
        ["▎a.txt", " c.txt", " src/lib.rs", " src/main.rs", " sub/b.txt"],
        "a.txt is the one that was opened"
    );
    assert_eq!(r.count(), "1 / 5");
}

#[test]
fn pick_3_no_matches_is_said_where_the_list_would_be_but_not_for_an_empty_query() {
    let mut r = pick_rig("p3-none", Skin::Flat, (130, 40));
    r.keys("<C-f>");
    assert_eq!(r.list(), Vec::<String>::new());
    r.keys("zzzz");
    assert_eq!(r.list(), [" no matches"]);
    let w = r.win().unwrap();
    assert!(sec_at(&r, w.list.y, w.list.x + 1..w.list.x + 11));
    assert_eq!(r.count(), "0 / 0");
    r.keys("<C-u><C-p>zzzz");
    assert_eq!(r.list(), [" no matches"]);
}

#[test]
fn pick_3_the_list_scrolls_to_keep_the_selected_row_in_view() {
    let files: Vec<String> = (0..40).map(|i| format!("f{i:02}.txt=x{i}\n")).collect();
    let refs: Vec<&str> = files.iter().map(String::as_str).collect();
    let mut r = rig_env("p3-scroll", &refs, &["f00.txt"], (130, 20), |e| {
        e.skin = Skin::Flat;
        e.syn = Syntax::new(true);
    });
    r.send(SimEvent::Term(Event::Bg(BG)));
    r.keys("<C-p>f");
    let rows = usize::from(r.win().unwrap().list.h);
    assert_eq!(rows, 11);
    for n in 1..=25 {
        r.keys("<C-n>");
        let list = r.list();
        let at =
            list.iter().position(|l| l.starts_with('▎')).unwrap_or_else(|| panic!("{n}: {list:?}"));
        assert!(at < rows);
        assert_eq!(list[at], format!("▎f{n:02}.txt"), "the selected row is the n-th");
    }
    r.keys("<C-p><C-p>");
    let list = r.list();
    assert!(list.iter().any(|l| l == "▎f23.txt"), "{list:?}");
    for _ in 0..30 {
        r.keys("<C-p>");
    }
    assert_eq!(r.list()[0], "▎f00.txt", "back at the top the list begins at its first row");
}

#[test]
fn pick_3_a_thousand_rows_are_listed_and_the_count_says_how_many_matched() {
    let files: Vec<String> = (0..1200).map(|i| format!("d/f{i:04}.txt=x\n")).collect();
    let refs: Vec<&str> = files.iter().map(String::as_str).collect();
    let mut r = rig_env("p3-many", &refs, &["d/f0000.txt"], (130, 40), |e| {
        e.skin = Skin::Flat;
        e.syn = Syntax::new(true);
    });
    r.send(SimEvent::Term(Event::Bg(BG)));
    r.keys("<C-p>f");
    assert_eq!(r.count(), "1 / 1,200");
    r.keys(&"<PageDown>".repeat(100));
    assert_eq!(r.count(), "1,000 / 1,200", "the thousand best are listed");
}

// ---- PICK-4: the preview ----

fn long_rig(name: &str, skin: Skin) -> Rig {
    let long: String = (1..=100)
        .map(|i| if i == 60 { "fn target() {}\n".to_string() } else { format!("line {i:03}\n") })
        .collect();
    let mut r = rig_env(
        name,
        &[&format!("long.rs={long}"), "other.txt=target of the other\nsecond\n"],
        &["other.txt"],
        (130, 40),
        |e| {
            e.skin = skin;
            e.syn = Syntax::new(true);
        },
    );
    r.send(SimEvent::Term(Event::Bg(BG)));
    r
}

#[test]
fn pick_4_the_preview_shows_the_matching_line_in_the_middle_with_numbers() {
    let mut r = long_rig("p4-middle", Skin::Flat);
    r.keys("<C-f>fn target");
    assert_eq!(r.list(), ["▎long.rs:60  fn target() {}"]);
    let (w, pane) = (r.win().unwrap(), r.pane());
    let at = pane.iter().position(|l| l.contains("fn target")).unwrap();
    assert!(pane[at].contains("60"), "{:?}", pane[at]);
    let mid = usize::from(w.pane.h) / 2;
    assert!(at.abs_diff(mid) <= 1, "row {at} of {} is the middle", w.pane.h);
    assert!(
        pane[at - 1].contains("line 059") && pane[at + 1].contains("line 061"),
        "{:?}",
        &pane[at - 1..=at + 1]
    );
    assert_eq!(r.float()[0].matches("long.rs:60").count(), 1, "the title of the pane");
}

#[test]
fn pick_4_the_matching_line_is_on_the_accent_s_block_and_its_match_on_the_solid_color() {
    let mut r = long_rig("p4-lit", Skin::Flat);
    r.keys("<C-f>get()");
    let (w, t) = (r.win().unwrap(), Tones::new(&palette(BG)));
    let pane = r.pane();
    let at = pane.iter().position(|l| l.contains("fn target")).unwrap() as u16;
    let y = w.pane.y + at;
    let (back, _) = t.accent_block();
    let (solid, on) = t.accent_solid();
    let x = w.pane.x + pane[usize::from(at)].find("get()").unwrap() as u16;
    assert_eq!(cell(&r, y, x - 1).bg, Some(back), "the line");
    assert_eq!(cell(&r, y, w.pane.x + w.pane.w - 2).bg, Some(back), "across the pane");
    assert_eq!((cell(&r, y, x).bg, cell(&r, y, x).fg), (Some(solid), Some(on)), "the match");
    assert_eq!(cell(&r, y, x + 4).bg, Some(solid));
    assert_eq!(cell(&r, y, x + 5).bg, Some(back));
    assert_ne!(cell(&r, y - 1, x).bg, Some(back), "not the line above");
    // the file finder has no matching line to light
    r.keys("<Esc><C-p>long");
    let y = w.pane.y;
    assert_ne!(cell(&r, y + 5, w.pane.x + 10).bg, Some(back));
}

#[test]
fn pick_4_the_file_finder_previews_the_first_lines_of_a_file() {
    let mut r = long_rig("p4-head", Skin::Flat);
    r.keys("<C-p>long");
    let pane = r.pane();
    assert!(pane[0].contains("line 001") && pane[1].contains("line 002"), "{:?}", &pane[..2]);
    assert_eq!(r.float()[0].matches("long.rs").count(), 1, "titled with the path, without a line");
    r.keys("<C-u>oth");
    assert!(r.pane()[0].contains("target of the other"));
}

#[test]
fn pick_4_the_preview_has_the_syntax_colors_once_the_highlighter_has_answered() {
    let mut r = long_rig("p4-colors", Skin::Flat);
    r.hold_spans = true;
    r.keys("<C-f>fn target");
    let w = r.win().unwrap();
    let kw = |r: &Rig| {
        (w.pane.y..w.pane.y + w.pane.h).any(|y| {
            r.shot.looks[usize::from(y)].iter().any(|(c, l)| *l == Look::Kw && c.start >= w.pane.x)
        })
    };
    assert!(!kw(&r), "plain until the spans come");
    let held = r.held.iter().find(|q| q.name == "long.rs").expect("a request for the preview");
    let id = held.id;
    assert!(r.sim.bufs.iter().all(|b| b.id != id), "an id no open file has");
    r.deliver_spans();
    assert!(kw(&r), "`fn` is a keyword");
    // moving the preview to another file lets the highlighter forget the first
    r.keys("<C-u>target");
    assert!(r.log.iter().any(|e| matches!(e, SimEffect::Close(i) if *i == id)), "{:?}", r.log);
}

#[test]
fn pick_4_a_preview_is_not_an_open_it_adds_no_history_buffer_or_watcher() {
    let mut r = long_rig("p4-notopen", Skin::Flat);
    let (bufs, saved) = (r.sim.bufs.len(), r.jobs.iter().filter(|j| **j == "history").count());
    r.keys("<C-f>target<C-n><Esc>");
    assert_eq!(r.sim.bufs.len(), bufs);
    assert_eq!(r.jobs.iter().filter(|j| **j == "history").count(), saved);
    assert!(!r.log.iter().any(|e| matches!(e, SimEffect::Open(..))), "{:?}", r.log);
}

#[test]
fn pick_4_a_late_preview_for_another_file_is_dropped() {
    let mut r = long_rig("p4-late", Skin::Flat);
    r.keys("<C-f>target");
    let before = r.pane();
    let file = Ok(Some(b"old file\nzzz target\n".to_vec()));
    r.send(SimEvent::Preview(sim::sim::Pv { path: "gone.txt".into(), file }));
    assert_eq!(r.pane(), before);
}

#[test]
fn pick_4_a_file_too_big_to_preview_says_so() {
    let mut r = long_rig("p4-big", Skin::Flat);
    r.keys("<C-f>fn target");
    let file = Err("too big to preview".to_string());
    r.send(SimEvent::Preview(sim::sim::Pv { path: "long.rs".into(), file }));
    assert!(r.pane().iter().any(|l| l.contains("too big to preview")), "{:?}", r.pane());
}

// ---- PICK-5: the count and the spinner ----

#[test]
fn pick_5_the_count_is_the_selected_result_over_all() {
    let mut r = pick_rig("p5-count", Skin::Flat, (130, 40));
    r.keys("<C-f>needle");
    assert_eq!(r.count(), "1 / 3");
    r.keys("<C-n><C-n>");
    assert_eq!(r.count(), "3 / 3");
    r.keys("<C-u>xyzzy");
    assert_eq!(r.count(), "0 / 0");
    assert!(r.float().last().unwrap().contains("↵ open"), "{}", r.float().last().unwrap());
}

#[test]
fn pick_5_a_spinner_runs_while_the_search_does_and_stops_when_it_is_over() {
    let mut r = pick_rig("p5-spin", Skin::Flat, (130, 40));
    r.hold_grep = true;
    r.keys("<C-f>needle");
    let generation = r.grep_held.last().unwrap().generation;
    assert!(r.count().ends_with(SPIN[0]), "{}", r.count());
    let asked = |r: &Rig| {
        r.log.iter().filter(|e| matches!(e, SimEffect::Spin(g) if *g == generation)).count()
    };
    assert!(asked(&r) >= 1, "the shell is asked for the next frame");
    let n = asked(&r);
    r.send(SimEvent::Spin(generation));
    assert!(r.count().ends_with(SPIN[1]), "{}", r.count());
    assert_eq!(asked(&r), n + 1, "and asked again");
    r.send(SimEvent::Spin(generation - 1));
    assert!(r.count().ends_with(SPIN[1]), "an old generation's is ignored");
    assert_eq!(asked(&r), n + 1);
    r.deliver_grep();
    assert_eq!(r.count(), "1 / 3", "no spinner once it is done");
    r.send(SimEvent::Spin(generation));
    assert_eq!(asked(&r), n + 1, "and the chain ends");
}

#[test]
fn pick_5_a_content_search_that_hits_the_cap_says_so() {
    let big: String = (0..1100).map(|i| format!("hit {i}\n")).collect();
    let mut r = rig("p5-cap", &[&format!("big.txt={big}")], &[]);
    r.keys("<C-f>hit");
    assert_eq!(r.count(), "1 / 1,000+");
}

// ---- the scope in the title and the keys in the bottom border ----

/// A picker over a root that is below the home directory, so that it is written `~/{name}`.
fn homed(name: &str, size: (u16, u16)) -> (Rig, String) {
    let mut r = rig_env(name, &FILES, &["a.txt"], size, |e| {
        (e.skin, e.syn, e.chrome) = (Skin::Flat, Syntax::new(true), true);
        e.home = e.cwd.parent().map(std::path::Path::to_path_buf);
    });
    r.send(SimEvent::Term(Event::Bg(BG)));
    let root = format!("~/{}", r.cwd.file_name().unwrap().to_string_lossy());
    (r, root)
}

#[test]
fn pick_1_the_top_border_names_the_mode_and_the_root_home_relative() {
    let (mut r, root) = homed("s", (130, 40));
    r.keys("<C-f>");
    let top = r.float()[0].clone();
    assert!(top.starts_with(&format!("╭─ Live grep · {root} ─")), "{top}");
    r.keys("<Esc><C-p>");
    let top = r.float()[0].clone();
    assert!(top.starts_with(&format!("╭─ Find files · {root} ─")), "{top}");
    // the same root the panel's header shows (UI-2), not the path
    assert_eq!(root, sim::sim::tilde(&r.sim.root, r.cwd.parent()));
    // the name is bold and the root the secondary gray, in flat and depth
    let (w, t) = (r.win().unwrap(), Tones::new(&palette(BG)));
    assert!(cell(&r, w.y, w.x + 3).bold, "the mode's name");
    assert_eq!(cell(&r, w.y, w.x + 3 + 12 + 3).fg, Some(t.focus), "the root: {top}");
    // stacked, under 100 columns, it is the same
    let (mut r, root) = homed("s", (80, 24));
    r.keys("<C-f>");
    assert!(r.float()[0].starts_with(&format!("╭─ Live grep · {root} ─")), "{}", r.float()[0]);
}

#[test]
fn pick_1_a_short_border_cuts_the_root_from_the_left_and_the_mode_always_stays() {
    let (mut r, root) = homed("s", (40, 24));
    r.keys("<C-f>");
    let top = r.float()[0].clone();
    let tail: String = root.chars().skip(root.chars().count() - 13).collect();
    assert!(top.starts_with(&format!("╭─ Live grep · …{tail} ─")), "{top}: ends of {root}");
    assert_eq!(width(&top), usize::from(r.win().unwrap().w));
    // a window as narrow as there is: two chars of the root, the last of it
    let (mut r, root) = homed("s", (20, 24));
    r.keys("<C-f>");
    let top = r.float()[0].clone();
    let last = root.chars().last().unwrap();
    assert!(top.starts_with(&format!("╭─ Live grep · …{last} ")), "{top}");
    // and where not even two chars fit the root is left out, and the name stays
    r.keys("<Esc><C-p>");
    let top = r.float()[0].clone();
    assert!(top.starts_with("╭─ Find files ─") && !top.contains('·'), "{top}");
    assert_eq!(width(&top), 20);
}

#[test]
fn pick_5_the_keys_in_the_bottom_border_include_moving_and_drop_move_then_close_then_open() {
    let full = "↵ open  ^n/^p move  esc close";
    // (columns of the screen, what the border says): beside the preview and stacked, wide to narrow
    for (cols, keys) in
        [(130, full), (60, full), (50, "↵ open  esc close"), (30, "↵ open"), (20, "")]
    {
        let mut r = pick_rig("p5-keys", Skin::Flat, (cols, 24));
        r.keys("<C-f>");
        let last = r.float().pop().unwrap();
        for part in ["↵ open", "^n/^p move", "esc close"] {
            assert_eq!(last.contains(part), keys.contains(part), "{cols} columns: {last}");
        }
        if !keys.is_empty() {
            assert!(last.contains(keys), "in order, two blanks apart: {last}");
        }
        assert_eq!(width(&last), usize::from(r.win().unwrap().w), "{last}");
    }
}
