//! LOOK-1 to LOOK-8: the three looks of sim's screen, from `Sim::update` to the Screen it asks the
//! shell to draw. The palette is the one of `#1e1e1e` and the terminal has truecolor, unless a
//! test says otherwise.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use sim::color::{Syntax, Tones};
use sim::layout::{Geo, Skin, Tick, ruler};
use sim::paint::{Look, Paint, Paints};
use sim::sim::{SimEffect, SimEvent};
use sim_core::core::app::Event;
use sim_core::core::color::{Rgb, palette};
use sim_core::core::vi::Mode;
use sim_core::core::view::Style;
use std::fs;

const BG: Rgb = (0x1e, 0x1e, 0x1e);
const SRC: &str = "fn one() {}\nfn two() {}\nfn three() {}\nfn four() {}\nfn five() {}\nfn six() {}\nfn seven() {}\nfn eight() {}\n";
/// The disk's version: lines 2 and 5 changed (two runs of marks).
const DISK: &str = "fn one() {}\nfn TWO() {}\nfn three() {}\nfn four() {}\nfn FIVE() {}\nfn six() {}\nfn seven() {}\nfn eight() {}\n";

fn tones() -> Tones {
    Tones::new(&palette(BG))
}

/// A Rig on `files` in `skin`, with the palette of `BG` (unless `bg` is false) and truecolor
/// (unless `truecolor` is false), the panel open on the first file.
fn looked(name: &str, skin: Skin, files: &[&str], size: (u16, u16)) -> Rig {
    looked_with(name, skin, files, &[], size)
}

fn looked_with(name: &str, skin: Skin, files: &[&str], args: &[&str], size: (u16, u16)) -> Rig {
    let mut r = rig_env(name, files, args, size, |e| {
        e.skin = skin;
        e.syn = Syntax::new(true);
    });
    r.send(SimEvent::Term(Event::Bg(BG)));
    r
}

/// `a.rs` open (the panel stays), then changed on disk in two places: two marks.
fn marked(name: &str, skin: Skin, size: (u16, u16)) -> Rig {
    let src = format!("a.rs={SRC}");
    let mut r = looked(name, skin, &[&src], size);
    r.keys("<CR>");
    let id = r.sim.bufs[0].id;
    r.send(SimEvent::App(id, disk(DISK.as_bytes())));
    assert_eq!(r.app().buf.marks.len(), 2);
    r
}

/// `a.rs` modified in two places (lines 2 and 5) and then changed on the disk in the same two: two
/// conflicts, marked on mine's lines, the cursor on the first line.
fn conflicted(name: &str, skin: Skin, size: (u16, u16)) -> Rig {
    let src = format!("a.rs={SRC}");
    let mut r = looked(name, skin, &[&src], size);
    r.keys("<CR>jx3jxgg");
    let id = r.sim.bufs[0].id;
    r.send(SimEvent::App(id, disk(DISK.as_bytes())));
    assert!(r.app().diverged());
    assert_eq!(r.app().buf.marks.len(), 2);
    r
}

fn bg_at(p: &Paints, col: u16) -> Option<Rgb> {
    p.iter().find(|(r, _)| r.contains(&col)).and_then(|x| x.1.bg)
}

fn paint_at(p: &Paints, col: u16) -> Paint {
    p.iter().find(|(r, _)| r.contains(&col)).map(|x| x.1).unwrap_or_default()
}

/// The background and the text color of the paint a cell has.
fn ink_at(r: &Rig, y: usize, col: u16) -> (Option<Rgb>, Option<Rgb>) {
    let p = paint_at(&r.shot.paints[y], col);
    (p.bg, p.fg)
}

fn style_at(r: &Rig, y: usize, col: u16) -> Option<Style> {
    r.shot.frame.rows[y].spans.iter().find(|(s, _)| s.contains(&col)).map(|x| x.1)
}

fn text(r: &Rig, y: usize) -> &str {
    &r.shot.frame.rows[y].text
}

/// The screen row whose text contains `what`.
fn row_of(r: &Rig, what: &str) -> usize {
    let y = (0..r.shot.frame.rows.len()).find(|&y| text(r, y).contains(what));
    assert!(y.is_some(), "no row with {what:?}:\n{}", dump(&r.shot.frame));
    y.unwrap()
}

// ---- LOOK-1 ----

#[test]
fn look_1_zl_cycles_calm_flat_depth_and_the_status_line_says_which() {
    let mut r =
        rig_env("l1-cycle", &["a.txt=one\n"], &["a.txt"], (80, 24), |e| e.skin = Skin::Calm);
    r.keys("zl");
    assert_eq!((r.sim.skin, r.status()), (Skin::Flat, "look: flat"));
    r.keys("zl");
    assert_eq!((r.sim.skin, r.status()), (Skin::Depth, "look: depth"));
    r.keys("zl");
    assert_eq!((r.sim.skin, r.status()), (Skin::Calm, "look: calm"));
    r.keys("j");
    assert!(r.status().starts_with("a.txt"), "until the next key: {}", r.status());
    // Each choice is asked to be remembered, in order.
    let asked: Vec<Skin> = r
        .log
        .iter()
        .filter_map(|e| if let SimEffect::Look(s) = e { Some(*s) } else { None })
        .collect();
    assert_eq!(asked, [Skin::Flat, Skin::Depth, Skin::Calm]);
    // The App got both keys: nothing was typed, nothing moved (its parser drops `zl`).
    assert_eq!((r.text(), r.app().buf.cursor), ("one".into(), 0));
}

