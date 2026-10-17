//! UI-1, UI-12: the accents, the blocks and the neutral surfaces of the interface (pure).
#![allow(clippy::unwrap_used)] // the tests themselves may unwrap
use si::core::color::{Rgb, apca, palette};
use sim::color::{self, Accent, Syntax, Tones, to_lch};

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

fn hex((r, g, b): Rgb) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

fn gap(a: f64, b: f64) -> f64 {
    let d = (a - b).abs();
    d.min(360.0 - d)
}

const ACCENTS: [(Accent, &str, f64); 5] = [
    (Accent::Red, "red", 25.0),
    (Accent::Green, "green", 145.0),
    (Accent::Amber, "amber", 85.0),
    (Accent::Blue, "blue", 250.0),
    (Accent::Purple, "purple", 320.0),
];

#[test]
fn ui_12_text_keeps_75_on_every_surface_and_on_its_block_on_every_background() {
    let (mut worst_primary, mut worst_accent, mut worst_block) =
        (f64::INFINITY, f64::INFINITY, f64::INFINITY);
    for bg in BGS {
        let (p, t) = (palette(bg), Tones::new(&palette(bg)));
        let surfaces = [
            ("page", bg),
            ("cursor", t.cursor),
            ("band", t.band),
            ("bar", t.bar),
            ("code", t.code),
            ("card", t.card),
        ];
        for (name, s) in surfaces {
            let lc = apca(p.primary, s).abs();
            assert!(lc >= 75.0, "{}: primary on the {name} surface |Lc| {lc:.1}", hex(bg));
            worst_primary = worst_primary.min(lc);
            for (a, an, _) in ACCENTS {
                let lc = apca(t.fg(a), s).abs();
                assert!(
                    lc >= 75.0,
                    "{}: {an} {} on the {name} surface |Lc| {lc:.1}",
                    hex(bg),
                    hex(t.fg(a))
                );
                worst_accent = worst_accent.min(lc);
            }
        }
        // The neutral block of the mode in Normal mode: text that reads on it, and a block that
        // stands out from the bar and the branch's surface it sits beside.
        let lc = apca(t.gray_on, t.gray).abs();
        assert!(lc >= 75.0, "{}: text on the gray block |Lc| {lc:.1}", hex(bg));
        worst_block = worst_block.min(lc);
        let gap = (to_lch(t.gray)[0] - to_lch(t.card)[0]).abs();
        assert!(
            gap > 0.03 || bg == (0, 0, 0),
            "{}: the gray block is {gap:.3} of lightness from the card",
            hex(bg)
        );
        for (a, an, _) in ACCENTS {
            let lc = apca(t.on(a), t.block(a)).abs();
            assert!(
                lc >= 75.0,
                "{}: text {} on the {an} block {} |Lc| {lc:.1}",
                hex(bg),
                hex(t.on(a)),
                hex(t.block(a))
            );
            worst_block = worst_block.min(lc);
        }
    }
    eprintln!(
        "UI-12 lowest |Lc|: primary on a surface {worst_primary:.1}, an accent on a surface {worst_accent:.1}, text on a block {worst_block:.1}"
    );
}

#[test]
fn ui_1_every_accent_keeps_the_hue_of_its_class_and_its_block_has_it_too() {
    for bg in BGS {
        let t = Tones::new(&palette(bg));
        for (a, name, hue) in ACCENTS {
            let [_, chroma, h] = to_lch(t.fg(a));
            assert!(gap(h, hue) < 14.0, "{} {name}: hue {h:.0} of {}", hex(bg), hex(t.fg(a)));
            assert!(chroma > 0.02, "{} {name}: chroma {chroma:.3}", hex(bg));
            let [_, chroma, h] = to_lch(t.block(a));
            assert!(
                gap(h, hue) < 14.0,
                "{} {name} block: hue {h:.0} of {}",
                hex(bg),
                hex(t.block(a))
            );
            assert!(chroma > 0.02, "{} {name} block: chroma {chroma:.3}", hex(bg));
            let lc = apca(t.block(a), bg).abs();
            assert!(lc >= 15.0, "{} {name} block: |Lc| {lc:.1} against the page", hex(bg));
        }
    }
}

