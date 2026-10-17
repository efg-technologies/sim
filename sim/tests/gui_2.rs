//! GUI-2: a paint says what its surface is (`Paint::role`), where `layout` makes it, so that a window
//! gives each surface an elevation and a shape without telling a band from a chip by its color. The
//! terminal draws the color and ignores the role: two paints that differ in role alone are the same
//! paint, and the bytes of the Painter do not change.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use sim::color::{Chg, Syntax, Tones};
use sim::layout::Skin;
use sim::paint::{Paint, Painter, Role, Screen};
use sim::sim::SimEvent;
use sim_core::core::app::Event;
use sim_core::core::color::{Rgb, palette};
use std::collections::HashSet;

const BG: Rgb = (0x1e, 0x1e, 0x1e);

const MD: &str = "# Title\n\nSome `code` here and **bold**.\n\n## Part\n\n```rust\nfn main() {}\n```\n\n- [x] done\n";

fn depth(name: &str, files: &[&str], args: &[&str], size: (u16, u16)) -> Rig {
    let mut r = rig_env(name, files, args, size, |e| {
        e.skin = Skin::Depth;
        e.syn = Syntax::new(true);
        e.chrome = true;
    });
    r.send(SimEvent::Term(Event::Bg(BG)));
    r
}

fn tones() -> Tones {
    Tones::new(&palette(BG))
}

/// Every paint of the screen with the text of the row it is in.
fn paints(r: &Rig) -> Vec<(usize, std::ops::Range<u16>, Paint)> {
    let mut out = vec![];
    for (y, row) in r.shot.paints.iter().enumerate() {
        out.extend(row.iter().map(|(c, p)| (y, c.clone(), *p)));
    }
    out
}

fn with_role(r: &Rig, role: Role) -> Vec<(usize, std::ops::Range<u16>, Paint)> {
    paints(r).into_iter().filter(|(_, _, p)| p.role == role).collect()
}

fn text(r: &Rig, y: usize) -> String {
    r.shot.frame.rows[y].text.clone()
}

#[test]
fn gui_2_two_paints_that_differ_in_role_alone_are_the_same_paint() {
    let a = Paint { bg: Some((1, 2, 3)), role: Role::Chip, ..Paint::default() };
    let b = Paint { bg: Some((1, 2, 3)), role: Role::Fence, ..Paint::default() };
    let c = Paint { bg: Some((1, 2, 4)), role: Role::Chip, ..Paint::default() };
    assert_eq!(a, b);
    assert_ne!(a, c);
    assert_eq!(HashSet::from([a, b]).len(), 1, "and they hash alike");
    assert_eq!(Paint::default().role, Role::None);
}

#[test]
fn gui_2_the_roles_never_reach_the_terminal() {
    // the same screen with every role taken away is drawn with the very same bytes
    let r = depth("g2-bytes", &[&format!("a.md={MD}")], &["a.md"], (80, 24));
    let draw = |s: Screen| {
        let mut p = Painter::new(Vec::new());
        p.draw(s).unwrap();
        p.out
    };
    let mut bare = r.shot.clone();
    bare.paints.iter_mut().flatten().for_each(|(_, p)| p.role = Role::None);
    assert!(
        paints(&r).iter().any(|(.., p)| p.role != Role::None),
        "the screen has roles to take away"
    );
    assert_eq!(draw(r.shot.clone()), draw(bare.clone()));
    assert_eq!(r.shot.paints, bare.paints, "and as a Screen the two are equal in what they paint");
}

#[test]
fn gui_2_markdown_s_surfaces_are_fences_headings_and_patches() {
    let r = depth("g2-md", &[&format!("a.md={MD}")], &["a.md"], (80, 24));
    let t = tones();
    let row_of =
        |what: &str| (1..r.shot.frame.rows.len()).find(|&y| text(&r, y).contains(what)).unwrap(); // (not the box's title, the breadcrumb)
    let on = |role| with_role(&r, role);
    // a first-level heading is on the stronger band and a second-level one on the fainter, both Heading
    let h1 = on(Role::Heading).into_iter().find(|(y, ..)| *y == row_of("Title")).unwrap();
    assert_eq!(h1.2.bg, Some(t.head1));
    let h2 = on(Role::Heading).into_iter().find(|(y, ..)| *y == row_of("Part")).unwrap();
    assert_eq!(h2.2.bg, Some(t.head2));
    // every row of the fenced block is on the fence's band
    let fence = on(Role::Fence);
    assert!(fence.len() >= 3 && fence.iter().all(|(.., p)| p.bg == Some(t.band)), "{fence:?}");
    assert!(fence.iter().any(|(y, ..)| *y == row_of("fn main")));
    // inline code is a patch
    let code = on(Role::Patch).into_iter().find(|(y, ..)| *y == row_of("Some")).unwrap();
    assert_eq!(code.2.bg, Some(t.code));
}