#[test]
fn look_1_zl_is_only_taken_in_normal_mode_with_nothing_pending_in_the_text() {
    let mut r =
        rig_env("l1-only", &["a.txt=one\ntwo\n"], &["a.txt"], (80, 24), |e| e.skin = Skin::Calm);
    r.keys("2zl");
    assert_eq!(r.sim.skin, Skin::Calm, "a count is pending");
    r.keys("zzl");
    assert_eq!(r.sim.skin, Skin::Calm, "the second z is the App's");
    r.keys("izl<Esc>");
    assert_eq!(r.sim.skin, Skin::Calm);
    assert_eq!(r.text().replace("zl", ""), "one\ntwo", "Insert mode types it: {}", r.text());
    r.keys(":zl\n");
    assert_eq!(r.sim.skin, Skin::Calm, "the command line too");
    r.keys("<C-n>zl");
    assert_eq!(r.sim.skin, Skin::Calm, "the panel's keys are not the text's");
    r.keys("<Esc>zl");
    assert_eq!(r.sim.skin, Skin::Flat);
}

#[test]
fn look_1_the_look_is_remembered_in_the_state_directory_and_depth_is_the_default() {
    let dir = temp_dir("l1-state");
    let state = dir.0.join("sim");
    assert_eq!(sim::io::load_look(&state), Skin::Depth, "no file");
    for s in [Skin::Flat, Skin::Calm, Skin::Depth, Skin::Flat] {
        sim::io::save_look(&state, s).unwrap();
        assert_eq!(sim::io::load_look(&state), s);
    }
    assert_eq!(fs::read_to_string(state.join("look")).unwrap(), "flat\n");
    use std::os::unix::fs::PermissionsExt;
    let mode = |p: &std::path::Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!(
        (mode(&state), mode(&state.join("look"))),
        (0o700, 0o600),
        "private, as the history"
    );
    for junk in ["", "bright\n", "FLAT", "\u{0}"] {
        fs::write(state.join("look"), junk).unwrap();
        assert_eq!(sim::io::load_look(&state), Skin::Depth, "{junk:?}");
    }
    assert_eq!(Skin::default(), Skin::Calm, "what Env starts with: the tests see si's styles");
}

// ---- LOOK-2: flat ----

#[test]
fn look_2_calm_has_no_paints_and_flat_without_a_palette_is_calm() {
    let r = marked("l2-calm", Skin::Calm, (80, 24));
    assert!(r.shot.paints.iter().all(Vec::is_empty));
    let calm = r.shot.frame.clone();
    // Flat with no palette: the same rows, the same styles, nothing painted.
    let src = format!("a.rs={SRC}");
    let mut f = rig_env("l2-nopal", &[&src], &[], (80, 24), |e| {
        (e.skin, e.syn) = (Skin::Flat, Syntax::new(true));
    });
    f.keys("<CR>");
    let id = f.sim.bufs[0].id;
    f.send(SimEvent::App(id, disk(DISK.as_bytes())));
    assert_eq!(f.shot.frame.rows, calm.rows, "no palette: no joins, no bar, no padding");
    assert!(f.shot.paints.iter().all(Vec::is_empty));
}

#[test]
fn look_2_a_changed_line_is_tinted_from_its_first_text_column_to_the_edge_of_the_pane() {
    let r = marked("l2-tint", Skin::Flat, (80, 24));
    let (g, t) = (r.sim.geo(), tones());
    let text0 = g.ex + 5; // the gutter is a symbol, three digits and a blank
    for y in [1usize, 4] {
        let p = &r.shot.paints[y];
        assert_eq!(bg_at(p, text0 - 1), None, "not in the gutter");
        // The tint is 30% of the way to green; the changed word is green itself, on top.
        let tinted = (text0..g.ex + g.ew).filter(|&x| bg_at(p, x) == Some(t.gain)).count();
        assert!(tinted > usize::from(g.ew) - 15, "row {y}: {p:?}");
        assert_eq!(bg_at(p, g.ex + g.ew), None);
        assert_eq!(
            text(&r, y).chars().count(),
            80,
            "the row is written to the edge, so that it is tinted there"
        );
    }
    for y in [0usize, 2, 3, 5, 6, 7] {
        assert!((g.ex..80).all(|x| bg_at(&r.shot.paints[y], x).is_none()), "row {y} has no change");
    }
}
#[test]
fn look_2_changed_words_are_on_green_with_their_text_gray_over_the_tint() {
    let r = marked("l2-words", Skin::Flat, (80, 24));
    let (g, t) = (r.sim.geo(), tones());
    let start = g.ex + 5 + 3; // `fn TWO() {}`: the word starts after `fn `
    assert_eq!(style_at(&r, 1, start), Some(Style::Change), "{:?}", r.shot.frame.rows[1]);
    assert_eq!(ink_at(&r, 1, start), (Some(t.add), Some(t.add_text)), "si's blue is under it");
    assert_eq!(ink_at(&r, 1, start + 2), (Some(t.add), Some(t.add_text)));
    assert_eq!(ink_at(&r, 1, start + 6), (Some(t.gain), None), "the rest of the line is tinted");
    assert_eq!(ink_at(&r, 1, g.ex).0, Some(t.add), "the `~` of the gutter is on green");
    // Calm keeps si's blue: the style and nothing else.
    let c = marked("l2-words-calm", Skin::Calm, (80, 24));
    assert_eq!(style_at(&c, 1, c.sim.geo().ex + 5 + 3), Some(Style::Change));
    assert!(c.shot.paints.iter().all(Vec::is_empty));
}
#[test]
fn look_2_a_gap_of_one_blank_between_changed_words_is_one_patch() {
    let mut r = looked("l2-join", Skin::Flat, &["a.rs=aa bb cc\nxx  yy\n"], (80, 24));
    r.keys("<CR>");
    let id = r.sim.bufs[0].id;
    r.send(SimEvent::App(id, disk(b"AA BB cc\nXX  YY\n")));
    let (g, t) = (r.sim.geo(), tones());
    let bg = |y: usize, i: u16| bg_at(&r.shot.paints[y], g.ex + 5 + i);
    let (a, tint) = (Some(t.add), Some(t.gain));
    assert_eq!([bg(0, 0), bg(0, 1), bg(0, 3), bg(0, 4)], [a; 4], "{:?}", r.shot.paints[0]);
    assert_eq!(bg(0, 2), a, "the blank between AA and BB is one patch with them");
    assert_eq!([bg(0, 5), bg(0, 6)], [tint; 2], "the blank after BB has no changed word after it");
    // Two blanks are not a gap of one.
    assert_eq!([bg(1, 0), bg(1, 1), bg(1, 4), bg(1, 5)], [a; 4], "{:?}", r.shot.paints[1]);
    assert_eq!([bg(1, 2), bg(1, 3)], [tint; 2]);
}
#[test]
fn look_2_the_wrapped_rows_of_a_changed_line_have_a_gutter_bar() {
    let long = "word ".repeat(30);
    let mut r = looked("l2-bar", Skin::Flat, &[&format!("a.rs={long}\nx\n")], (60, 24));
    r.keys("<CR>");
    let id = r.sim.bufs[0].id;
    r.send(SimEvent::App(id, disk(format!("{}WORD \nx\n", "word ".repeat(29)).as_bytes())));
    let (g, t) = (r.sim.geo(), tones());
    // The rows of line 1, up to the row of line 2.
    let next = (1..)
        .find(|&y| {
            text(&r, y).chars().skip(usize::from(g.ex) + 1).take(3).collect::<String>().trim()
                == "2"
        })
        .unwrap();
    assert!(next >= 4, "the line wraps: {}", dump(&r.shot.frame));
    assert!(text(&r, 0).chars().nth(usize::from(g.ex)) == Some('~'));
    for y in 0..next {
        assert_eq!(bg_at(&r.shot.paints[y], g.ex), Some(t.add), "row {y}: the bar is continuous");
    }
    assert_eq!(bg_at(&r.shot.paints[next], g.ex), None, "a row of no change has none");
    // Calm has the symbol of the first row only.
    let mut c = looked("l2-bar-calm", Skin::Calm, &[&format!("a.rs={long}\nx\n")], (60, 24));
    c.keys("<CR>");
    c.send(SimEvent::App(id, disk(format!("{}WORD \nx\n", "word ".repeat(29)).as_bytes())));
    assert!(c.shot.paints.iter().all(Vec::is_empty));
}
#[test]
fn look_2_the_tree_row_of_a_changed_file_is_tinted_across_the_panel() {
    let r = marked("l2-tree", Skin::Flat, (80, 24));
    let (g, t) = (r.sim.geo(), tones());
    // The name is on green (`  a.rs` after the marker), the rest of the row is the tint.
    let p = &r.shot.paints[0];
    assert_eq!(ink_at(&r, 0, 2), (Some(t.add), Some(t.add_text)), "{p:?}");
    assert!((0..2).chain(6..g.pw).all(|x| bg_at(p, x) == Some(t.gain)), "{p:?}");
    assert!(r.shot.paints[1].is_empty() || bg_at(&r.shot.paints[1], 0).is_none());
}

