//! GUI-3: what a cell finally is (`paint::resolve`, `Resolver`, `Look::attrs`, `Attrs`), the one
//! place that has the Painter's rules, so that a window draws the pixels of the bytes the terminal
//! is sent, with no table of its own. The Painter's output and the Resolver's cells agree on every
//! column of every row.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use sim::color::{NUMBER_FLOOR, Syntax, TEXT_FLOOR};
use sim::layout::Skin;
use sim::paint::{Attrs, Look, Paint, Painter, Resolved, Resolver, Role, Screen, resolve};
use sim::sim::SimEvent;
use sim_core::core::app::Event;
use sim_core::core::color::{Rgb, apca, palette};
use sim_core::core::view::Style;

const DARK: Rgb = (0x1e, 0x1e, 0x1e);
const LIGHT: Rgb = (0xfd, 0xf6, 0xe3);

fn attrs(look: Look, pal: bool) -> (Vec<u8>, Option<Look>, bool) {
    let a = look.attrs(pal);
    ([1u8, 2, 3, 4, 7].into_iter().filter(|&p| a.attrs.has(p)).collect(), a.hue, a.dim)
}

#[test]
fn gui_3_attrs_are_the_parameters_in_the_order_they_are_written() {
    let a = Attrs::parse(";4;1");
    assert!(a.underline() && a.bold() && !a.italic() && !a.faint() && !a.reverse());
    assert_eq!(a.to_string(), ";4;1", "written back as it came, in its order");
    assert_eq!(Attrs::parse("").to_string(), "");
    assert_eq!(Attrs::parse(";2;3").to_string(), ";2;3");
    assert!(Attrs::parse(";2;3").faint() && Attrs::parse(";2;3").italic());
    assert!(Attrs::parse(";7").reverse());
    // what is not a number is not a parameter, and no more than six are kept
    assert_eq!(Attrs::parse(";x;1;;2").to_string(), ";1;2");
    assert_eq!(Attrs::parse(";1;2;3;4;7;1;2;3").to_string(), ";1;2;3;4;7;1");
    assert_eq!(Attrs::default(), Attrs::parse(""));
}

#[test]
fn gui_3_a_look_says_its_weight_slant_underline_hue_and_whether_it_is_the_secondary_gray() {
    // (look, palette known) -> (attributes, the hue it takes, the secondary gray)
    assert_eq!(attrs(Look::Kw, true), (vec![1], Some(Look::Kw), false));
    for l in [Look::Str, Look::Ty, Look::Num, Look::Fun] {
        assert_eq!(attrs(l, true), (vec![], Some(l), false), "{l:?}");
    }
    for l in [Look::H1, Look::H2, Look::H3, Look::H4, Look::H5, Look::H6, Look::Bold] {
        assert_eq!(attrs(l, true), (vec![1], None, false), "{l:?}");
    }
    assert_eq!(attrs(Look::Link, true), (vec![4], None, false));
    assert_eq!(attrs(Look::Em, true), (vec![3], None, false));
    for l in [Look::Bullet, Look::Quote, Look::Lang, Look::Done, Look::Rule] {
        assert_eq!(attrs(l, true), (vec![], None, true), "{l:?}");
    }
    for l in [Look::Code, Look::Band, Look::Sel, Look::Sub] {
        assert_eq!(attrs(l, true), (vec![], None, false), "{l:?}");
    }
    // a comment is italic where the palette is known, and faint and italic where it is not
    assert_eq!(attrs(Look::Com, true), (vec![3], None, true));
    assert_eq!(attrs(Look::Com, false), (vec![2, 3], None, true));
    assert_eq!(Look::Com.attrs(false).attrs.to_string(), ";2;3");
    for l in [Look::Mark, Look::Tick, Look::Url] {
        assert_eq!(attrs(l, true), (vec![], None, true), "{l:?}");
        assert_eq!(attrs(l, false), (vec![2], None, true), "{l:?}");
    }
}

fn resolved(style: Style, ul: bool, look: Option<Look>, paint: Paint, bg: Option<Rgb>) -> Resolved {
    let syn = bg.map_or(Syntax::new(true), |b| {
        let mut s = Syntax::new(true);
        s.bg(b);
        s
    });
    resolve(style, ul, look, paint, bg.map(palette), &syn)
}

