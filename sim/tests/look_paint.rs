//! LOOK-2, LOOK-6, LOOK-9: the colors the looks add to si's palette (pure), and how the Painter lays
//! them on: a tint behind the cells that have the page behind them, a fade of the foreground.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
use si::core::color::{Rgb, apca, palette};
use si::core::view::{Frame, Row, Shape, Style};
use sim::color::{Syntax, Tones, contrast, fade, fade_to, mix};
use sim::paint::{Look, Paint, Painter, Screen};

/// VIS-4's backgrounds.
const BGS: [Rgb; 8] = [
    (0x00, 0x00, 0x00),
    (0x1e, 0x1e, 0x1e),
    (0x28, 0x2c, 0x34),
    (0x00, 0x2b, 0x36),
    (0xff, 0xff, 0xff),
    (0xfa, 0xfa, 0xfa),
    (0xfd, 0xf6, 0xe3),
    (0xef, 0xf1, 0xf5),
];
const DARK: Rgb = (0x1e, 0x1e, 0x1e);

fn hex((r, g, b): Rgb) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

#[test]
fn look_9_primary_text_reads_on_every_tint_and_on_the_card_on_every_background() {
    let mut worst = f64::INFINITY;
    for bg in BGS {
        let (p, t) = (palette(bg), Tones::new(&palette(bg)));
        for (name, tint) in [("gain", t.gain), ("loss", t.loss), ("card", t.card)] {
            let lc = apca(p.primary, tint).abs();
            assert!(lc >= 75.0, "{}: primary on the {name} tint |Lc| {lc:.1}", hex(bg));
            worst = worst.min(lc);
        }
    }
    eprintln!("LOOK-9 lowest |Lc| of primary on any tint or the card: {worst:.1}");
}

/// The owner of sim chose green for what comes and red for what goes in flat and depth, and said to
/// let color vision go: each is found the way si's change color is (the most chromatic color of its
/// hue that is the nearest to the page at |Lc| 15 and the margin away from it), without the
/// simulations of VIS-5, and has its own text gray at |Lc| 75.
#[test]
fn look_2_green_and_red_are_found_like_si_s_change_color_and_the_text_on_them_reads() {
    for bg in BGS {
        let (p, t) = (palette(bg), Tones::new(&palette(bg)));
        for (name, c, text, hue) in
            [("green", t.add, t.add_text, 150.0), ("red", t.del, t.del_text, 25.0)]
        {
            let [_, chroma, h] = sim::color::to_lch(c);
            assert!((h - hue).abs() < 4.0, "{} {name}: hue {h:.1} of {}", hex(bg), hex(c));
            assert!(chroma > 0.03, "{} {name}: chroma {chroma:.3}", hex(bg));
            let lc = apca(c, bg).abs();
            assert!(lc >= 15.0, "{} {name} {}: |Lc| {lc:.1} against the page", hex(bg), hex(c));
            let on = apca(text, c).abs();
            assert!(on >= 75.0, "{} {name}: {} on {} |Lc| {on:.1}", hex(bg), hex(text), hex(c));
            assert!(text.0 == text.1 && text.1 == text.2, "{} {name}: the text is a gray", hex(bg));
            // On primary's side of the page: light text on a dark page, dark on a light one.
            assert_eq!(apca(p.primary, bg) < 0.0, text.0 > 128, "{} {name}", hex(bg));
        }
        // The two are not the same color, and the tints are paler forms of them.
        assert_ne!(t.add, t.del);
        assert!(apca(t.gain, bg).abs() < apca(t.add, bg).abs(), "{}", hex(bg));
        // (on black both are as near to the page as APCA tells apart: 0 and 0)
    }
}