#[test]
fn look_2_what_comes_is_green_what_goes_red_and_the_gutter_says_which() {
    let t = tones();
    // A line the disk added: `+` on green, the whole line on green (it is all new), the rest tinted.
    let mut r = looked("l2-added", Skin::Flat, &[&format!("a.rs={SRC}")], (80, 24));
    r.keys("<CR>");
    let id = r.sim.bufs[0].id;
    r.send(SimEvent::App(
        id,
        disk(SRC.replace("fn two() {}\n", "fn two() {}\nfn extra() {}\n").as_bytes()),
    ));
    let g = r.sim.geo();
    let y = row_of(&r, "fn extra");
    assert_eq!(text(&r, y).chars().nth(usize::from(g.ex)), Some('+'));
    assert_eq!(ink_at(&r, y, g.ex), (Some(t.add), Some(t.add_text)));
    assert_eq!(ink_at(&r, y, g.ex + 5 + 3), (Some(t.add), Some(t.add_text)));
    assert_eq!(bg_at(&r.shot.paints[y], g.ex + g.ew - 1), Some(t.gain));
    // A line the disk removed: `-` on red, and the line after it, where the text went, tinted red
    // (nothing of it is new, and nothing of it is struck: it is not there).
    let mut r = looked("l2-removed", Skin::Flat, &[&format!("a.rs={SRC}")], (80, 24));
    r.keys("<CR>");
    r.send(SimEvent::App(id, disk(SRC.replace("fn three() {}\n", "").as_bytes())));
    let y = (0..24).find(|&y| text(&r, y).chars().nth(usize::from(g.ex)) == Some('-')).unwrap();
    assert!(text(&r, y).contains("fn four"), "{}", text(&r, y));
    assert_eq!(bg_at(&r.shot.paints[y], g.ex), Some(t.del));
    assert!((g.ex + 5..g.ex + g.ew).all(|x| bg_at(&r.shot.paints[y], x) == Some(t.loss)));
    assert!(r.shot.paints[y].iter().all(|(_, p)| p.bg != Some(t.add)), "no green on it");
}

#[test]
fn look_2_a_conflict_is_marked_on_mine_as_a_change_from_disk_is() {
    // The rows of a conflict are mine, marked whole: green and a tint of a gain, as a mark's.
    let r = conflicted("l2-conflict", Skin::Depth, (80, 24));
    let (g, t) = (r.sim.geo(), tones());
    let y = row_of(&r, "n two()");
    assert_eq!(text(&r, y).chars().nth(usize::from(g.ex)), Some('~'));
    assert_eq!(ink_at(&r, y, g.ex).0, Some(t.add), "`~` is on green");
    let words: Vec<u16> =
        (g.ex + 5..g.ex + g.ew).filter(|&x| style_at(&r, y, x) == Some(Style::Change)).collect();
    assert!(
        !words.is_empty()
            && words.iter().all(|&x| ink_at(&r, y, x) == (Some(t.add), Some(t.add_text)))
    );
    assert_eq!(bg_at(&r.shot.paints[y], g.ex + g.ew - 1), Some(t.gain));
}

// ---- LOOK-3: the boxes ----