#[test]
fn gui_3_the_style_gives_the_colors_and_the_change_style_wins_over_a_look() {
    let p = palette(DARK);
    let r = resolved(Style::Primary, false, None, Paint::default(), Some(DARK));
    assert_eq!((r.fg, r.bg, r.ansi, r.attrs), (Some(p.primary), None, None, Attrs::default()));
    let r = resolved(Style::Secondary, false, None, Paint::default(), Some(DARK));
    assert_eq!(r.fg, Some(p.secondary));
    // the change style is the only chromatic background; a look on it can be bold or italic only
    let r = resolved(Style::Change, false, Some(Look::Kw), Paint::default(), Some(DARK));
    assert_eq!((r.bg, r.fg, r.ansi), (Some(p.change), Some(p.change_text), None));
    assert!(r.attrs.bold(), "the weight of the keyword stays");
    // Hidden is the primary color, reversed, and says it is hidden
    let r = resolved(Style::Hidden, false, None, Paint::default(), Some(DARK));
    assert!(r.hidden && r.attrs.reverse() && r.fg == Some(p.primary));
    // a sealed paint is a hidden char whatever the style says
    let sealed = Paint { sealed: true, ..Paint::default() };
    assert!(resolved(Style::Primary, false, None, sealed, Some(DARK)).hidden);
    assert!(!resolved(Style::Primary, false, None, Paint::default(), Some(DARK)).hidden);
}

#[test]
fn gui_3_a_paint_gives_its_own_colors_weight_and_role_and_the_underline_comes_before_the_look() {
    let t = sim::color::Tones::new(&palette(DARK));
    let paint = Paint {
        fg: Some((1, 2, 3)),
        bg: Some(t.card),
        bold: true,
        role: Role::Chip,
        ..Paint::default()
    };
    let r = resolved(Style::Primary, true, Some(Look::Em), paint, Some(DARK));
    assert_eq!((r.fg, r.bg, r.role), (Some((1, 2, 3)), Some(t.card), Role::Chip));
    // the order the terminal is sent: the underline, the look's, the paint's weight
    assert_eq!(r.attrs.to_string(), ";4;3;1");
    // a color of the paint's is meant over a syntax look's hue
    let kw = resolved(Style::Primary, false, Some(Look::Kw), Paint::default(), Some(DARK));
    let own = resolved(Style::Primary, false, Some(Look::Kw), paint, Some(DARK));
    assert!(kw.fg.is_some() && own.fg == Some((1, 2, 3)) && own.ansi.is_none());
    // with no palette a keyword is the terminal's own color (SGR 31-36), not a color of ours
    let r = resolved(Style::Primary, false, Some(Look::Kw), Paint::default(), None);
    assert_eq!((r.fg, r.ansi), (None, Some(1)));
}

#[test]
fn gui_3_the_fog_is_the_pages_rank_and_not_the_secondary_gray_s_and_a_selection_has_none() {
    let p = palette(DARK);
    let fog = Paint { fade: 60, ..Paint::default() };
    // text fades
    assert_eq!(resolved(Style::Primary, false, None, fog, Some(DARK)).fade, 60);
    // the secondary gray is rank, not distance: it does not fade, unless it is the numbers
    assert_eq!(resolved(Style::Secondary, false, None, fog, Some(DARK)).fade, 0);
    assert_eq!(resolved(Style::Secondary, false, Some(Look::Com), fog, Some(DARK)).fade, 0);
    let numbers = Paint { fade: 60, soft: true, ..Paint::default() };
    let n = resolved(Style::Secondary, false, None, numbers, Some(DARK));
    assert!(n.fade == 60 && n.soft && n.fg == Some(p.secondary));
    // the change style never fades
    assert_eq!(resolved(Style::Change, false, None, fog, Some(DARK)).fade, 0);
    // a selection takes its gray and the fog does not reach it
    let r = resolved(Style::Primary, false, Some(Look::Sel), fog, Some(DARK));
    assert!(r.selected && r.fade == 0 && r.bg.is_some() && r.ansi.is_none());
    assert!(!resolved(Style::Primary, false, None, fog, Some(DARK)).selected);
    // no palette, no fog: there is no page to fade into
    assert_eq!(resolved(Style::Primary, false, None, fog, None).fade, 0);
}