/// The values of the mockups' own arithmetic (pal.py's `mix`, an OKLab mix whose result is clipped
/// to sRGB) and of a port of si's search for a color of a given hue, worked out for two of VIS-4's
/// backgrounds.
#[test]
fn look_2_the_tones_are_oklab_mixes_from_the_pages_colors() {
    let t = Tones::new(&palette(DARK));
    assert_eq!((t.add, t.add_text), ((0, 102, 44), (237, 237, 237)), "green, hue 150");
    assert_eq!((t.del, t.del_text), ((168, 0, 25), (237, 237, 237)), "red, hue 25");
    assert_eq!(t.gain, (32, 51, 36), "30% toward green");
    assert_eq!(t.loss, (71, 34, 32), "30% toward red");
    assert_eq!(t.card, (54, 54, 54), "18% toward secondary");
    assert_eq!(t.shadow, (4, 4, 4), "55% toward black");
    assert_eq!(t.line, (92, 92, 92), "45% toward secondary");
    assert_eq!(t.dim, (123, 123, 123), "secondary, 35% toward the page");
    assert_eq!((t.focus, t.primary), (palette(DARK).secondary, palette(DARK).primary));
    let t = Tones::new(&palette((255, 255, 255)));
    assert_eq!((t.add, t.add_text), ((58, 255, 129), (64, 64, 64)));
    assert_eq!((t.del, t.del_text), ((255, 214, 210), (62, 62, 62)));
    assert_eq!((t.gain, t.loss, t.card), ((212, 255, 219), (255, 243, 241), (234, 234, 234)));
    assert_eq!((t.shadow, t.line, t.dim), ((85, 85, 85), (203, 203, 203), (180, 180, 180)));
    assert_eq!(mix(DARK, (0, 94, 164), 0.0), DARK);
    assert_eq!(mix(DARK, (0, 94, 164), 1.0), (0, 94, 164));
}

#[test]
fn look_6_a_fade_goes_toward_the_page_and_never_takes_text_below_lc_75() {
    for bg in BGS {
        let p = palette(bg);
        // A syntax color is a hue moved until it reads: the same clamp holds for it.
        let hue = contrast((200, 60, 60), bg, 75.0);
        for fg in [p.primary, p.secondary, hue] {
            assert_eq!(fade(fg, bg, 0.0), fg, "{}: no fade changes nothing", hex(bg));
            let was = apca(fg, bg).abs();
            let mut last = was;
            for i in 0..=20 {
                let f = f64::from(i) / 20.0;
                let now = apca(fade(fg, bg, f), bg).abs();
                assert!(
                    now >= 75.0f64.min(was) - 1e-9,
                    "{} {}: f {f} |Lc| {now:.1}",
                    hex(bg),
                    hex(fg)
                );
                assert!(
                    now <= last + 3.0 || fg == hue,
                    "{} {}: f {f} moves away from the page",
                    hex(bg),
                    hex(fg)
                );
                last = now;
            }
        }
        // Gray text on a gray page: a plain mix until it would go under 75, and then it stops.
        let (far, near) = (fade(p.primary, bg, 0.02), fade(p.primary, bg, 1.0));
        assert_eq!(far, mix(p.primary, bg, 0.02), "{}: below the clamp it is the mix", hex(bg));
        assert!(apca(near, bg).abs() >= 75.0, "{}: all the way is clamped", hex(bg));
        assert_ne!(near, bg);
    }
    // Text that is already under its floor is left as it is (a fade must not push it the other way).
    let dim = mix((0x55, 0x55, 0x55), DARK, 0.5);
    assert_eq!(fade(dim, DARK, 0.5), dim);
    // The mockup's number for primary on #1e1e1e, 45% of the way, is 132 and reads at Lc 35: the
    // clamp stops short of it, within the bisection's step of the Lc 75 line.
    let (p, held) = (palette(DARK).primary, fade(palette(DARK).primary, DARK, 0.45));
    assert_eq!(fade(p, DARK, 0.05), mix(p, DARK, 0.05), "where it reads, it is the mix");
    assert!(held.0 > 132 && (75.0..78.0).contains(&apca(held, DARK).abs()), "{held:?}");
}

/// The SGR parameters written just before the char `ch`.
fn sgr_before(out: &str, ch: char) -> String {
    let found = out.split('\x1b').find_map(|p| {
        let (params, text) = p.strip_prefix('[')?.split_once('m')?;
        text.starts_with(ch).then(|| params.to_string())
    });
    assert!(found.is_some(), "no {ch:?} in {out:?}");
    found.unwrap()
}

/// A one-row screen on the palette of `DARK`, with the style spans and the paints given.
fn screen(
    text: &str,
    spans: Vec<(std::ops::Range<u16>, Style)>,
    paints: Vec<(std::ops::Range<u16>, Paint)>,
) -> Screen {
    let pal = Some(palette(DARK));
    let width = text.chars().count() as u16;
    let row = Row { text: text.into(), spans, underline: vec![] };
    let rows = vec![row, Row::default()];
    let frame = Frame { width, height: 2, rows, cursor: (0, 0), shape: Shape::Block, pal };
    Screen {
        frame,
        looks: vec![vec![], vec![]],
        paints: vec![paints, vec![]],
        syn: Syntax::default(),
    }
}