#[test]
fn look_3_depth_puts_a_box_around_the_panel_and_one_around_the_editor() {
    for size in [(40, 10), (80, 24), (120, 32), (200, 60)] {
        let r = marked("l3-geo", Skin::Depth, size);
        let (w, h) = size;
        let g = r.sim.geo();
        assert!(g.boxed);
        assert_eq!((g.pw, g.ew), (r.sim.widths().0, r.sim.widths().1));
        assert_eq!(g.ew + g.pw + 4, w, "{size:?}: two boxes, their borders");
        assert_eq!((g.y0, g.rows), (1, h - 3));
        assert_eq!(r.app().size, (g.ew, h - 2), "the App's last row is the status line");
        let rows = &r.shot.frame.rows;
        assert_eq!(rows.len(), usize::from(h));
        for (y, row) in rows.iter().enumerate().take(usize::from(h) - 1) {
            assert_eq!(width(&row.text), usize::from(w), "{size:?} row {y}: {:?}", row.text);
        }
        let chars: Vec<Vec<char>> = rows.iter().map(|r| r.text.chars().collect()).collect();
        let pw = usize::from(g.pw);
        assert_eq!(
            [chars[0][0], chars[0][pw + 1], chars[0][pw + 2], chars[0][usize::from(w) - 1]],
            ['╭', '╮', '╭', '╮']
        );
        let last = usize::from(h) - 2;
        assert_eq!(
            [
                chars[last][0],
                chars[last][pw + 1],
                chars[last][pw + 2],
                chars[last][usize::from(w) - 1]
            ],
            ['╰', '╯', '╰', '╯']
        );
        for (y, row) in chars.iter().enumerate().take(last).skip(1) {
            assert_eq!([row[0], row[pw + 1], row[pw + 2]], ['│'; 3], "row {y}");
            assert!("│┃".contains(row[usize::from(w) - 1]), "row {y}");
        }
        assert!(
            chars[usize::from(h) - 1].iter().collect::<String>().starts_with("a.rs"),
            "si's status line, whole"
        );
    }
}

#[test]
fn look_3_the_top_border_carries_the_panel_s_title_and_the_editor_s_badge() {
    let mut r = marked("l3-title", Skin::Depth, (80, 24));
    let top = text(&r, 0).to_string();
    // UI-16: no numbers, and the editor's border does not repeat what its tab says
    assert!(top.starts_with("╭─files─"), "{top}");
    assert!(top.contains("─ 2 changes on disk ─"), "{top}");
    assert!(!top.contains('¹') && !top.contains('²') && !top.contains("a.rs"), "{top}");
    let g = r.sim.geo();
    // The badge, blanks included, is on green with its text gray (with a palette, there is no
    // style of change under it: that is for a screen without one).
    let t = tones();
    let col = top[..top.find(" 2 changes on disk ").unwrap()].chars().count() as u16;
    assert!(
        (col..col + 19).all(|x| ink_at(&r, 0, x) == (Some(t.add), Some(t.add_text))),
        "{:?}",
        r.shot.paints[0]
    );
    assert_ne!(bg_at(&r.shot.paints[0], col - 1), Some(t.add));
    assert_ne!(bg_at(&r.shot.paints[0], col + 19), Some(t.add));
    assert!(r.shot.frame.rows[0].spans.is_empty());
    // The editor has the focus: its lines are the secondary gray, the panel's the lines' color.
    assert_eq!(paint_at(&r.shot.paints[0], 0).fg, Some(t.line));
    assert_eq!(paint_at(&r.shot.paints[0], g.ex - 1).fg, Some(t.focus));
    assert_eq!(paint_at(&r.shot.paints[5], g.ex - 1).fg, Some(t.focus));
    assert_eq!(paint_at(&r.shot.paints[5], 0).fg, Some(t.line));
    // The panel's title is bold, and dimmed while the editor has the focus.
    assert_eq!(paint_at(&r.shot.paints[0], 2).fg, Some(t.dim));
    assert!(paint_at(&r.shot.paints[0], 2).bold);
    // The panel has the focus: the other way round.
    r.keys("<C-n>");
    assert_eq!(paint_at(&r.shot.paints[0], 0).fg, Some(t.focus));
    assert_eq!(paint_at(&r.shot.paints[0], g.ex - 1).fg, Some(t.line));
    assert_eq!(paint_at(&r.shot.paints[0], 2).fg, None, "the focused box's title is not dimmed");
    // No badge when nothing changed, one run of marks is one change, and proposals are counted.
    let src = format!("a.rs={SRC}");
    let mut q = looked("l3-badge", Skin::Depth, &[&src], (80, 24));
    q.keys("<CR>");
    assert!(
        !text(&q, 0).contains("change") && !text(&q, 0).contains("conflict"),
        "{}",
        text(&q, 0)
    );
    let id = q.sim.bufs[0].id;
    q.send(SimEvent::App(
        id,
        disk(DISK.replace("FIVE", "five").replace("three", "THREE").as_bytes()),
    ));
    assert!(text(&q, 0).contains("─ 1 change on disk ─"), "{}", text(&q, 0));
    let r = conflicted("l3-badge-conflict", Skin::Depth, (100, 24));
    assert!(text(&r, 0).contains("─ 1 conflict with the disk ─"), "{}", text(&r, 0));
}

#[test]
fn ui_16_the_editor_s_border_has_no_title_whatever_the_path_and_the_panel_s_is_files() {
    let path = "deep/dir/with/a/long/path/to/some/other/place/name.rs";
    let src = format!("{path}=fn x() {{}}\n");
    let r = looked_with("l3-short", Skin::Depth, &[&src], &[path], (40, 12));
    assert_eq!(r.open_files(), [path]);
    let top = text(&r, 0);
    assert_eq!(width(top), 40);
    assert!(top.chars().all(|c| "╭╮─".contains(c)), "a plain rule: {top}");
    // with the panel open: its `files` and nothing else
    let mut r = looked_with("l3-files", Skin::Depth, &[&src], &[path], (80, 12));
    r.keys("<C-n>");
    let top = text(&r, 0);
    assert!(top.starts_with("╭─files─"), "{top}");
    let editor = &top[top.find("╮╭").unwrap() + 3..];
    assert!(editor.chars().all(|c| "╭╮─".contains(c)), "{top}");
    // a conflict's badge is still there, and no title is made of the path or its hostile chars
    let r = conflicted("l3-badge-only", Skin::Depth, (100, 24));
    assert!(text(&r, 0).contains("─ 1 conflict with the disk ─"), "{}", text(&r, 0));
}