#[test]
fn gui_2_the_bars_chips_and_the_open_file_s_row_say_what_they_are() {
    let mut r = depth("g2-chrome", &["a.rs=fn a() {}\n", "b.rs=fn b() {}\n"], &["a.rs"], (100, 24));
    r.keys("<C-n>");
    let t = tones();
    // the header of the tree and the tab bar are bars; the shown tab is raised from the bar
    let bar = with_role(&r, Role::Bar);
    assert!(
        bar.iter().any(|(y, ..)| *y == 1) && bar.iter().all(|(.., p)| p.bg.is_some()),
        "{bar:?}"
    );
    let chips = with_role(&r, Role::Chip);
    assert!(
        chips.iter().any(|(y, c, p)| *y == 1 && p.bg == Some(t.card) && c.start > 30),
        "the tab: {chips:?}"
    );
    // the open file's row in the tree is a chip too (the card's surface)
    let g = r.sim.geo();
    assert!(
        chips.iter().any(|(y, c, _)| *y >= 2 && c.start == g.px && c.end == g.px + g.pw),
        "the open file's row: {chips:?}"
    );
    // the status line has a block for the mode, which is a chip, on a bar
    let last = r.shot.frame.rows.len() - 1;
    assert!(chips.iter().any(|(y, ..)| *y == last), "the mode's block");
    assert!(bar.iter().any(|(y, ..)| *y == last), "the status line's own surface");
}

#[test]
fn gui_2_a_change_from_outside_is_a_tint_and_a_patch_and_the_colorcolumn_a_column() {
    let long = format!("{}\n", "x".repeat(120));
    let mut r = depth("g2-tint", &[&format!("a.txt={long}fn b() {{}}\n")], &["a.txt"], (140, 12));
    r.keys("i");
    // Insert mode: the cursor's line has a surface across it
    let tint = with_role(&r, Role::Tint);
    assert!(tint.iter().any(|(_, c, p)| c.end - c.start > 100 && p.bg.is_some()), "{tint:?}");
    // the hundredth column of the text, on every row
    let col = with_role(&r, Role::Column);
    assert!(
        col.len() >= 3
            && col.iter().all(|(_, c, p)| c.end - c.start == 1 && p.bg == Some(tones().cc))
    );
    // a write from outside: its words are patches and its rows tinted
    r.keys("<Esc>");
    let id = r.sim.bufs[0].id;
    r.send(SimEvent::App(id, disk(format!("{long}fn b() {{}}\nadded line\n").as_bytes())));
    assert!(
        with_role(&r, Role::Patch).iter().any(|(.., p)| p.bg == Some(tones().add)),
        "the words that came"
    );
    assert!(
        with_role(&r, Role::Tint).iter().any(|(.., p)| p.bg == Some(tones().gain)),
        "the row's tint"
    );
}

#[test]
fn gui_2_a_hidden_char_and_the_untrusted_root_are_the_alert() {
    let mut r = depth("g2-alert", &["h.txt=a\u{202e}b <U+202E>\n"], &["h.txt"], (80, 10));
    r.sim.trust = Some(false);
    r.keys("j");
    let alert = with_role(&r, Role::Alert);
    let t = tones();
    let (back, _) = t.alert_block();
    // the real char, as <U+202E>, is sealed on the alert's block; its literal twin has no such paint
    let sealed: Vec<_> = alert.iter().filter(|(.., p)| p.sealed && p.bg == Some(back)).collect();
    assert_eq!(sealed.len(), 1, "{alert:?}");
    let y = sealed[0].0;
    let row = text(&r, y);
    let cols = &sealed[0].1;
    let shown: String =
        row.chars().skip(cols.start as usize).take((cols.end - cols.start) as usize).collect();
    assert_eq!(shown, "<U+202E>");
    // the status line's security slot, in the alert's ink, says it is the alert's too
    let last = r.shot.frame.rows.len() - 1;
    assert!(alert.iter().any(|(y, .., p)| *y == last && p.fg == Some(t.alert_fg())), "{alert:?}");
}

#[test]
fn gui_2_the_picker_s_fill_and_the_terminal_s_shadow_are_said_and_the_selected_row_is_a_patch() {
    let mut r = depth("g2-picker", &["a.rs=fn a() {}\n", "b.rs=fn b() {}\n"], &["a.rs"], (100, 24));
    r.keys("<C-p>");
    let w = r.sim.pick_win().unwrap();
    let t = tones();
    let fill = with_role(&r, Role::Fill);
    assert!(fill.len() >= usize::from(w.h) && fill.iter().all(|(.., p)| p.bg == Some(t.cursor)));
    // a window casts a shadow of its own: the one-cell strip the terminal paints beside it is marked
    let shadow = with_role(&r, Role::Shadow);
    assert!(
        !shadow.is_empty() && shadow.iter().all(|(.., p)| p.bg == Some(t.shadow)),
        "{shadow:?}"
    );
    let sel = with_role(&r, Role::Patch);
    assert!(sel.iter().any(|(y, c, p)| *y == usize::from(w.list.y)
        && c.end - c.start == w.list.w
        && p.bg == Some(t.accent_block().0)));
}