fn draw(s: Screen) -> String {
    let mut p = Painter::new(Vec::<u8>::new());
    p.draw(s).unwrap();
    String::from_utf8(p.out).unwrap()
}

fn rgb((r, g, b): Rgb) -> String {
    format!("{r};{g};{b}")
}

#[test]
fn look_2_the_painter_lays_the_tint_and_the_inks_over_the_cells_the_first_paint_names() {
    let t = Tones::new(&palette(DARK));
    let tint = Paint { bg: Some(t.gain), ..Paint::default() };
    let green = Paint { bg: Some(t.add), fg: Some(t.add_text), ..Paint::default() };
    let red = Paint { bg: Some(t.del), ..Paint::default() };
    // As `layout` makes them: the words' own paints ahead of the tint of the row.
    let paints = vec![(1..2, green), (3..4, red), (0..5, tint)];
    let s = screen("ab cd", vec![(1..2, Style::Change)], paints);
    let out = draw(s);
    let on = |c: Rgb| format!(";48;2;{}", rgb(c));
    assert!(sgr_before(&out, 'a').contains(&on(t.gain)), "a plain cell: the tint");
    assert!(sgr_before(&out, ' ').contains(&on(t.gain)), "a blank one too");
    // The change style is si's blue; the ink after it puts green, and the text gray, over it.
    let b = sgr_before(&out, 'b');
    assert!(b.contains(&on(t.add)) && b.contains(&format!(";38;2;{}", rgb(t.add_text))), "{b}");
    assert!(!b.contains(&on(t.gain)), "{b}");
    // A word of the plain style with red behind it keeps its own text color.
    let c = sgr_before(&out, 'c');
    let ink = format!("38;2;{}", rgb(palette(DARK).primary));
    assert!(c.contains(&on(t.del)) && c.contains(&ink), "{c}");
    assert!(sgr_before(&out, 'd').contains(&on(t.gain)), "the tint is behind the rest");
}

#[test]
fn look_2_the_painter_puts_a_paints_own_colors_on_a_cell() {
    let t = Tones::new(&palette(DARK));
    let half = Paint { fg: Some(t.card), bg: Some(t.shadow), ..Paint::default() };
    let line = Paint { fg: Some(t.line), ..Paint::default() };
    let out = draw(screen("▀│", vec![], vec![(0..1, half), (1..2, line)]));
    let a = sgr_before(&out, '▀');
    assert!(
        a.ends_with(&format!(";48;2;{}", rgb(t.shadow)))
            || a.contains(&format!("48;2;{}", rgb(t.shadow))),
        "{a}"
    );
    assert!(a.contains(&format!("38;2;{}", rgb(t.card))), "{a}");
    assert!(sgr_before(&out, '│').contains(&format!("38;2;{}", rgb(t.line))));
}

#[test]
fn look_6_the_painter_fades_the_foreground_of_a_run_toward_the_page() {
    let p = palette(DARK);
    let half = Paint { fade: 50, ..Paint::default() };
    // A primary cell, a secondary one, a comment (secondary and italic), the change style (never faded).
    let mut s =
        screen("pqrc", vec![(1..2, Style::Secondary), (3..4, Style::Change)], vec![(0..4, half)]);
    s.looks[0].push((2..3, Look::Com));
    let out = draw(s);
    let at = |c: char| sgr_before(&out, c);
    assert!(at('p').contains(&format!("38;2;{}", rgb(fade(p.primary, DARK, 0.5)))), "{}", at('p'));
    // secondary text does not fade: it is rank, the fog is distance (COLOR-1)
    let gray = format!("38;2;{}", rgb(p.secondary));
    assert!(at('q').contains(&gray) && at('r').contains(&gray), "{} {}", at('q'), at('r'));
    let c = at('c');
    assert!(!c.contains(&rgb(fade(p.change_text, DARK, 0.5))), "{c}: the change style never fades");
}

#[test]
fn look_6_the_painter_fades_the_syntax_colors_too() {
    let mut syn = Syntax::new(true);
    syn.bg(DARK);
    for (i, c) in [
        (1, "rgb:c0/40/40"),
        (2, "rgb:40/c0/40"),
        (3, "rgb:c0/c0/40"),
        (5, "rgb:c0/40/c0"),
        (6, "rgb:40/c0/c0"),
    ] {
        syn.reply(&format!("4;{i};{c}"));
    }
    let hue = syn.hue(Look::Kw).unwrap();
    let mut s = screen("kw", vec![], vec![(0..2, Paint { fade: 40, ..Paint::default() })]);
    s.looks[0].push((0..2, Look::Kw));
    s.syn = syn;
    let out = draw(s);
    let k = sgr_before(&out, 'k');
    assert!(k.contains(&format!("38;2;{}", rgb(fade(hue, DARK, 0.4)))), "{k}");
}