#[test]
fn look_3_the_border_has_a_badge_while_the_disk_conflicts_and_its_bottom_is_plain() {
    let r = conflicted("l3-conflict", Skin::Depth, (100, 24));
    let top = text(&r, 0).to_string();
    assert!(
        top.contains("─ 2 changes on disk ─") && top.contains("─ 1 conflict with the disk ─"),
        "{top}"
    );
    assert!(text(&r, 22).starts_with("╰──"), "{}", text(&r, 22));
    assert!(text(&r, 23).contains("conflicts with the disk"), "si's status line says it too");
}

#[test]
fn look_3_a_screen_too_small_for_the_boxes_is_flat() {
    for size in [(39, 24), (80, 9), (20, 5)] {
        let r = marked("l3-small", Skin::Depth, size);
        let g = r.sim.geo();
        assert!(!g.boxed, "{size:?}");
        assert_eq!(r.app().size.1, size.1, "{size:?}: the App is given the whole height");
        assert!(!text(&r, 0).contains('╭'));
        assert_eq!(Geo::new(size, Skin::Flat, true).ew, g.ew, "{size:?}: flat's geometry");
        assert!(r.shot.paints.iter().any(|p| !p.is_empty()), "{size:?}: flat's tint");
    }
}

#[test]
fn look_3_the_mouse_lands_where_things_are_drawn_in_the_boxes() {
    let mut r = marked("l3-mouse", Skin::Depth, (80, 24));
    let g = r.sim.geo();
    // A click on the text of line 3 (`fn three() {}`): the second `e` of `three`.
    let y = row_of(&r, "fn three") as u16;
    let x = g.ex + 5 + 6;
    r.click(x, y);
    assert_eq!((r.app().buf.line_of(r.app().buf.cursor), r.app().buf.col()), (2, 6));
    assert_eq!(r.shot.frame.cursor, (x, y), "the cursor is drawn where the text is");
    // Clicks on the borders and on the status line do nothing.
    for (cx, cy) in [(g.ex - 1, y), (g.ex + g.ew, y), (g.ex + 5, 0), (g.ex + 5, 22), (g.ex + 5, 23)]
    {
        r.click(cx, cy);
        assert_eq!(
            (r.app().buf.line_of(r.app().buf.cursor), r.app().buf.col()),
            (2, 6),
            "click at {cx},{cy}"
        );
    }
    // The wheel on a border does nothing either; over the text it scrolls.
    let top = r.app().top;
    r.wheel(false, g.ex - 1, y);
    assert_eq!(r.app().top, top);
    // The panel: its rows start under the top border, and its borders are no rows.
    let mut r = looked("l3-panel", Skin::Depth, &["a.rs=A\n", "b.rs=B\n"], (80, 24));
    for (cx, cy) in [(g.px + 2, 0), (g.px + 2, g.y0 + g.rows), (0, g.y0 + 1), (g.pw + 1, g.y0 + 1)]
    {
        r.click(cx, cy);
        assert!(r.open_files().is_empty(), "click at {cx},{cy}");
    }
    r.click(g.px + 2, g.y0 + 1);
    assert_eq!(r.open_files(), ["b.rs"]);
}

#[test]
fn look_3_every_look_at_every_size_draws_a_whole_screen_within_its_columns() {
    let mut painter = sim::paint::Painter::new(Vec::<u8>::new());
    for skin in [Skin::Calm, Skin::Flat, Skin::Depth] {
        // (the edges of the boxes' limits, 40 columns and 10 rows, and of nothing at all)
        for (w, h) in [1, 3, 8, 20, 39, 40, 41, 62]
            .into_iter()
            .flat_map(|w| [2, 3, 9, 10, 11, 26].map(|h| (w, h)))
        {
            let src = format!("a.rs={SRC}");
            let mut r = looked("l3-sizes", skin, &[&src, "日本語.txt=x\n"], (w, h));
            r.keys("<CR>jx3jx");
            let id = r.sim.bufs[0].id;
            r.send(SimEvent::App(id, disk(DISK.as_bytes())));
            for k in ["gg]c", "]c", "<Esc>", "zl", "<C-n>", "/a", "<Esc><Esc>", ":s/f/g/\n"] {
                r.keys(k);
                let (f, g) = (&r.shot.frame, r.sim.geo());
                let at = format!("{skin:?} {w}x{h} after {k}");
                assert_eq!(
                    (f.width, usize::from(f.height), f.rows.len()),
                    (w, usize::from(h), usize::from(h)),
                    "{at}"
                );
                assert_eq!(r.shot.paints.len(), r.shot.looks.len(), "{at}");
                assert_eq!(r.app().size, (g.ew, g.rows + 1), "{at}");
                for row in &f.rows {
                    assert!(width(&row.text) <= usize::from(w), "{at}: {:?}", row.text);
                }
                painter.draw(r.shot.clone()).unwrap();
                painter.out.clear();
            }
        }
    }
}

// ---- LOOK-4: the overview ruler ----

