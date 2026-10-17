//! GUI-1: a Screen says its layers, from the lowest to the highest, in cells: the boxes (depth), the
//! text of the open file, its scrollbar, the tree's cursor row, the status line, and the windows over
//! the rest, so that a window draws a surface for each and infers none from a geometry or a color.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use sim::color::Syntax;
use sim::layout::Skin;
use sim::paint::{Layer, LayerKind};
use sim::pick::Rect;
use sim::sim::SimEvent;
use sim_core::core::app::Event;

const TEXT: &str = "fn main() {\n    println!(\"hi\");\n}\n";

fn at(name: &str, skin: Skin, files: &[&str], args: &[&str], size: (u16, u16)) -> Rig {
    let mut r = rig_env(name, files, args, size, |e| {
        e.skin = skin;
        e.syn = Syntax::new(true);
        e.chrome = true;
    });
    r.send(SimEvent::Term(Event::Bg((0x1e, 0x1e, 0x1e))));
    r
}

fn depth(name: &str, size: (u16, u16)) -> Rig {
    let files = [format!("a.rs={TEXT}"), "b.rs=other\n".to_string()];
    let files: Vec<&str> = files.iter().map(String::as_str).collect();
    at(name, Skin::Depth, &files, &["a.rs"], size)
}

fn rect(x: u16, y: u16, w: u16, h: u16) -> Rect {
    Rect { x, y, w, h }
}

fn kinds(r: &Rig) -> Vec<LayerKind> {
    r.shot.layers.iter().map(|l| l.kind).collect()
}

fn find(r: &Rig, f: impl Fn(&LayerKind) -> bool) -> Option<Layer> {
    r.shot.layers.iter().copied().find(|l| f(&l.kind))
}

#[test]
fn gui_1_depth_says_its_boxes_its_text_its_scrollbar_and_its_status_line_lowest_first() {
    let mut r = depth("g1-depth", (100, 24));
    r.keys("<C-n>");
    let (g, (w, h)) = (r.sim.geo(), (100, 24));
    let ls = &r.shot.layers;
    let text = LayerKind::Text { sign: g.sign, gutter: 3 + 2 };
    assert_eq!(
        kinds(&r),
        [
            LayerKind::Box { focused: true },
            LayerKind::Box { focused: false },
            text,
            LayerKind::Scroll,
            LayerKind::Cursor,
            LayerKind::Status
        ]
    );
    // the panel's box and the text's box: the borders' rings are in them, the status line is under them
    assert_eq!(ls[0].rect, rect(0, 0, g.pw + 2, h - 1));
    assert_eq!(ls[1].rect, rect(g.ex - 1, 0, g.sign + g.ew + 2, h - 1));
    assert_eq!(ls[2].rect, rect(g.ex, g.y0, g.sign + g.ew, g.rows));
    assert_eq!(ls[3].rect, rect(g.ex + g.sign + g.ew, g.y0, 1, g.rows));
    assert_eq!(ls[5].rect, rect(0, h - 1, w, 1));
    // the boxes end where the status line begins and do not overlap each other
    assert_eq!(ls[0].rect.x + ls[0].rect.w, ls[1].rect.x);
}

#[test]
fn gui_1_the_box_that_has_the_focus_says_so_and_so_does_the_tree_s_cursor_row() {
    let mut r = depth("g1-focus", (100, 24));
    r.keys("<C-n>");
    assert_eq!(
        kinds(&r)[..2],
        [LayerKind::Box { focused: true }, LayerKind::Box { focused: false }]
    );
    let g = r.sim.geo();
    let cursor = find(&r, |k| *k == LayerKind::Cursor).unwrap();
    assert_eq!(cursor.rect, rect(g.px, g.y0 + (r.sim.panel.cur - r.sim.panel.top) as u16, g.pw, 1));
    // the cursor's row moves with the tree's cursor
    let before = cursor.rect.y;
    r.keys("j");
    assert_eq!(find(&r, |k| *k == LayerKind::Cursor).unwrap().rect.y, before + 1);
    // a file opened gives the text the focus: the tree has no cursor row to show
    r.keys("\n");
    assert_eq!(
        kinds(&r)[..2],
        [LayerKind::Box { focused: false }, LayerKind::Box { focused: true }]
    );
    assert!(find(&r, |k| *k == LayerKind::Cursor).is_none());
}

#[test]
fn gui_1_the_text_layer_says_the_sign_column_and_the_width_of_the_numbers() {
    let long: String = (1..=1200).map(|i| format!("{i}\n")).collect();
    let r = at("g1-gutter", Skin::Depth, &[&format!("a.txt={long}")], &["a.txt"], (100, 24));
    let g = r.sim.geo();
    let t = find(&r, |k| matches!(k, LayerKind::Text { .. })).unwrap();
    // 1,200 lines are four digits: the gutter is the digits and a blank on each side
    assert_eq!(t.kind, LayerKind::Text { sign: g.sign, gutter: 4 + 2 });
    assert_eq!(g.sign, 1, "the interface keeps a sign column");
}

