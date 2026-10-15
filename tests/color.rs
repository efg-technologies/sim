//! VIS-4 and VIS-5 for src/core/color.rs. Pure math, so nothing here touches the terminal.
use si::core::color::{Cvd, Rgb, apca, oklch, palette, parse_osc11, simulate};

/// The backgrounds VIS-4 names.
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
const KINDS: [Cvd; 3] = [Cvd::Protan, Cvd::Deutan, Cvd::Tritan];
/// The calmest the color vision filter lets the change be is about 17.3-19.1 on the 8 backgrounds.
const CALM_LC: f64 = 20.0;

// An independent inverse of `oklch` (Ottosson's published sRGB -> OKLab), to check it by round trip.
fn to_oklch((r, g, b): Rgb) -> (f64, f64, f64) {
    let lin = |c: u8| {
        let v = f64::from(c) / 255.0;
        if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    let (r, g, b) = (lin(r), lin(g), lin(b));
    let l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b).cbrt();
    let m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b).cbrt();
    let s = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b).cbrt();
    let big_l = 0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s;
    let a = 1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s;
    let b = 0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s;
    (big_l, a.hypot(b), b.atan2(a).to_degrees().rem_euclid(360.0))
}

fn hex((r, g, b): Rgb) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

fn is_gray((r, g, b): Rgb) -> bool {
    r == g && g == b
}

#[test]
fn vis_4_apca_matches_published_reference_values() {
    let (white, black, gray) = ((255, 255, 255), (0, 0, 0), (0x88, 0x88, 0x88));
    // APCA-W3 0.0.98G-4g: #888 on #fff 63.06, #fff on #888 -68.54, #000 on #fff 106.04, #fff on #000 -107.88.
    for (text, bg, lc) in [
        (gray, white, 63.06),
        (white, gray, -68.54),
        (black, white, 106.04),
        (white, black, -107.88),
    ] {
        let got = apca(text, bg);
        assert!((got - lc).abs() < 0.01, "{} on {}: {got:.4}, expected {lc}", hex(text), hex(bg));
    }
    assert_eq!(apca(gray, gray), 0.0);
    assert_eq!(apca((0x12, 0x34, 0x56), (0x12, 0x34, 0x56)), 0.0);
    // Colors that are too close, or whose Lc would be under the 0.1 clip, give exactly 0.
    assert_eq!(apca((0x80, 0x80, 0x80), (0x81, 0x81, 0x81)), 0.0);
}

#[test]
fn vis_4_oklch_agrees_with_published_colors_and_the_gamut() {
    // Ottosson / CSS Color 4: sRGB red is oklch(0.628 0.2577 29.23), blue is oklch(0.452 0.3132 264.05).
    let (l, c, h) = to_oklch((255, 0, 0));
    assert!((l - 0.62796).abs() < 1e-3 && (c - 0.25768).abs() < 1e-3 && (h - 29.23).abs() < 0.05);
    let (l, c, h) = to_oklch((0, 0, 255));
    assert!((l - 0.45201).abs() < 1e-3 && (c - 0.31321).abs() < 1e-3 && (h - 264.05).abs() < 0.05);

    assert_eq!(oklch(1.0, 0.0, 0.0), Some((255, 255, 255)));
    assert_eq!(oklch(0.0, 0.0, 0.0), Some((0, 0, 0)));
    assert_eq!(oklch(0.7, 0.4, 250.0), None, "far outside the gamut");
    assert_eq!(oklch(0.9, 0.2, 250.0), None, "light and chromatic");
    assert_eq!(oklch(f64::NAN, 0.1, 250.0), None);
    assert_eq!(oklch(0.5, f64::NAN, 250.0), None);

    // Round trip over a grid of 8-bit colors: oklch(to_oklch(c)) == c.
    for r in (0..=255).step_by(15) {
        for g in (0..=255).step_by(15) {
            for b in (0..=255).step_by(15) {
                let c = (r as u8, g as u8, b as u8);
                let (l, ch, h) = to_oklch(c);
                let back = oklch(l, ch, h).unwrap_or_else(|| panic!("{} left the gamut", hex(c)));
                let d = |x: u8, y: u8| x.abs_diff(y);
                assert!(
                    d(back.0, c.0) <= 1 && d(back.1, c.1) <= 1 && d(back.2, c.2) <= 1,
                    "{} came back as {}",
                    hex(c),
                    hex(back)
                );
            }
        }
    }
}

