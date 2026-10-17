//! COLOR-2: no color at all where it is not wanted (`NO_COLOR`, no-color.org) or cannot be had (a
//! terminal of fewer than 8 colors): weight, underline, slant and inverse video carry everything,
//! the terminal is not asked for its colors, and the interface is still drawn.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use sim::color::Syntax;
use sim::io::{colors_in, mono, terminfo_colors};
use sim::layout::Skin;
use sim::paint::Painter;
use sim::sim::SimEvent;
use sim_core::core::app::Event;
use std::ffi::OsString;

/// The SGR parameters of a stream that name a color: 30-38, 40-48, 90-97 and 100-107 (the default
/// colors, 39 and 49, name none).
fn colored(bytes: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(bytes);
    let mut found = vec![];
    for seq in text.split("\x1b[").skip(1) {
        let Some((params, _)) = seq.split_once(|c: char| c.is_ascii_alphabetic()) else { continue };
        if !seq[params.len()..].starts_with('m') {
            continue;
        }
        for p in params.split(';').filter_map(|p| p.parse::<u16>().ok()) {
            if matches!(p, 30..=38 | 40..=48 | 90..=97 | 100..=107) {
                found.push(format!("{p} in ESC[{params}m"));
            }
        }
    }
    found
}

/// What the terminal gets for a screen, drawn from nothing.
fn drawn(r: &Rig) -> Vec<u8> {
    let mut p = Painter::new(Vec::<u8>::new());
    p.draw(r.shot.clone()).unwrap();
    p.out
}

/// A Rig in `skin` with no color, the interface drawn, `a.rs` and `b.md` in a directory.
fn bare(name: &str, skin: Skin) -> Rig {
    let a = "a.rs=// a comment\nfn main() {\n    let s = \"text\"; // more\n    let n: u32 = 42;\n    one(s, n)\n}\n";
    let b = "b.md=# Title\n\nA [link](http://x.y) with **strong**, *slant* and `code`.\n\n- item\n> quote\n\n```rust\nfn x() {}\n```\n";
    rig_env(name, &[a, b], &[], (100, 30), |e| {
        e.skin = skin;
        e.chrome = true;
        e.syn = Syntax::mono();
    })
}

#[test]
fn color_2_no_color_where_it_is_not_wanted_or_cannot_be_had() {
    let os = |s: &str| Some(OsString::from(s));
    // NO_COLOR set and not empty (no-color.org)
    assert!(mono(os("1"), os("xterm-256color"), None));
    assert!(mono(os("0"), os("xterm-256color"), None), "any value, even 0, says no color");
    assert!(!mono(os(""), os("xterm-256color"), None), "empty means unset");
    assert!(!mono(None, os("xterm-256color"), None));
    // terminals that cannot do 8 colors, by name
    for term in ["dumb", "vt100", "vt102", "vt220", "vt525"] {
        assert!(mono(None, os(term), None), "{term}");
    }
    assert!(mono(None, None, None), "no TERM at all: nothing is known, nothing is asked");
    // by terminfo's colors
    assert!(mono(None, os("tty0"), Some(2)));
    assert!(mono(None, os("tty0"), Some(0)));
    assert!(!mono(None, os("linux"), Some(8)));
    assert!(!mono(None, os("xterm"), Some(256)));
    assert!(!mono(None, os("weird"), None), "a terminfo that cannot be read says nothing");
}

/// A compiled terminfo file as ncurses writes it: `numbers` in `width` bytes each.
fn terminfo(wide: bool, bools: usize, numbers: &[i32]) -> Vec<u8> {
    let name = b"test|a test terminal\0";
    let mut out = vec![];
    let magic: u16 = if wide { 0o1036 } else { 0o432 };
    for v in [magic, name.len() as u16, bools as u16, numbers.len() as u16, 0, 0] {
        out.extend(v.to_le_bytes());
    }
    out.extend(name);
    out.extend(vec![0; bools]);
    if (name.len() + bools) % 2 == 1 {
        out.push(0);
    }
    for n in numbers {
        match wide {
            true => out.extend(n.to_le_bytes()),
            false => out.extend((*n as i16).to_le_bytes()),
        }
    }
    out
}

fn numbers(colors: i32, count: usize) -> Vec<i32> {
    (0..count).map(|i| if i == 13 { colors } else { 80 }).collect()
}

#[test]
fn color_2_terminfo_says_how_many_colors() {
    for wide in [false, true] {
        for bools in [0, 1, 3, 30] {
            let db = terminfo(wide, bools, &numbers(8, 16));
            assert_eq!(colors_in(&db), Some(8), "wide {wide}, {bools} booleans");
            let db = terminfo(wide, bools, &numbers(256, 16));
            assert_eq!(colors_in(&db), Some(256));
            let db = terminfo(wide, bools, &numbers(-1, 16));
            assert_eq!(colors_in(&db), None, "-1 is absent");
            let db = terminfo(wide, bools, &numbers(-2, 16));
            assert_eq!(colors_in(&db), None, "-2 is cancelled");
        }
    }
    assert_eq!(colors_in(&terminfo(false, 0, &numbers(8, 13))), None, "too few numbers: no colors");
    assert_eq!(colors_in(&[]), None);
    assert_eq!(colors_in(b"not a terminfo file at all"), None);
    let mut cut = terminfo(true, 2, &numbers(8, 16));
    cut.truncate(cut.len() - 30);
    assert_eq!(colors_in(&cut), None, "a file cut short");
}