#[test]
fn look_4_a_row_of_the_border_stands_for_a_range_of_lines() {
    use Tick::{Gain, Loss, Plain as P, Shown};
    // 100 lines on 10 rows: row r has the lines r*10..r*10+10.
    let changes = [(40..43, Gain), (99..100, Loss)].into_iter();
    let t = ruler(10, 100, changes, Some((30, 50)));
    assert_eq!(t, [P, P, P, Shown, Gain, Shown, P, P, P, Loss]);
    // A hunk marks every row from its first line to its last.
    assert_eq!(
        ruler(10, 100, std::iter::once((20..45, Gain)), None),
        [P, P, Gain, Gain, Gain, P, P, P, P, P]
    );
    // Where both are in one row, the change wins over the screen, and green over red.
    let both = [(41..42, Loss), (44..45, Gain), (45..46, Loss)].into_iter();
    assert_eq!(ruler(10, 100, both, Some((0, 99)))[4], Gain);
    assert_eq!(ruler(10, 100, [(41..42, Loss)].into_iter(), Some((0, 99)))[4], Loss);
    // A short file: each line is on the row it maps to, and the rows between are plain.
    let t = ruler(21, 3, std::iter::once((1..2, Gain)), Some((0, 0)));
    assert_eq!((t[0], t[7], t[14]), (Shown, Gain, P));
    assert_eq!(t.iter().filter(|x| **x != P).count(), 2);
    // Nothing to show: no file, or no rows.
    assert_eq!(ruler(0, 5, std::iter::empty(), None), []);
}
#[test]
fn look_4_the_right_border_of_the_editor_shows_the_changes_and_the_screen() {
    let src: String = (0..100).map(|i| format!("line {i}\n")).collect();
    let src = format!("a.rs={src}");
    let mut r = looked("l4-ruler", Skin::Depth, &[&src], (80, 24));
    r.keys("<CR>");
    let id = r.sim.bufs[0].id;
    // Two lines changed (green) and one removed (red, on its own row).
    let new: String = (0..100)
        .filter(|&i| i != 80)
        .map(|i| if i == 60 || i == 61 { format!("LINE {i}\n") } else { format!("line {i}\n") })
        .collect();
    r.send(SimEvent::App(id, disk(new.as_bytes())));
    let (g, t) = (r.sim.geo(), tones());
    let x = g.ex + g.ew;
    // 100 lines on 21 rows: line 60 is on row 60 * 21 / 100 = 12, line 80 on row 16; the screen
    // shows lines 1 to ~22.
    let at = |r: &Rig, y: usize| {
        (text(r, y).chars().nth(usize::from(x)).unwrap(), paint_at(&r.shot.paints[y], x).fg)
    };
    assert_eq!(at(&r, 1 + 12), ('┃', Some(t.add)));
    assert_eq!(at(&r, 1 + 16), ('┃', Some(t.del)), "a deletion point alone is red");
    assert_eq!(at(&r, 1), ('┃', Some(t.line)), "the lines on screen");
    assert_eq!(at(&r, 1 + 8), ('│', Some(t.focus)), "the plain border");
    // Jump to the change: the screen is there, and the changed rows keep their colors.
    r.keys("60G");
    assert_eq!(at(&r, 1 + 12), ('┃', Some(t.add)));
    assert_eq!(at(&r, 1 + 16), ('┃', Some(t.del)));
    assert_eq!(at(&r, 1 + 11), ('┃', Some(t.line)));
    assert_eq!(at(&r, 1 + 2), ('│', Some(t.focus)));
}
// ---- LOOK-6: the fog ----

#[test]
fn look_6_rows_of_a_change_never_fade_and_context_keeps_its_syntax_colors() {
    let r = conflicted("l6-other", Skin::Depth, (80, 24));
    let g = r.sim.geo();
    let other = row_of(&r, "n five");
    assert!(r.shot.paints[other].iter().all(|(_, p)| p.fade == 0), "{:?}", r.shot.paints[other]);
    let y = row_of(&r, "fn seven");
    assert!(r.shot.paints[y].iter().any(|(_, p)| p.fade > 0));
    assert!(
        r.shot.looks[y].iter().any(|(s, l)| *l == Look::Kw && s.start == g.ex + 5),
        "the keyword `fn` keeps its look: {:?}",
        r.shot.looks[y]
    );
}

#[test]
fn look_6_the_text_fades_by_its_distance_from_the_cursor_row_and_changes_and_the_status_line_do_not()
 {
    let src: String = (0..60).map(|i| format!("let x{i} = {i};\n")).collect();
    let mut r = looked("l6-grad", Skin::Depth, &[&format!("a.rs={src}")], (80, 24));
    r.keys("<CR>30G");
    let g = r.sim.geo();
    let edge = g.ex..g.ex + g.ew;
    let fades = |r: &Rig| -> Vec<u8> {
        (1..=usize::from(g.rows)).map(|y| fade_over(r, y, edge.clone())).collect()
    };
    let crow = usize::from(r.shot.frame.cursor.1 - g.y0);
    for (i, f) in fades(&r).into_iter().enumerate() {
        assert_eq!(f, ramp(i.abs_diff(crow), 3.0, 24.0, 0.45), "row {i}, the cursor on {crow}");
        // the line numbers are faded with the text
        assert_eq!(paint_at(&r.shot.paints[i + 1], g.ex + 1).fade, f);
    }
    assert!(fades(&r).iter().any(|&f| f >= 15), "{:?}", fades(&r));
    // The borders, the ruler and the status line are not.
    assert!(r.shot.paints[0].iter().all(|(_, p)| p.fade == 0));
    assert!(r.shot.paints[usize::from(g.rows) + 1].iter().all(|(_, p)| p.fade == 0));
    assert!(r.shot.paints[23].is_empty());
    assert!(
        r.shot.paints[1].iter().filter(|(rg, _)| rg.start >= g.ex + g.ew).all(|(_, p)| p.fade == 0)
    );
    // The distance is from the cursor: at the top of the file the fade grows down the screen.
    r.keys("gg");
    let top = fades(&r);
    assert_eq!(top, (0..usize::from(g.rows)).map(|i| ramp(i, 3.0, 24.0, 0.45)).collect::<Vec<_>>());
    // On the command line the cursor is in the status row, and the fade stays where it was.
    r.keys(":");
    assert_eq!(r.app().vi.mode, Mode::Cmd);
    assert_eq!(fades(&r), top);
    r.keys("<Esc>");
    // Flat and calm have no fog.
    for skin in [Skin::Flat, Skin::Calm] {
        let mut f = looked("l6-grad-flat", skin, &[&format!("a.rs={src}")], (80, 24));
        f.keys("<CR>30G");
        assert!(f.shot.paints.iter().flatten().all(|(_, p)| p.fade == 0), "{skin:?}");
    }
}