#[test]
fn vis_4_primary_and_secondary_are_grays_at_their_target_lc() {
    for bg in BGS {
        let p = palette(bg);
        assert_eq!(p.bg, bg);
        assert!(is_gray(p.primary) && is_gray(p.secondary), "{}: {p:?}", hex(bg));
        let (hi, lo) = (apca(p.primary, bg).abs(), apca(p.secondary, bg).abs());
        assert!((89.0..=91.0).contains(&hi), "{}: primary |Lc| {hi:.2}", hex(bg));
        assert!((59.0..=61.0).contains(&lo), "{}: secondary |Lc| {lo:.2}", hex(bg));
        // Light text on a dark background, dark text on a light one.
        let dark_bg = to_oklch(bg).0 < 0.5;
        for text in [p.primary, p.secondary] {
            assert_eq!(apca(text, bg) < 0.0, dark_bg, "{}: wrong side for {}", hex(bg), hex(text));
        }
    }
}

#[test]
fn vis_4_change_is_chromatic_at_hue_250_and_stands_out_from_the_background() {
    for bg in BGS {
        let c = palette(bg).change;
        let lc = apca(c, bg).abs();
        assert!(lc >= 15.0, "{}: change {} against bg |Lc| {lc:.2}", hex(bg), hex(c));
        let (l, chroma, hue) = to_oklch(c);
        // 8-bit rounding moves a and b by about 0.003, which is this many degrees at this chroma.
        let slack = (0.004 / chroma).to_degrees();
        assert!(
            (hue - 250.0).abs() <= slack,
            "{}: change {} has hue {hue:.1} (chroma {chroma:.3})",
            hex(bg),
            hex(c)
        );
        assert!(
            chroma >= 0.04,
            "{}: change {} is not visibly chromatic ({chroma:.3})",
            hex(bg),
            hex(c)
        );
        assert!(
            oklch(l, chroma, hue).is_some(),
            "{}: change {} is outside the gamut",
            hex(bg),
            hex(c)
        );
    }
}

#[test]
fn vis_4_text_on_change_reaches_75() {
    for bg in BGS {
        let p = palette(bg);
        let lc = apca(p.change_text, p.change).abs();
        assert!(
            lc >= 75.0,
            "{}: {} on change {} |Lc| {lc:.2}",
            hex(bg),
            hex(p.change_text),
            hex(p.change)
        );
        assert!(is_gray(p.change_text), "{}: {}", hex(bg), hex(p.change_text));
        // On primary's side (light text on a dark bg, dark on a light one), and as near to primary
        // as it can be: the next gray toward primary no longer reaches 75 + the 0.5 margin.
        let dark_bg = to_oklch(bg).0 < 0.5;
        let (t, pr) = (p.change_text.0, p.primary.0);
        assert!(
            if dark_bg { t >= pr } else { t <= pr },
            "{}: {} vs primary {}",
            hex(bg),
            hex(p.change_text),
            hex(p.primary)
        );
        if t != pr {
            let closer = if dark_bg { t - 1 } else { t + 1 };
            let lc = apca((closer, closer, closer), p.change).abs();
            assert!(lc < 75.5, "{}: {} would do, |Lc| {lc:.2}", hex(bg), hex(p.change_text));
        }
    }
}

#[test]
fn vis_4_change_is_a_calm_tint_not_a_shout() {
    // Just above the 15 + 0.5 floor, as far as the color vision filter allows.
    for bg in BGS {
        let lc = apca(palette(bg).change, bg).abs();
        assert!(lc < CALM_LC, "{}: change against bg |Lc| {lc:.2}", hex(bg));
    }
}