#[test]
fn look_6_the_line_numbers_have_a_floor_of_their_own_at_lc_40_and_the_text_at_75() {
    for bg in BGS {
        let p = palette(bg);
        let (text, number) = (fade(p.primary, bg, 1.0), fade_to(p.primary, bg, 1.0, 40.0));
        let (lt, ln) = (apca(text, bg).abs(), apca(number, bg).abs());
        assert!((75.0..78.5).contains(&lt), "{}: text {lt:.1}", hex(bg));
        assert!((40.0..43.5).contains(&ln), "{}: a number {ln:.1}", hex(bg));
        assert_ne!(text, number);
        // what is under its floor already is left alone, whatever the floor
        let dim = mix(p.primary, bg, 0.9);
        assert_eq!(
            fade_to(dim, bg, 0.5, 40.0),
            if apca(dim, bg).abs() < 40.0 { dim } else { fade_to(dim, bg, 0.5, 40.0) }
        );
    }
    // the Painter: a paint with `soft` stops at 40, one without at 75
    let p = palette(DARK);
    let s = screen(
        "pq",
        vec![],
        vec![
            (0..1, Paint { fade: 100, ..Paint::default() }),
            (1..2, Paint { fade: 100, soft: true, ..Paint::default() }),
        ],
    );
    let out = draw(s);
    let ink = |c: Rgb| format!("38;2;{}", rgb(c));
    assert!(sgr_before(&out, 'p').contains(&ink(fade(p.primary, DARK, 1.0))), "{out:?}");
    assert!(sgr_before(&out, 'q').contains(&ink(fade_to(p.primary, DARK, 1.0, 40.0))), "{out:?}");
}

/// One row of `text` on the palette of `bg`: the spans given, and a fade of 100% (the farthest fog)
/// over all of it.
fn fogged(bg: Rgb, text: &str, spans: Vec<(std::ops::Range<u16>, Style)>, soft: bool) -> String {
    let width = text.chars().count() as u16;
    let row = Row { text: text.into(), spans, underline: vec![] };
    let frame = Frame {
        width,
        height: 2,
        rows: vec![row, Row::default()],
        cursor: (0, 0),
        shape: Shape::Block,
        pal: Some(palette(bg)),
    };
    let paints = vec![vec![(0..width, Paint { fade: 100, soft, ..Paint::default() })], vec![]];
    draw(Screen { frame, looks: vec![vec![], vec![]], paints, syn: Syntax::default() })
}

/// The foreground set just before the first `ch` of `out`.
fn ink_of(out: &str, ch: char) -> Rgb {
    let seq = sgr_before(out, ch);
    let rest = &seq[seq.rfind("38;2;").unwrap() + 5..];
    let v: Vec<u8> =
        rest.split(';').take(3).map(|n| n.trim_end_matches('m').parse().unwrap()).collect();
    (v[0], v[1], v[2])
}

#[test]
fn look_6_in_the_farthest_fog_primary_text_stays_15_lc_above_secondary_and_secondary_does_not_fade()
{
    for bg in BGS {
        let p = palette(bg);
        // a primary cell and a secondary one (what a comment, a done task, a marker is), faded all the way
        let out = fogged(bg, "ps", vec![(1..2, Style::Secondary)], false);
        let (primary, secondary) = (ink_of(&out, 'p'), ink_of(&out, 's'));
        assert_eq!(secondary, p.secondary, "{}: secondary text does not fade", hex(bg));
        let (lp, ls) = (apca(primary, bg).abs(), apca(secondary, bg).abs());
        println!("{}: primary {lp:.1}, secondary {ls:.1}", hex(bg));
        assert!(lp >= 75.0, "{}: primary text keeps body readability, Lc {lp:.1}", hex(bg));
        assert!(
            lp - ls >= 15.0 - 1e-9,
            "{}: primary {lp:.1} is not 15 above secondary {ls:.1}",
            hex(bg)
        );
        // a line number is secondary and fades, to its own floor
        let out = fogged(bg, "ps", vec![(1..2, Style::Secondary)], true);
        let number = ink_of(&out, 's');
        assert_ne!(number, p.secondary, "{}: a number fades", hex(bg));
        assert!(apca(number, bg).abs() >= 40.0 - 1e-9 || apca(p.secondary, bg).abs() < 40.0);
    }
}