#[test]
fn gui_1_the_picker_is_a_modal_window_over_the_rest_and_which_key_is_not() {
    let mut r = depth("g1-float", (100, 24));
    assert!(find(&r, |k| matches!(k, LayerKind::Float { .. })).is_none());
    r.keys("<C-p>");
    let w = r.sim.pick_win().unwrap();
    let last = *r.shot.layers.last().unwrap();
    assert_eq!(last.kind, LayerKind::Float { modal: true });
    assert_eq!(last.rect, rect(w.x, w.y, w.w, w.h), "the window of PICK-1");
    r.keys("<Esc>");
    assert!(find(&r, |k| matches!(k, LayerKind::Float { .. })).is_none());
    // which-key after a pause on the leader: a window, and not a modal one (the keys still go on)
    r.keys(" ");
    r.pause();
    let last = *r.shot.layers.last().unwrap();
    assert_eq!(last.kind, LayerKind::Float { modal: false });
    assert!(last.rect.w > 0 && last.rect.x + last.rect.w <= 100 && last.rect.y + last.rect.h <= 23);
    // the windows are the highest: nothing is above them but a window
    assert!(r.shot.layers.iter().rev().take(1).all(|l| matches!(l.kind, LayerKind::Float { .. })));
}

#[test]
fn gui_1_calm_has_no_layer_but_its_windows_and_flat_has_no_boxes() {
    let files = [format!("a.rs={TEXT}")];
    let names: Vec<&str> = files.iter().map(String::as_str).collect();
    let mut calm = at("g1-calm", Skin::Calm, &names, &["a.rs"], (100, 24));
    assert!(calm.shot.layers.is_empty(), "calm draws no surface: {:?}", kinds(&calm));
    calm.keys("<C-p>");
    assert_eq!(
        kinds(&calm),
        [LayerKind::Float { modal: true }],
        "but a window is one in every look"
    );
    let flat = at("g1-flat", Skin::Flat, &names, &["a.rs"], (100, 24));
    assert!(kinds(&flat).iter().all(|k| !matches!(k, LayerKind::Box { .. })), "{:?}", kinds(&flat));
    assert!(matches!(kinds(&flat)[0], LayerKind::Text { .. }), "{:?}", kinds(&flat));
}

#[test]
fn gui_1_with_no_file_there_is_no_text_and_no_scrollbar() {
    let r = at("g1-none", Skin::Depth, &["x.txt=1\n"], &[], (100, 24));
    let k = kinds(&r);
    assert!(k.iter().all(|k| !matches!(k, LayerKind::Text { .. } | LayerKind::Scroll)), "{k:?}");
    assert!(k.contains(&LayerKind::Status));
}

#[test]
fn gui_1_a_small_screen_has_no_boxes_because_depth_has_none_there() {
    let r = depth("g1-small", (30, 8));
    assert!(kinds(&r).iter().all(|k| !matches!(k, LayerKind::Box { .. })), "{:?}", kinds(&r));
}

/// The depth look over a repository that differs from HEAD (DIFF-1): `src/main.rs` is open.
fn changes(name: &str) -> Rig {
    let mut r = rig_env(name, &CHANGED_FILES, &["src/main.rs"], (100, 24), |e| {
        changed_repo(&e.cwd);
        e.skin = Skin::Depth;
        e.syn = Syntax::new(true);
        e.chrome = true;
    });
    r.send(SimEvent::Term(Event::Bg((0x1e, 0x1e, 0x1e))));
    r
}

#[test]
fn gui_1_the_changes_view_has_the_trees_layers_and_the_head_tab_has_the_texts() {
    let mut r = changes("g1-changes");
    r.keys(" ge");
    assert!(r.status().contains("CHANGES"), "{}", r.status());
    // the view is the panel: its box has the focus, the text's does not, and the cursor row is the tree's
    assert_eq!(
        kinds(&r)[..2],
        [LayerKind::Box { focused: true }, LayerKind::Box { focused: false }]
    );
    let g = r.sim.geo();
    let cursor = find(&r, |k| *k == LayerKind::Cursor).unwrap();
    assert_eq!(cursor.rect, rect(g.px, g.y0 + (r.sim.panel.cur - r.sim.panel.top) as u16, g.pw, 1));
    r.keys("j");
    assert_eq!(find(&r, |k| *k == LayerKind::Cursor).unwrap().rect.y, cursor.rect.y + 1);
    // a deleted file's row opens HEAD's version: a tab of its own, the text has the focus and the tree
    // has no cursor row any more
    walk_to(&mut r, "gone.rs");
    r.keys("\n");
    assert!(r.screen.rows[1].text.contains("HEAD:gone.rs"), "{}", r.screen.rows[1].text);
    assert_eq!(
        kinds(&r)[..2],
        [LayerKind::Box { focused: false }, LayerKind::Box { focused: true }]
    );
    assert!(find(&r, |k| matches!(k, LayerKind::Text { .. })).is_some());
    assert!(find(&r, |k| *k == LayerKind::Cursor).is_none());
}