#[test]
fn vis_4_palette_is_total_and_deterministic() {
    // Every gray, including mid-grays where |Lc| 90 is out of reach (the text ends at black or white).
    for v in 0..=255u8 {
        let bg = (v, v, v);
        let p = palette(bg);
        assert_eq!(p, palette(bg));
        assert!(is_gray(p.primary) && is_gray(p.secondary));
        assert!(apca(p.primary, bg).abs() >= apca(p.secondary, bg).abs() - 1e-9);
        // change_text: gray, on primary's side, and the extreme if it cannot reach 75.
        let t = p.change_text;
        assert!(is_gray(t));
        let light = apca(p.primary, bg) < 0.0;
        assert!(if light { t.0 >= p.primary.0 } else { t.0 <= p.primary.0 }, "{bg:?}: {p:?}");
        assert!(
            apca(t, p.change).abs() >= 75.5 || t.0 == if light { 255 } else { 0 },
            "{bg:?}: {p:?}"
        );
    }
    // A few colored backgrounds, and the extremes of each channel.
    for bg in [(255, 0, 0), (0, 255, 0), (0, 0, 255), (255, 255, 0), (12, 34, 56), (200, 100, 50)] {
        assert_eq!(palette(bg), palette(bg));
    }
}

#[test]
fn vis_4_palette_table_for_eyeballing() {
    // Run with `cargo test vis_4_palette_table -- --nocapture`.
    println!(
        "bg       primary (Lc)     secondary (Lc)   change   chg/bg  chroma  hue    change_text (Lc on change)  min CVD"
    );
    for bg in BGS {
        let p = palette(bg);
        let (_, chroma, hue) = to_oklch(p.change);
        let sim = |k| apca(simulate(p.change, k), simulate(bg, k));
        let min_cvd = KINDS.iter().map(|&k| sim(k).abs()).fold(f64::INFINITY, f64::min);
        println!(
            "{:<8} {} {:+7.2}  {} {:+7.2}  {}  {:+6.2}  {chroma:.3}  {hue:.1}  {} {:+7.2}  {min_cvd:.2}",
            hex(bg),
            hex(p.primary),
            apca(p.primary, bg),
            hex(p.secondary),
            apca(p.secondary, bg),
            hex(p.change),
            apca(p.change, bg),
            hex(p.change_text),
            apca(p.change_text, p.change),
        );
        for k in KINDS {
            println!(
                "           {k:?}: change {} bg {} Lc {:+.2}",
                hex(simulate(p.change, k)),
                hex(simulate(bg, k)),
                sim(k)
            );
        }
    }
}

#[test]
fn vis_4_parse_osc11_reads_one_to_four_hex_digits_per_channel() {
    for (reply, want) in [
        ("rgb:1e1e/1e1e/1e1e", (30, 30, 30)),
        ("rgb:fdfd/f6f6/e3e3", (253, 246, 227)),
        ("rgb:0000/2b2b/3636", (0, 43, 54)),
        ("rgb:ffff/ffff/ffff", (255, 255, 255)),
        ("rgb:0000/0000/0000", (0, 0, 0)),
        ("rgb:8080/8080/8080", (128, 128, 128)),
        ("rgb:1e/1e/1e", (30, 30, 30)),
        ("rgb:FD/F6/E3", (253, 246, 227)),
        ("rgb:f/f/f", (255, 255, 255)),
        ("rgb:0/0/0", (0, 0, 0)),
        ("rgb:1/2/3", (17, 34, 51)),
        ("rgb:fff/000/80", (255, 0, 128)),
        ("rgb:ff/ffff/f", (255, 255, 255)),
        ("#1e1e1e", (30, 30, 30)),
        ("#FDF6E3", (253, 246, 227)),
        ("#000000", (0, 0, 0)),
    ] {
        assert_eq!(parse_osc11(reply), Some(want), "{reply}");
    }
}