#[test]
fn gui_2_the_scrollbar_s_handle_is_a_thumb() {
    let long: String = (1..=200).map(|i| format!("line {i}\n")).collect();
    let r = depth("g2-thumb", &[&format!("a.txt={long}")], &["a.txt"], (80, 24));
    let thumb = with_role(&r, Role::Thumb);
    let g = r.sim.geo();
    assert!(!thumb.is_empty(), "the lines on screen are a handle");
    assert!(thumb.iter().all(|(y, c, p)| *y >= usize::from(g.y0)
        && c.start == g.ex + g.sign + g.ew
        && p.bg == Some(tones().thumb)));
}

#[test]
fn gui_2_a_changed_file_in_the_tree_is_a_tint_across_its_row() {
    let mut r = depth("g2-tree", &["a.rs=1\n", "b.rs=2\n"], &["a.rs"], (100, 24));
    r.keys("<C-n>");
    let id = r.sim.bufs[0].id;
    r.send(SimEvent::App(id, disk(b"1\nchanged\n")));
    let g = r.sim.geo();
    let tint = with_role(&r, Role::Tint);
    assert!(
        tint.iter().any(|(y, c, p)| *y >= 2
            && c.start == g.px
            && c.end == g.px + g.pw
            && p.bg == Some(tones().gain)),
        "{tint:?}"
    );
}

fn changes(name: &str) -> Rig {
    let mut r = rig_env(name, &CHANGED_FILES, &["src/main.rs"], (100, 24), |e| {
        changed_repo(&e.cwd);
        e.skin = Skin::Depth;
        e.syn = Syntax::new(true);
        e.chrome = true;
    });
    r.send(SimEvent::Term(Event::Bg(BG)));
    r
}

/// The text of the cells a paint covers.
fn under(r: &Rig, y: usize, c: &std::ops::Range<u16>) -> String {
    let cols: Vec<char> = text(r, y).chars().collect();
    cols[usize::from(c.start).min(cols.len())..usize::from(c.end).min(cols.len())].iter().collect()
}

#[test]
fn gui_2_the_changes_view_s_numbers_are_text_colors_and_the_head_tab_is_a_chip() {
    let mut r = changes("g2-changes");
    r.keys(" ge");
    let t = tones();
    let y = (0..r.shot.frame.rows.len())
        .find(|&y| text(&r, y).contains("main.rs") && text(&r, y).contains("+2"))
        .unwrap();
    // `+2` is green text and `−1` red text: colors of a text, no surface and no role
    let word = |what: &str| {
        paints(&r)
            .into_iter()
            .filter(|(row, c, _)| *row == y && under(&r, y, c) == what)
            .map(|(.., p)| p)
            .next_back()
            .unwrap_or_else(|| panic!("no paint on {what:?}"))
    };
    let (add, del) = (word("+2"), word("\u{2212}1"));
    assert_eq!((add.fg, add.bg, add.role), (Some(t.change_fg(Chg::Add)), None, Role::None));
    assert_eq!((del.fg, del.bg, del.role), (Some(t.change_fg(Chg::Del)), None, Role::None));
    // HEAD's version of the deleted file is a tab like any other: the shown one is a chip, whose cells
    // (a tab's pieces are paints of their own) read `HEAD:gone.rs` and are on the card's surface
    walk_to(&mut r, "gone.rs");
    r.keys("\n");
    let mut chip: Vec<_> =
        with_role(&r, Role::Chip).into_iter().filter(|(y, ..)| *y == 1).collect();
    chip.sort_by_key(|(_, c, _)| c.start);
    let said: String = chip.iter().map(|(y, c, _)| under(&r, *y, c)).collect();
    assert!(said.contains("HEAD:gone.rs"), "{said:?}");
    assert!(chip.iter().all(|(.., p)| p.bg == Some(t.card)));
}

#[test]
fn gui_2_no_background_of_depth_is_without_a_role_in_the_tree_the_changes_view_or_the_picker() {
    let mut r = changes("g2-roles");
    let mut stages = vec![];
    let mut check = |r: &Rig, what: &str| {
        for (y, c, p) in paints(r) {
            assert!(
                p.bg.is_none() || p.role != Role::None,
                "{what}: row {y} {c:?} has {:?} and no role: {:?}",
                p.bg,
                under(r, y, &c)
            );
        }
        stages.push(what.to_string());
    };
    check(&r, "a file");
    r.keys("<C-n>");
    check(&r, "the tree");
    r.keys("<Esc> ge");
    // (a name too long for the panel is cut with a `…`: text, and the row has no surface of its own)
    assert!(r.panel().iter().any(|l| l.contains('\u{2026}')), "{:?}", r.panel());
    check(&r, "the changes view");
    walk_to(&mut r, "gone.rs");
    r.keys("\n");
    check(&r, "HEAD's version in a tab");
    r.keys("<C-p>");
    check(&r, "the picker");
    r.keys("<Esc> ");
    r.pause();
    check(&r, "which-key");
    r.keys("<Esc>/main");
    check(&r, "a search");
    assert_eq!(stages.len(), 7);
}
