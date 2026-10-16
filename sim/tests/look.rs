//! LOOK-1 to LOOK-8: the three looks of sim's screen, from `Sim::update` to the Screen it asks the
//! shell to draw. The palette is the one of `#1e1e1e` and the terminal has truecolor, unless a
//! test says otherwise.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use si::core::app::Event;
use si::core::color::{Rgb, palette};
use si::core::vi::Mode;
use si::core::view::Style;
use sim::color::{Syntax, Tones};
use sim::layout::{Geo, Skin, Tick, ruler};
use sim::paint::{Look, Paint, Paints};
use sim::sim::{SimEffect, SimEvent};
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
/// hunks (proposals), the cursor on the first line.
fn conflicted(name: &str, skin: Skin, size: (u16, u16)) -> Rig {
    let src = format!("a.rs={SRC}");
    let mut r = looked(name, skin, &[&src], size);
    r.keys("<CR>jx3jx");
    let id = r.sim.bufs[0].id;
    r.send(SimEvent::App(id, disk(DISK.as_bytes())));
    assert_eq!(r.app().buf.hunks.len(), 2);
    r.keys("gg");
    r
}

/// `conflicted`, and REVIEW on the first hunk with `]c`.
fn reviewing(name: &str, skin: Skin, size: (u16, u16)) -> Rig {
    let mut r = conflicted(name, skin, size);
    r.keys("]c");
    assert_eq!(r.app().vi.mode, Mode::Review);
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
fn look_2_a_proposal_is_a_neutral_row_with_the_words_it_would_replace_on_red() {
    // Outside REVIEW the rows of a hunk are mine, with the words a proposal would replace marked.
    let r = conflicted("l2-hunk", Skin::Flat, (80, 24));
    let (g, t) = (r.sim.geo(), tones());
    assert_eq!(r.app().vi.mode, Mode::Normal);
    let y = row_of(&r, "n two()");
    assert_eq!(text(&r, y).chars().nth(usize::from(g.ex)), Some('~'));
    assert_eq!(bg_at(&r.shot.paints[y], g.ex), Some(t.add), "`~` is on green, as for a mark");
    let words: Vec<u16> =
        (g.ex + 5..g.ex + g.ew).filter(|&x| style_at(&r, y, x) == Some(Style::Change)).collect();
    assert!(
        !words.is_empty()
            && words.iter().all(|&x| ink_at(&r, y, x) == (Some(t.del), Some(t.del_text))),
        "{:?}",
        r.shot.paints[y]
    );
    let rest = g.ex + g.ew - 1;
    assert_eq!(bg_at(&r.shot.paints[y], rest), Some(t.hunk), "the row is neutral: it holds both");
    // The other hunk is the same: neutral at the end of its row.
    let o = row_of(&r, "n five");
    assert_eq!(bg_at(&r.shot.paints[o], g.ex + g.ew - 1), Some(t.hunk));
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
fn look_3_the_borders_carry_the_numbers_the_titles_and_the_badge() {
    let mut r = marked("l3-title", Skin::Depth, (80, 24));
    let top = text(&r, 0).to_string();
    assert!(top.starts_with("╭─¹files─"), "{top}");
    assert!(top.contains("²a.rs─ 2 changes on disk ─"), "{top}");
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
    // The number is the secondary gray in the focused box and dimmed in the other; the title is bold.
    assert_eq!(paint_at(&r.shot.paints[0], g.ex + 1).fg, Some(t.focus));
    assert_eq!(paint_at(&r.shot.paints[0], 2).fg, Some(t.dim));
    assert!(
        r.shot.looks[0].iter().any(|(s, l)| *l == Look::Bold && s.start == g.ex + 2),
        "{:?}",
        r.shot.looks[0]
    );
    assert!(r.shot.looks[0].iter().any(|(s, l)| *l == Look::Bold && s.start == 3));
    // The panel has the focus: the other way round.
    r.keys("<C-n>");
    assert_eq!(paint_at(&r.shot.paints[0], 0).fg, Some(t.focus));
    assert_eq!(paint_at(&r.shot.paints[0], g.ex - 1).fg, Some(t.line));
    // No badge when nothing changed, one run of marks is one change, and proposals are counted.
    let src = format!("a.rs={SRC}");
    let mut q = looked("l3-badge", Skin::Depth, &[&src], (80, 24));
    q.keys("<CR>");
    assert!(
        !text(&q, 0).contains("change") && !text(&q, 0).contains("proposal"),
        "{}",
        text(&q, 0)
    );
    let id = q.sim.bufs[0].id;
    q.send(SimEvent::App(
        id,
        disk(DISK.replace("FIVE", "five").replace("three", "THREE").as_bytes()),
    ));
    assert!(text(&q, 0).contains("─ 1 change on disk ─"), "{}", text(&q, 0));
    let r = reviewing("l3-badge-hunks", Skin::Depth, (80, 24));
    assert!(text(&r, 0).contains("─ 2 proposals ─"), "{}", text(&r, 0));
}

#[test]
fn look_3_the_title_is_cut_to_what_the_border_has_left_by_si_s_rule() {
    let path = "deep/dir/with/a/long/path/to/some/other/place/name.rs";
    let src = format!("{path}=fn x() {{}}\n");
    let r = looked_with("l3-short", Skin::Depth, &[&src], &[path], (40, 12));
    assert_eq!(r.open_files(), [path]);
    let top = text(&r, 0);
    assert_eq!(width(top), 40);
    assert!(top.starts_with("╭─²…/") && top.contains("name.rs─"), "{top}");
    assert!(!top.contains("deep"), "{top}");
}

#[test]
fn look_3_review_shows_which_hunk_and_the_keys_in_the_borders() {
    let mut r = reviewing("l3-review", Skin::Depth, (100, 24));
    let (g, t) = (r.sim.geo(), tones());
    let top = text(&r, 0).to_string();
    assert!(top.trim_end().ends_with("─ 1/2 ─╮"), "{top}");
    let bottom = text(&r, 22).to_string();
    assert!(
        bottom.contains("╰─ REVIEW  y accept  n reject  Y all  N none  Esc back ─"),
        "{bottom}"
    );
    // Letters are primary and bold, words secondary: the words by style, the letters by none.
    let col = |s: &str| bottom.find(s).map(|i| bottom[..i].chars().count() as u16).unwrap();
    let (y, accept) = (col(" y accept"), col(" accept"));
    assert_eq!(style_at(&r, 22, y + 1), None);
    assert!(
        r.shot.looks[22].iter().any(|(s, l)| *l == Look::Bold && s.contains(&(y + 1))),
        "{:?}",
        r.shot.looks[22]
    );
    assert_eq!(style_at(&r, 22, accept + 1), Some(Style::Secondary));
    assert_eq!(paint_at(&r.shot.paints[22], g.ex).fg, Some(t.focus));
    // i is the hunk REVIEW is on.
    r.keys("]c");
    assert!(text(&r, 0).trim_end().ends_with("─ 2/2 ─╮"), "{}", text(&r, 0));
    // Normal mode: the bottom border is plain, si's status line below says the rest.
    r.keys("<Esc>");
    assert!(
        !text(&r, 22).contains("REVIEW") && text(&r, 22).starts_with("╰──"),
        "{}",
        text(&r, 22)
    );
    assert!(text(&r, 23).contains("NORMAL"));
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
                assert_eq!(
                    r.app().size,
                    (g.ew, g.rows + 1 - 2 * u16::from(r.app().vi.mode == Mode::Review && g.boxed)),
                    "{at}"
                );
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
// ---- LOOK-5, LOOK-6: the card and the fog ----

#[test]
fn look_5_review_raises_the_hunk_being_decided_as_a_card() {
    let r = reviewing("l5-card", Skin::Depth, (80, 24));
    let (g, t) = (r.sim.geo(), tones());
    let y = usize::from(r.shot.frame.cursor.1);
    let text0 = g.ex + 5;
    assert!(text(&r, y).contains("TWO"), "the cursor is on the card's first row: {}", text(&r, y));
    // The card's rows: a neutral raised surface over the text columns, with the words on their
    // colors (what would go on red, what would come on green); the half rows around it.
    let rows_of = |y: usize| &r.shot.paints[y];
    let raised = (text0..g.ex + g.ew).filter(|&x| bg_at(rows_of(y), x) == Some(t.card)).count();
    assert!(raised > usize::from(g.ew) - 15, "{:?}", rows_of(y));
    for x in text0..g.ex + g.ew {
        match style_at(&r, y, x) {
            Some(Style::Change) => assert_eq!(ink_at(&r, y, x), (Some(t.add), Some(t.add_text))),
            Some(Style::Deleted) => assert_eq!(ink_at(&r, y, x), (Some(t.del), None), "dim stays"),
            _ => assert_eq!(bg_at(rows_of(y), x), Some(t.card)),
        }
    }
    let up: String =
        text(&r, y - 1).chars().skip(usize::from(text0)).take(usize::from(g.ew - 5)).collect();
    assert_eq!(up, "▄".repeat(usize::from(g.ew - 5)));
    assert!(
        (text0..g.ex + g.ew)
            .all(|x| paint_at(rows_of(y - 1), x) == Paint { fg: Some(t.card), ..Paint::default() })
    );
    let dn = text(&r, y + 1);
    assert_eq!(
        dn.chars().skip(usize::from(text0)).take(usize::from(g.ew - 5)).collect::<String>(),
        "▀".repeat(usize::from(g.ew - 5))
    );
    assert!(
        (text0..g.ex + g.ew).all(|x| paint_at(rows_of(y + 1), x)
            == Paint { fg: Some(t.card), bg: Some(t.shadow), fade: 0 })
    );
    assert!(
        text(&r, y - 1).chars().skip(usize::from(g.ex)).take(5).all(|c| c == ' '),
        "the gutter of a half row is blank"
    );
    // The other hunk keeps the flat tint (neutral too), with its words on green and red.
    let other = row_of(&r, "FIVE");
    assert!(
        (text0..g.ex + g.ew)
            .all(|x| style_at(&r, other, x).is_some() || bg_at(rows_of(other), x) == Some(t.hunk)),
        "{:?}",
        rows_of(other)
    );
    assert!((text0..g.ex + g.ew).any(|x| ink_at(&r, other, x).0 == Some(t.add)), "its new words");
    assert!((text0..g.ex + g.ew).any(|x| ink_at(&r, other, x).0 == Some(t.del)), "its old words");
    // The gutter bar runs over the card's rows, on green (`~`).
    assert_eq!(bg_at(rows_of(y), g.ex), Some(t.add));
}
#[test]
fn look_5_the_frame_is_two_rows_shorter_only_while_a_card_is_shown() {
    let mut r = reviewing("l5-short", Skin::Depth, (80, 24));
    assert_eq!(r.app().size.1, 24 - 2 - 2);
    assert_eq!(r.shot.frame.rows.len(), 24, "with its two half rows the screen is whole");
    r.keys("<Esc>");
    assert_eq!((r.app().vi.mode, r.app().size.1), (Mode::Normal, 22));
    assert!(!(1..23).any(|y| text(&r, y).contains('▄')));
    // Flat has no card: the App has the whole height.
    let r = reviewing("l5-flat", Skin::Flat, (80, 24));
    assert_eq!(r.app().size.1, 24);
    assert!(!(0..24).any(|y| text(&r, y).contains('▄')));
    // No palette, no card: nothing to raise it with (LOOK-8).
    let src = format!("a.rs={SRC}");
    let mut q = rig_env("l5-nopal", &[&src], &[], (80, 24), |e| {
        (e.skin, e.syn) = (Skin::Depth, Syntax::new(true));
    });
    q.keys("<CR>jx3jx");
    let id = q.sim.bufs[0].id;
    q.send(SimEvent::App(id, disk(DISK.as_bytes())));
    q.keys("gg]c");
    assert_eq!((q.app().vi.mode, q.app().size.1), (Mode::Review, 22));
    assert!(!(0..24).any(|y| text(&q, y).contains('▄')));
}

#[test]
fn look_5_a_click_lands_on_the_row_it_is_drawn_at_with_the_card_inserted() {
    let g = Geo::new((80, 24), Skin::Depth, true);
    for (what, line) in [("fn one", 0), ("fn six", 5), ("fn eight", 7), ("FIVE", 4)] {
        let mut r = reviewing("l5-click", Skin::Depth, (80, 24));
        let y = row_of(&r, what) as u16;
        r.click(g.ex + 6, y);
        assert_eq!(r.app().vi.mode, Mode::Normal, "a click leaves REVIEW (PTR-2)");
        assert_eq!(r.app().buf.line_of(r.app().buf.cursor), line, "{what}");
    }
}

/// The fade of a row `d` rows from the card, in percent.
fn fog(d: usize) -> u8 {
    (((d as f64 - 2.0) / 11.0 * 0.6).clamp(0.0, 0.6) * 100.0).round() as u8
}

#[test]
fn look_6_rows_that_are_no_change_fade_by_their_distance_from_the_card() {
    let src: String = (0..40).map(|i| format!("let x{i} = {i};\n")).collect();
    let mut r = looked("l6-fog", Skin::Depth, &[&format!("a.rs={src}")], (80, 24));
    r.keys("<CR>20Gx");
    let id = r.sim.bufs[0].id;
    let new: String = (0..40)
        .map(|i| if i == 19 { "let x19 = 99;\n".into() } else { format!("let x{i} = {i};\n") })
        .collect();
    r.send(SimEvent::App(id, disk(new.as_bytes())));
    r.keys("]c");
    assert_eq!(r.app().vi.mode, Mode::Review);
    let (g, t) = (r.sim.geo(), tones());
    let cy = usize::from(r.shot.frame.cursor.1);
    let card = cy..cy + 1;
    let mut seen = vec![];
    for y in 1..=usize::from(g.rows) {
        let p = &r.shot.paints[y];
        let fade = paint_at(p, g.ex + 1).fade;
        let fade_text = paint_at(p, g.ex + 5 + 2).fade;
        if card.contains(&y) || y == cy - 1 || y == cy + 1 {
            assert_eq!((fade, fade_text), (0, 0), "row {y}: the card and its half rows");
            continue;
        }
        let d = cy.abs_diff(y);
        // (the half rows count: a row below the card is one further than the frame's own row)
        let d = d - 1;
        assert_eq!((fade, fade_text), (fog(d), fog(d)), "row {y}, d {d}: line numbers and text");
        seen.push(fade);
    }
    assert!(
        seen.contains(&0) && seen.iter().any(|&f| f >= 40),
        "near rows are clear, far ones faded: {seen:?}"
    );
    // The gutter symbol and the bar of changes are never faded, nor is the tint.
    assert_eq!(paint_at(&r.shot.paints[cy], g.ex + 1), Paint::default());
    assert_eq!(bg_at(&r.shot.paints[cy], g.ex + g.ew - 1), Some(t.card));
}

#[test]
fn look_6_rows_of_other_changes_never_fade_and_context_keeps_its_syntax_colors() {
    let r = reviewing("l6-other", Skin::Depth, (80, 24));
    let g = r.sim.geo();
    let other = row_of(&r, "FIVE");
    assert!(r.shot.paints[other].iter().all(|(_, p)| p.fade == 0), "{:?}", r.shot.paints[other]);
    assert!(r.shot.looks[other].is_empty(), "a row of a change has no syntax colors in REVIEW");
    let y = row_of(&r, "fn seven");
    assert!(r.shot.paints[y].iter().any(|(_, p)| p.fade > 0));
    assert!(
        r.shot.looks[y].iter().any(|(s, l)| *l == Look::Kw && s.start == g.ex + 5),
        "the keyword `fn` keeps its look: {:?}",
        r.shot.looks[y]
    );
    // Flat and calm: REVIEW has no syntax colors, as SYN-1 says.
    for skin in [Skin::Flat, Skin::Calm] {
        let r = reviewing("l6-nosyn", skin, (80, 24));
        assert!(r.shot.looks.iter().all(Vec::is_empty), "{skin:?}");
        assert!(r.shot.paints.iter().flatten().all(|(_, p)| p.fade == 0), "{skin:?}");
    }
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

#[test]
fn look_7_the_panel_recedes_while_the_editor_has_the_focus() {
    let mut r = looked("l7-recede", Skin::Depth, &FILES, (80, 24));
    assert!(r.sim.panel.focus);
    assert!(r.shot.paints.iter().flatten().all(|(_, p)| p.fade == 0), "the panel has the focus");
    r.keys("<CR>"); // a.txt, the first row
    assert!(!r.sim.panel.focus);
    let g = r.sim.geo();
    for y in 1..=9 {
        let fades: Vec<u8> =
            (g.px..g.px + g.pw).map(|x| paint_at(&r.shot.paints[y], x).fade).collect();
        assert!(fades.iter().all(|&f| f == 45), "row {y}: {fades:?}");
    }
    assert!(r.shot.paints[10].iter().all(|(_, p)| p.fade == 0), "no row, nothing to fade");
    // A row on the change background does not recede: it has the tint instead.
    let id = r.sim.bufs[0].id;
    r.send(SimEvent::App(id, disk(b"two\n")));
    let (y, band) = (1, tones().gain);
    assert!(
        (g.px..g.px + g.pw).all(|x| paint_at(&r.shot.paints[y], x).fade == 0
            && [Some(band), Some(tones().add)].contains(&bg_at(&r.shot.paints[y], x))),
        "{:?}",
        r.shot.paints[y]
    );
    assert_eq!(paint_at(&r.shot.paints[y + 1], g.px).fade, 45);
    // The editor does not recede for it.
    assert!(
        r.shot.paints.iter().flatten().filter(|(rg, _)| rg.start >= g.ex).all(|(_, p)| p.fade == 0)
    );
    // Flat has no recede.
    let mut f = looked("l7-flat", Skin::Flat, &FILES, (80, 24));
    f.keys("<CR>");
    assert!(f.shot.paints.iter().flatten().all(|(_, p)| p.fade == 0));
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
            text(&r, 0).starts_with("╭─¹files─") && text(&r, 0).contains("²a.rs"),
            "{name}: {}",
            text(&r, 0)
        );
        assert!(
            r.shot.paints.iter().all(Vec::is_empty),
            "{name}: no tint, fog, shading or border colors"
        );
        let top = text(&r, 0);
        let col = top[..top.find(" 2 changes on disk ").unwrap()].chars().count() as u16;
        assert_eq!(
            style_at(&r, 0, col + 1),
            Some(Style::Change),
            "{name}: the badge is on the change style (reverse video)"
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
fn look_5_golden_review() {
    let src: String = (0..30).map(|i| format!("let x{i} = {i};\n")).collect();
    let mut r = looked("l5-golden", Skin::Depth, &[&format!("a.rs={src}")], (64, 24));
    r.keys("<CR>15Gx");
    let id = r.sim.bufs[0].id;
    let new: String = (0..30)
        .map(|i| {
            if i == 14 || i == 8 {
                format!("let x{i} = 99;\n")
            } else {
                format!("let x{i} = {i};\n")
            }
        })
        .collect();
    r.send(SimEvent::App(id, disk(new.as_bytes())));
    r.keys("]c");
    golden("look_5_review", &dump_look(&r.shot, &tones()));
}

#[test]
fn look_7_golden_find() {
    let mut r = looked("l7-golden", Skin::Depth, &FILES, (64, 14));
    r.keys("/eta");
    golden("look_7_find", &dump_look(&r.shot, &tones()));
}