#[test]
fn vis_4_parse_osc11_rejects_anything_else() {
    for reply in [
        "",
        "garbage",
        "rgb:",
        "rgb:1e1e/1e1e",
        "rgb:1e1e/1e1e/1e1e/1e1e",
        "rgb:1e1e1e/0/0",
        "rgb:12345/0/0",
        "rgb:/0/0",
        "rgb:0//0",
        "rgb:zz/00/00",
        "rgb:+1/+1/+1",
        "rgb:-1/00/00",
        "rgb: 1/1/1",
        "rgb:1e1e,1e1e,1e1e",
        "rgba:1e1e/1e1e/1e1e/ffff",
        "#",
        "#fff",
        "#12345",
        "#1234567",
        "#12345g",
        "#+12345",
        "# 12345",
        "1e1e1e",
        " #1e1e1e",
        "#1e1e1e ",
        "#1e1e1e\x07",
        "rgb:1e1e/1e1e/1e1e\x1b\\",
        // Multi-byte characters, including ones that put a byte offset inside a character.
        "#\u{e9}\u{e9}\u{e9}",
        "#a\u{e9}12f",
        "rgb:\u{e9}/00/00",
    ] {
        assert_eq!(parse_osc11(reply), None, "{reply:?}");
    }
}

#[test]
fn vis_5_change_survives_every_color_vision_simulation() {
    for bg in BGS {
        let c = palette(bg).change;
        for k in KINDS {
            let (sc, sb) = (simulate(c, k), simulate(bg, k));
            let lc = apca(sc, sb).abs();
            assert!(
                lc >= 15.0,
                "{} {k:?}: change {} -> {} against bg -> {}: |Lc| {lc:.2}",
                hex(bg),
                hex(c),
                hex(sc),
                hex(sb)
            );
        }
    }
}

#[test]
fn vis_5_simulation_keeps_grays_gray_and_changes_saturated_colors() {
    for v in (0..=255u8).step_by(5).chain([255]) {
        for k in KINDS {
            let (r, g, b) = simulate((v, v, v), k);
            assert!(
                r.abs_diff(v) <= 1 && g.abs_diff(v) <= 1 && b.abs_diff(v) <= 1,
                "{k:?} gray {v}: {:?}",
                (r, g, b)
            );
        }
    }
    for k in KINDS {
        for c in [(255, 0, 0), (0, 255, 0), (0, 0, 255)] {
            assert_ne!(simulate(c, k), c, "{k:?} {c:?}");
        }
    }
}

#[test]
fn vis_5_simulation_uses_the_published_matrices_in_linear_rgb() {
    // Machado, Oliveira & Fernandes (2009), Table 1, severity 1.0 (rows: output r, g, b).
    let tables: [(Cvd, [[f64; 3]; 3]); 3] = [
        (
            Cvd::Protan,
            [
                [0.152286, 1.052583, -0.204868],
                [0.114503, 0.786281, 0.099216],
                [-0.003882, -0.048116, 1.051998],
            ],
        ),
        (
            Cvd::Deutan,
            [
                [0.367322, 0.860646, -0.227968],
                [0.280085, 0.672501, 0.047413],
                [-0.011820, 0.042940, 0.968881],
            ],
        ),
        (
            Cvd::Tritan,
            [
                [1.255528, -0.076749, -0.178779],
                [-0.078411, 0.930809, 0.147602],
                [0.004733, 0.691367, 0.303900],
            ],
        ),
    ];
    let encode = |x: f64| {
        let x = x.clamp(0.0, 1.0);
        let v = if x <= 0.0031308 { 12.92 * x } else { 1.055 * x.powf(1.0 / 2.4) - 0.055 };
        (v * 255.0).round() as u8
    };
    for (kind, m) in tables {
        // Rows of a published matrix sum to 1 (white stays white).
        for row in m {
            assert!((row.iter().sum::<f64>() - 1.0).abs() < 1e-5, "{kind:?} {row:?}");
        }
        // A pure primary in linear RGB is one column of the matrix.
        for (col, input) in [(0, (255, 0, 0)), (1, (0, 255, 0)), (2, (0, 0, 255))] {
            let want = (encode(m[0][col]), encode(m[1][col]), encode(m[2][col]));
            assert_eq!(simulate(input, kind), want, "{kind:?} column {col}");
        }
    }
    // Linear, not gamma: protan red is (109, 95, 0) here; the same matrix on encoded values would give r = 39.
    assert_eq!(simulate((255, 0, 0), Cvd::Protan), (109, 95, 0));
}