#[test]
fn look_6_a_row_of_a_change_does_not_fade_however_far_it_is_from_the_cursor() {
    let src: String = (0..40).map(|i| format!("let x{i} = {i};\n")).collect();
    let mut r = looked("l6-change", Skin::Depth, &[&format!("a.rs={src}")], (80, 24));
    r.keys("<CR>");
    let id = r.sim.bufs[0].id;
    let new = src.replace("let x15 = 15;", "let x15 = 99;");
    r.send(SimEvent::App(id, disk(new.as_bytes())));
    r.keys("gg");
    let g = r.sim.geo();
    let y = row_of(&r, "x15 = 99");
    assert_eq!(y, 1 + 15);
    assert!(r.shot.paints[y].iter().all(|(_, p)| p.fade == 0), "{:?}", r.shot.paints[y]);
    let (above, below) =
        (fade_over(&r, y - 1, g.ex..g.ex + g.ew), fade_over(&r, y + 1, g.ex..g.ex + g.ew));
    assert_eq!((above, below), (ramp(14, 3.0, 24.0, 0.45), ramp(16, 3.0, 24.0, 0.45)));
    assert!(above > 0);
}

#[test]
fn look_6_the_picker_floats_over_the_boxes_of_depth_and_the_rows_stay_whole() {
    let files = [
        "a.txt=alpha needle\nbeta\n",
        "sub/b.txt=gamma\nneedle again and Needle\n",
        "c.txt=nothing here\n",
    ];
    let mut r = looked("l6-live", Skin::Depth, &files, (100, 24));
    let before: Vec<String> = (0..=22).map(|y| text(&r, y).to_string()).collect();
    r.keys("<C-f>needle");
    let w = r.win().unwrap();
    // Every row of the screen is as wide as the screen: the window replaces cells, it adds none.
    for y in 0..=22 {
        assert_eq!(width(text(&r, y)), 100, "row {y}: {}", text(&r, y));
    }
    // The window is whole, with its shadow shifted by one cell, and the boxes beside it are as they were.
    assert!(text(&r, usize::from(w.y)).chars().nth(usize::from(w.x)) == Some('╭'));
    let boxes = |y: usize| -> String { text(&r, y).chars().take(usize::from(w.x)).collect() };
    let inside = usize::from(w.y)..usize::from(w.y + w.h);
    for (y, was) in before.iter().enumerate() {
        if inside.contains(&y) {
            assert_eq!(
                boxes(y),
                was.chars().take(usize::from(w.x)).collect::<String>(),
                "row {y} beside it"
            );
        } else {
            assert_eq!(text(&r, y), was, "row {y} above and below the window");
        }
    }
    // The panel's rows are not faded by a search of contents: it is not the panel's search.
    let g = r.sim.geo();
    let cols = g.px..g.px + g.pw;
    assert!(fade_over(&r, 1, cols.clone()) < 60, "only the tree's own gradient");
}

// ---- LOOK-7: the panel recedes, and a search fades it ----

/// The tree's order: `a.txt`, `alpha.rs`, `beta.rs`, `delta.rs`, `epsilon.rs`, `eta.rs`, `gamma.rs`,
/// `theta.rs`, `zeta.rs`.
const FILES: [&str; 9] = [
    "alpha.rs",
    "beta.rs",
    "gamma.rs",
    "delta.rs",
    "epsilon.rs",
    "eta.rs",
    "theta.rs",
    "zeta.rs",
    "a.txt=one\n",
];

/// The fade in percent, from `from` rows away over `span` rows up to `top`: the SPEC's formulas.
fn ramp(d: usize, from: f64, span: f64, top: f64) -> u8 {
    (((d as f64 - from) / span * top).clamp(0.0, top) * 100.0).round() as u8
}

/// The fade of the first and the last column of a column range of a row, which must agree.
fn fade_over(r: &Rig, y: usize, cols: std::ops::Range<u16>) -> u8 {
    let (a, b) =
        (paint_at(&r.shot.paints[y], cols.start), paint_at(&r.shot.paints[y], cols.end - 1));
    assert_eq!(a.fade, b.fade, "row {y}: {:?}", r.shot.paints[y]);
    a.fade
}