#[test]
fn gui_3_ink_fades_toward_the_page_and_stops_at_the_floor_of_text_or_of_the_numbers() {
    for page in [DARK, LIGHT] {
        let p = palette(page);
        let text = resolved(Style::Primary, false, None, Paint::default(), Some(page));
        let soft = Resolved { soft: true, ..text };
        assert_eq!(text.ink(page, 0), Some(p.primary), "no fade changes nothing");
        let (mut last_t, mut last_n) = (apca(p.primary, page).abs(), apca(p.primary, page).abs());
        for f in (10..=100).step_by(10) {
            let (t, n) = (text.ink(page, f).unwrap(), soft.ink(page, f).unwrap());
            let (lt, ln) = (apca(t, page).abs(), apca(n, page).abs());
            assert!(lt >= TEXT_FLOOR - 1.0, "{page:?} fade {f}: text |Lc| {lt:.1}");
            assert!(ln >= NUMBER_FLOOR - 1.0, "{page:?} fade {f}: numbers |Lc| {ln:.1}");
            assert!(lt <= last_t + 3.0 && ln <= last_n + 3.0, "never away from the page");
            (last_t, last_n) = (lt, ln);
        }
        // all the way: the text rests at 75, the numbers go on to 40
        assert!((TEXT_FLOOR - 1.0..TEXT_FLOOR + 3.0).contains(&last_t), "{page:?} {last_t:.1}");
        assert!((NUMBER_FLOOR - 1.0..NUMBER_FLOOR + 3.0).contains(&last_n), "{page:?} {last_n:.1}");
        // a fade over 100 is a fade of 100, and a cell with no color has no ink
        assert_eq!(text.ink(page, 200), text.ink(page, 100));
        assert_eq!(Resolved { fg: None, ..text }.ink(page, 50), None);
    }
}

// A Painter's bytes, read back cell by cell: (attributes, text color, ansi, background).
type Cell = (String, Option<Rgb>, Option<u8>, Option<Rgb>);

fn params(sgr: &str) -> Cell {
    let n: Vec<u8> = sgr.split(';').filter_map(|p| p.parse().ok()).collect();
    let (mut attrs, mut fg, mut ansi, mut bg, mut i) = (String::new(), None, None, None, 1);
    while i < n.len() {
        match n[i] {
            38 | 48 if n.get(i + 1) == Some(&2) && i + 4 < n.len() => {
                let c = (n[i + 2], n[i + 3], n[i + 4]);
                if n[i] == 38 {
                    fg = Some(c)
                } else {
                    bg = Some(c)
                }
                i += 5;
            }
            c @ 30..=37 => (ansi, i) = (Some(c - 30), i + 1),
            c => (attrs += &format!(";{c}"), i += 1).0,
        }
    }
    (attrs, fg, ansi, bg)
}

/// What the terminal would show: the cells of each row, as the bytes the Painter writes set them.
fn terminal(s: &Screen) -> Vec<Vec<Cell>> {
    let mut p = Painter::new(Vec::new());
    p.draw(s.clone()).unwrap();
    let out = String::from_utf8(p.out).unwrap();
    let mut rows: Vec<Vec<Cell>> = vec![vec![]; s.frame.rows.len()];
    let (mut row, mut now) = (0usize, Cell::default());
    let mut cs = out.chars().peekable();
    while let Some(c) = cs.next() {
        if c != '\x1b' {
            let w = width(c.encode_utf8(&mut [0; 4]));
            (0..w).for_each(|_| rows[row].push(now.clone()));
            continue;
        }
        let mut seq = String::new();
        while let Some(&n) = cs.peek() {
            cs.next();
            seq.push(n);
            if n.is_ascii_alphabetic() {
                break;
            }
        }
        match seq.chars().last() {
            Some('H') if !seq.starts_with('[') || !seq.contains('?') => {
                row =
                    seq[1..seq.len() - 1].split(';').next().unwrap().parse::<usize>().unwrap() - 1;
            }
            Some('m') => {
                let sgr = &seq[1..seq.len() - 1];
                now = if sgr == "0" { Cell::default() } else { params(&format!("0{}", &sgr[1..])) };
            }
            _ => {}
        }
    }
    rows
}