#[test]
fn color_2_terminfo_is_found_by_name_in_the_directories_of_the_system() {
    let dir = temp_dir("color-2-terminfo");
    let one = |sub: &str, name: &str, colors: i32| {
        let at = dir.0.join(sub);
        std::fs::create_dir_all(&at).unwrap();
        std::fs::write(at.join(name), terminfo(false, 2, &numbers(colors, 16))).unwrap();
    };
    one("x", "xtest", 256);
    one("78", "xhex", 2); // macOS names the folder by the first letter's hex
    let dirs = [std::path::PathBuf::from("/nonexistent"), dir.0.clone()];
    assert_eq!(terminfo_colors("xtest", &dirs), Some(256));
    assert_eq!(terminfo_colors("xhex", &dirs), Some(2));
    assert_eq!(terminfo_colors("xnone", &dirs), None);
    assert_eq!(terminfo_colors("", &dirs), None);
    // a TERM is a name, not a path
    assert_eq!(terminfo_colors("../x/xtest", &dirs), None);
    assert_eq!(terminfo_colors("x/xtest", &dirs), None);
}

#[test]
fn color_2_a_screen_without_color_has_no_color_in_it() {
    for skin in [Skin::Calm, Skin::Flat, Skin::Depth] {
        let mut r = bare("color-2-none", skin);
        let seen = |r: &Rig, what: &str| {
            let bad = colored(&drawn(r));
            assert!(bad.is_empty(), "{skin:?} {what}: {bad:?}");
        };
        seen(&r, "opening");
        r.keys("<CR>");
        seen(&r, "a.rs");
        r.keys("<C-w>l/more\n");
        seen(&r, "a search lit");
        r.keys("ggvjl");
        seen(&r, "a selection");
        r.keys("<Esc>");
        // the disk changes the file under the editor: marks, then a conflict with an edit
        let id = r.sim.bufs[0].id;
        let disk = "// a comment\nfn main() {\n    let s = \"TEXT\"; // more\n    let n: u32 = 43;\n    one(s, n)\n}\n";
        r.send(SimEvent::App(id, self::disk(disk.as_bytes())));
        seen(&r, "marks");
        r.keys("jjx");
        seen(&r, "an edit");
        r.keys("<Space>");
        seen(&r, "a key after");
    }
}

#[test]
fn color_2_markdown_and_the_panel_have_no_color_either() {
    for skin in [Skin::Calm, Skin::Flat, Skin::Depth] {
        let mut r = bare("color-2-md", skin);
        r.keys("jj<CR>"); // b.md
        let bad = colored(&drawn(&r));
        assert!(bad.is_empty(), "{skin:?}: {bad:?}");
        assert!(r.text().contains("Title"), "{skin:?}: the file is open");
    }
}

#[test]
fn color_2_weight_underline_slant_and_inverse_stand_in_for_the_colors() {
    let mut r = bare("color-2-attrs", Skin::Depth);
    r.keys("<CR>");
    let out = String::from_utf8(drawn(&r)).unwrap();
    // a comment is faint and slanted, as it is where there is no palette
    assert!(out.contains("\x1b[0;2;3m"), "a comment is faint and italic: {out:?}");
    // the interface is drawn: a box (depth) and the header
    assert!(r.sim.ui(), "no color does not mean no interface");
    assert!(out.contains('╭') || out.contains('┌'), "a box is drawn: {out:?}");
    // a changed word is inverse video (si's change style without a palette)
    let id = r.sim.bufs[0].id;
    r.send(SimEvent::App(id, disk(b"// a comment\nfn MAIN() {\n}\n")));
    let out = String::from_utf8(drawn(&r)).unwrap();
    assert!(out.contains(";7m"), "a word that came from the disk is inverse: {out:?}");
    // a heading is bold, a link underlined
    r.keys("<C-w>h");
    let mut m = bare("color-2-attrs-md", Skin::Depth);
    m.keys("jj<CR>");
    let out = String::from_utf8(drawn(&m)).unwrap();
    assert!(out.contains(";1m"), "a heading is bold: {out:?}");
    assert!(out.contains(";4m"), "a link is underlined: {out:?}");
}

#[test]
fn color_2_a_reply_to_the_color_query_is_not_wanted_and_changes_nothing() {
    let mut r = bare("color-2-reply", Skin::Depth);
    r.keys("<CR>");
    let before = drawn(&r);
    r.send(SimEvent::Term(Event::Bg((255, 255, 255))));
    r.send(SimEvent::Osc("4;1;rgb:ff/00/00".into()));
    assert_eq!(drawn(&r), before, "the screen is as it was");
    assert!(colored(&drawn(&r)).is_empty());
    assert!(!r.sim.asking(), "nothing is waited for at the quit: nothing was asked");
}