#[test]
fn look_7_the_tree_is_not_in_fog_whatever_its_distance_from_the_open_file_or_the_cursor() {
    let panel_fade = |r: &Rig| -> Vec<u8> {
        let g = r.sim.geo();
        (1..=9)
            .flat_map(|y| (g.px..g.px + g.pw).map(move |x| (y, x)))
            .map(|(y, x)| paint_at(&r.shot.paints[y], x).fade)
            .collect()
    };
    let mut r = looked("l7-near", Skin::Depth, &FILES, (80, 24));
    assert!(r.sim.panel.focus);
    // The tree has the focus, the cursor on its first row and then on its eighth: no row fades.
    assert!(panel_fade(&r).iter().all(|&f| f == 0), "{:?}", panel_fade(&r));
    r.keys("jjjjjjj");
    assert!(panel_fade(&r).iter().all(|&f| f == 0));
    // `eta.rs` is row 5 and is open, the focus in the text: the far rows are as clear as the near ones.
    r.keys("gg<CR>"); // a.txt
    r.keys("<C-n>jjjjj<CR>");
    assert_eq!(r.open_files(), ["a.txt", "eta.rs"]);
    assert!(!r.sim.panel.focus);
    assert!(panel_fade(&r).iter().all(|&f| f == 0), "{:?}", panel_fade(&r));
    // A file in a closed directory, and no file at all: the same.
    let mut files: Vec<String> = (1..=8).map(|i| format!("m{i}.rs")).collect();
    files.push("d/f.rs=x\n".into());
    let names: Vec<&str> = files.iter().map(String::as_str).collect();
    let mut c = looked_with("l7-closed", Skin::Depth, &names, &["d/f.rs"], (80, 24));
    c.keys("<C-n>jjjj<Esc>");
    assert!(panel_fade(&c).iter().all(|&f| f == 0));
    let mut n = looked("l7-nofile", Skin::Depth, &names, (80, 24));
    n.keys("jjj<Esc>");
    assert!(panel_fade(&n).iter().all(|&f| f == 0));
    // The editor's own fog is as it was (LOOK-6): the rows far from the cursor fade there.
    let text: String = (0..60).map(|i| format!("line {i}\n")).collect();
    let mut f = looked("l7-editor", Skin::Depth, &[&format!("long.txt={text}")], (80, 30));
    f.keys("<CR>");
    let g = f.sim.geo();
    let far = f.shot.paints[usize::from(g.y0) + 25].iter().any(|(_, p)| p.fade > 0);
    assert!(far, "{:?}", f.shot.paints[usize::from(g.y0) + 25]);
    // Flat and calm have none either.
    for skin in [Skin::Flat, Skin::Calm] {
        let mut f = looked("l7-flat", skin, &FILES, (80, 24));
        f.keys("jjjjj<CR>");
        assert!(f.shot.paints.iter().flatten().all(|(_, p)| p.fade == 0), "{skin:?}");
    }
}

#[test]
fn look_7_a_search_fades_the_rows_by_their_distance_from_the_cursor_except_the_matches() {
    let mut r = looked("l7-find", Skin::Depth, &FILES, (80, 24));
    r.keys("/eta");
    let g = r.sim.geo();
    let cur = r.sim.panel.cur;
    let want = [
        ("a.txt", false),
        ("alpha.rs", false),
        ("beta.rs", true),
        ("delta.rs", true),
        ("epsilon.rs", false),
        ("eta.rs", true),
        ("gamma.rs", false),
        ("theta.rs", true),
        ("zeta.rs", true),
    ];
    for (i, (name, hit)) in want.into_iter().enumerate() {
        let y = 1 + i;
        assert!(text(&r, y).contains(name), "row {y}: {}", text(&r, y));
        let d = i.abs_diff(cur) as f64;
        let f = (0.12 + d / 16.0 * 0.6).min(0.7);
        let want = if hit { 0 } else { (f * 100.0).round() as u8 };
        let got: Vec<u8> =
            (g.px..g.px + g.pw).map(|x| paint_at(&r.shot.paints[y], x).fade).collect();
        assert!(got.iter().all(|&x| x == want), "{name} (d {d}): want {want}, got {got:?}");
    }
    // A query that lights nothing dims nothing.
    r.keys("<BS><BS><BS>");
    assert!(r.shot.paints.iter().flatten().all(|(_, p)| p.fade == 0), "{:?}", r.shot.paints);
}

// ---- LOOK-8: no palette, no truecolor ----

#[test]
fn look_8_without_a_palette_or_truecolor_depth_is_its_boxes_in_the_default_colors() {
    for (name, bg, truecolor) in [("nopal", false, true), ("notrue", true, false)] {
        let src = format!("a.rs={SRC}");
        let mut r = rig_env(name, &[&src], &[], (80, 24), |e| {
            (e.skin, e.syn) = (Skin::Depth, Syntax::new(truecolor));
        });
        if bg {
            r.send(SimEvent::Term(Event::Bg(BG)));
        }
        r.keys("<CR>");
        let id = r.sim.bufs[0].id;
        r.send(SimEvent::App(id, disk(DISK.as_bytes())));
        assert!(
            text(&r, 0).starts_with("╭─files─") && text(&r, 0).contains("changes on disk"),
            "{name}: {}",
            text(&r, 0)
        );
        let bare = |p: &Paint| p.fg.is_none() && p.bg.is_none() && p.fade == 0;
        assert!(
            r.shot.paints.iter().flatten().all(|(_, p)| bare(p)),
            "{name}: no tint, fog, shading or border colors (bold only)"
        );
        let top = text(&r, 0);
        let col = top[..top.find(" 2 changes on disk ").unwrap()].chars().count() as u16;
        assert!(
            paint_at(&r.shot.paints[0], col + 1).bold,
            "{name}: the badge is bold, with no color"
        );
    }
    // Flat is calm.
    let make = |skin| {
        let mut r = rig_env("flat-calm", &["a.txt=one\ntwo\n"], &[], (80, 24), |e| {
            (e.skin, e.syn) = (skin, Syntax::new(false))
        });
        r.send(SimEvent::Term(Event::Bg(BG)));
        r.keys("<CR>");
        let id = r.sim.bufs[0].id;
        r.send(SimEvent::App(id, disk(b"ONE\ntwo\n")));
        r
    };
    let (flat, calm) = (make(Skin::Flat), make(Skin::Calm));
    assert_eq!(flat.shot.frame, calm.shot.frame);
    assert!(flat.shot.paints.iter().all(Vec::is_empty));
}

// ---- goldens ----

#[test]
fn look_2_golden_flat() {
    let r = marked("l2-golden", Skin::Flat, (64, 14));
    golden("look_2_flat", &dump_look(&r.shot, &tones()));
}

#[test]
fn look_3_golden_depth() {
    let r = marked("l3-golden", Skin::Depth, (64, 14));
    golden("look_3_depth", &dump_look(&r.shot, &tones()));
}

#[test]
fn look_7_golden_find() {
    let mut r = looked("l7-golden", Skin::Depth, &FILES, (64, 14));
    r.keys("/eta");
    golden("look_7_find", &dump_look(&r.shot, &tones()));
}