fn resolver_agrees_with_the_painter(s: &Screen, what: &str) {
    let (mut res, term) = (Resolver::new(), terminal(s));
    let (mut checked, mut seen) = (0, std::collections::HashSet::new());
    for (i, want) in term.iter().enumerate() {
        let got = res.row(s, i);
        assert!(
            got.len() >= want.len(),
            "{what} row {i}: {} cells, the Painter wrote {}",
            got.len(),
            want.len()
        );
        for (x, (w, g)) in want.iter().zip(&got).enumerate() {
            let fg = s.frame.pal.map_or(g.fg, |p| g.ink(p.bg, g.fade));
            let cell = (g.attrs.to_string(), fg, g.ansi, g.bg);
            assert_eq!(&cell, w, "{what} row {i} col {x}: {:?}", s.frame.rows[i].text);
            (checked, _) = (checked + 1, seen.insert(cell));
        }
    }
    assert!(
        checked > 100 && seen.len() >= 4,
        "{what}: {checked} cells compared, {} kinds",
        seen.len()
    );
}

const MIXED: &str = "# Title 日本語\n\nSome `code` and **bold** and *italic* [x](y)\n\n```rust\nfn main() { let s = \"hi\"; } // note\n```\n\n- [x] done\n> quote\n\u{202e}hidden\n";

fn screen(name: &str, skin: Skin, bg: Option<Rgb>, size: (u16, u16), keys: &str) -> Screen {
    let text = format!("a.md={MIXED}");
    let mut r = rig_env(name, &[&text, "b.rs=fn b() {}\n"], &["a.md"], size, |e| {
        e.skin = skin;
        e.syn = Syntax::new(bg.is_some());
        e.chrome = true;
    });
    if let Some(bg) = bg {
        r.send(SimEvent::Term(Event::Bg(bg)));
    }
    r.keys(keys);
    r.shot.clone()
}

#[test]
fn gui_3_the_resolver_s_cells_are_the_ones_the_painter_writes_in_every_look() {
    for (skin, name) in [(Skin::Depth, "depth"), (Skin::Flat, "flat"), (Skin::Calm, "calm")] {
        for bg in [DARK, LIGHT] {
            let s = screen(&format!("g3-{name}"), skin, Some(bg), (100, 30), "");
            resolver_agrees_with_the_painter(&s, &format!("{name} {bg:?}"));
        }
    }
    // with the tree and its cursor, a selection, the fog of a scrolled file, a window over it all
    let s = screen("g3-tree", Skin::Depth, Some(DARK), (100, 30), "<C-n>");
    resolver_agrees_with_the_painter(&s, "tree");
    let s = screen("g3-visual", Skin::Depth, Some(DARK), (100, 30), "Vjj");
    resolver_agrees_with_the_painter(&s, "visual");
    let s = screen("g3-picker", Skin::Depth, Some(DARK), (100, 30), "<C-p>b");
    resolver_agrees_with_the_painter(&s, "picker");
    let s = screen("g3-find", Skin::Depth, Some(DARK), (100, 30), "/it");
    resolver_agrees_with_the_painter(&s, "find");
    // a terminal whose palette is not known: attributes and the terminal's own colors
    let s = screen("g3-nopal", Skin::Depth, None, (100, 30), "");
    assert!(s.frame.pal.is_none());
    resolver_agrees_with_the_painter(&s, "no palette");
}

#[test]
fn gui_3_the_resolver_has_a_cell_for_each_column_a_wide_char_has_two_the_same() {
    let s = screen("g3-wide", Skin::Depth, Some(DARK), (100, 30), "");
    let mut res = Resolver::new();
    let y = s.frame.rows.iter().position(|r| r.text.contains("日本語")).unwrap();
    let row = res.row(&s, y);
    let at = s.frame.rows[y].text.chars().take_while(|&c| c != '日').count();
    // (the row's columns before it: all narrow)
    assert_eq!(row[at], row[at + 1], "the two columns of a wide char are the same cell");
    assert_eq!(row[at + 2], row[at + 3]);
    // a screen that changed its palette is resolved with the new one
    let mut other = s.clone();
    other.frame.pal = Some(palette(LIGHT));
    let before = res.row(&s, y);
    let after = res.row(&other, y);
    assert_ne!(
        before.iter().filter_map(|c| c.fg).collect::<Vec<_>>(),
        after.iter().filter_map(|c| c.fg).collect::<Vec<_>>()
    );
}