#[test]
fn ui_1_the_reply_of_the_terminal_moves_an_accent_and_blue_is_never_asked_for() {
    let bg = (0x1e, 0x1e, 0x1e);
    let before = Tones::new(&palette(bg));
    let mut syn = Syntax::new(true);
    syn.bg(bg);
    syn.reply("4;3;rgb:e0/a0/20"); // the terminal's own yellow
    syn.reply("4;4;rgb:20/60/e0"); // and a blue, if it volunteers one
    let after = Tones::with(&palette(bg), &syn);
    assert_ne!(after.fg(Accent::Amber), before.fg(Accent::Amber), "amber follows the reply");
    assert!(gap(to_lch(after.fg(Accent::Amber))[2], to_lch((0xe0, 0xa0, 0x20))[2]) < 6.0);
    assert!(gap(to_lch(after.fg(Accent::Blue))[2], to_lch((0x20, 0x60, 0xe0))[2]) < 6.0);
    assert!(
        !color::QUERY.contains("4;4;"),
        "blue is derived, not asked for: a quit waits for no more replies"
    );
}

#[test]
fn ui_1_the_surfaces_are_steps_from_the_page_toward_secondary_and_none_is_above_the_card() {
    for bg in BGS {
        let (p, t) = (palette(bg), Tones::new(&palette(bg)));
        // (APCA reads two near-blacks as the same: the steps are measured in OKLab lightness)
        let dist = |c: Rgb| (to_lch(c)[0] - to_lch(bg)[0]).abs();
        let steps = [t.cursor, t.band, t.bar, t.code, t.card].map(dist);
        assert!(steps.windows(2).all(|w| w[0] <= w[1]), "{}: {steps:?} are in order", hex(bg));
        assert!(steps[4] > steps[0], "{}: the card is above the cursor's line", hex(bg));
        // (on a pure black page 6% of the way is under one level of eight bits: it is the page)
        assert!(
            steps[0] > 0.005 || bg == (0, 0, 0),
            "{}: the cursor's line is a surface: {steps:?}",
            hex(bg)
        );
        assert!(dist(t.card) < dist(p.secondary), "{}: the card is below secondary", hex(bg));
        assert_eq!(t.card, Tones::new(&p).card, "the card is LOOK-5's");
    }
}

#[test]
fn ui_6_the_colorcolumn_and_the_handle_are_steps_from_the_page_the_author_s_scale_gives() {
    use sim::color::{scale, stepped};
    // His scale_hex: each channel times the factor, clipped (a dark page); divided on a light one.
    assert_eq!(scale((0x16, 0x17, 0x19), 1.18), (26, 27, 30));
    assert_eq!(scale((0x16, 0x17, 0x19), 1.8), (40, 41, 45));
    assert_eq!(scale((250, 250, 250), 1.18), (212, 212, 212), "a light page goes darker");
    assert_eq!(scale((250, 250, 250), 1.8), (139, 139, 139));
    for bg in BGS {
        let t = Tones::new(&palette(bg));
        let l = |c: Rgb| to_lch(c)[0];
        let (cc, thumb) = ((l(t.cc) - l(bg)).abs(), (l(t.thumb) - l(bg)).abs());
        assert!(cc >= 0.019 && thumb >= 0.069, "{}: steps of {cc:.3} and {thumb:.3}", hex(bg));
        assert!(thumb > cc, "{}: the handle is a bigger step than the column", hex(bg));
        let dark = l(bg) < 0.6;
        assert_eq!(l(t.cc) > l(bg), dark, "{}: away from the page", hex(bg));
        assert_eq!(l(t.thumb) > l(bg), dark, "{}", hex(bg));
    }
    // Made of the page and the gray, so a tinted page's tint is not scaled into a hue.
    let solarized = stepped((0x00, 0x2b, 0x36), (0x93, 0xa1, 0xa1), 1.8, 0.07);
    assert!(to_lch(solarized)[1] <= to_lch((0x00, 0x2b, 0x36))[1] + 0.01);
}
