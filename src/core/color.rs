//! VIS-4, VIS-5
//! OKLCH, APCA, color vision simulation, computing the palette.

pub type Rgb = (u8, u8, u8);

/// The real colors for the Frame styles, computed from the terminal background.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    pub bg: Rgb,
    /// Text: |Lc| 90 against `bg`.
    pub primary: Rgb,
    /// Line numbers, unchanged lines in REVIEW, deleted words: |Lc| 60.
    pub secondary: Rgb,
    /// The one chromatic color (OKLCH h = 250), used only as a background.
    pub change: Rgb,
    /// Text on `change`: the gray nearest to `primary` with |Lc| 75 against `change`.
    pub change_text: Rgb,
}

/// The kinds of dichromacy simulated for VIS-5.
#[derive(Clone, Copy, Debug)]
pub enum Cvd {
    Protan,
    Deutan,
    Tritan,
}

const HUE: f64 = 250.0;

/// Inverse sRGB transfer curve (linear 0..1 to encoded 0..255). Used by OKLCH and the CVD matrices.
fn encode(x: f64) -> u8 {
    let x = x.clamp(0.0, 1.0);
    let v = if x <= 0.0031308 { 12.92 * x } else { 1.055 * x.powf(1.0 / 2.4) - 0.055 };
    (v * 255.0).round() as u8
}

fn decode(c: u8) -> f64 {
    let v = f64::from(c) / 255.0;
    if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
}

/// APCA-W3 0.0.98G-4g (SAPC-APCA): the luminance Y uses a plain 2.4 power, not the sRGB curve.
fn apca_y((r, g, b): Rgb) -> f64 {
    let p = |c: u8| (f64::from(c) / 255.0).powf(2.4);
    0.2126729 * p(r) + 0.7151522 * p(g) + 0.0721750 * p(b)
}

/// APCA-W3 0.0.98G-4g lightness contrast Lc (signed, as published: positive for dark text on light bg).
pub fn apca(text: Rgb, bg: Rgb) -> f64 {
    let soft = |y: f64| if y > 0.022 { y } else { y + (0.022 - y).powf(1.414) };
    let (t, b) = (soft(apca_y(text)), soft(apca_y(bg)));
    if (b - t).abs() < 0.0005 {
        return 0.0;
    }
    let (sapc, offset) = if b > t {
        ((b.powf(0.56) - t.powf(0.57)) * 1.14, -0.027)
    } else {
        ((b.powf(0.65) - t.powf(0.62)) * 1.14, 0.027)
    };
    if sapc.abs() < 0.1 { 0.0 } else { (sapc + offset) * 100.0 }
}

/// OKLCH to sRGB (Ottosson's OKLab matrices); `None` if outside the sRGB gamut.
pub fn oklch(l: f64, c: f64, h_deg: f64) -> Option<Rgb> {
    let (a, b) = (c * h_deg.to_radians().cos(), c * h_deg.to_radians().sin());
    let l_ = (l + 0.3963377774 * a + 0.2158037573 * b).powi(3);
    let m_ = (l - 0.1055613458 * a - 0.0638541728 * b).powi(3);
    let s_ = (l - 0.0894841775 * a - 1.2914855480 * b).powi(3);
    let lin = [
        4.0767416621 * l_ - 3.3077115913 * m_ + 0.2309699292 * s_,
        -1.2684380046 * l_ + 2.6097574011 * m_ - 0.3413193965 * s_,
        -0.0041960863 * l_ - 0.7034186147 * m_ + 1.7076147010 * s_,
    ];
    // Also false for NaN. The slack is below one 8-bit step.
    if !lin.iter().all(|v| (-1e-4..=1.0 + 1e-4).contains(v)) {
        return None;
    }
    Some((encode(lin[0]), encode(lin[1]), encode(lin[2])))
}

/// Machado, Oliveira & Fernandes (2009) simulation at severity 1.0 (their published matrices,
/// applied in linear RGB).
pub fn simulate(c: Rgb, kind: Cvd) -> Rgb {
    let m = match kind {
        Cvd::Protan => [
            [0.152286, 1.052583, -0.204868],
            [0.114503, 0.786281, 0.099216],
            [-0.003882, -0.048116, 1.051998],
        ],
        Cvd::Deutan => [
            [0.367322, 0.860646, -0.227968],
            [0.280085, 0.672501, 0.047413],
            [-0.011820, 0.042940, 0.968881],
        ],
        Cvd::Tritan => [
            [1.255528, -0.076749, -0.178779],
            [-0.078411, 0.930809, 0.147602],
            [0.004733, 0.691367, 0.303900],
        ],
    };
    let v = [decode(c.0), decode(c.1), decode(c.2)];
    let [r, g, b] = m.map(|row| encode(row[0] * v[0] + row[1] * v[1] + row[2] * v[2]));
    (r, g, b)
}

/// One OSC 11 channel: 1 to 4 hex digits, scaled to 8 bits (the X11 rule).
fn channel(p: &str) -> Option<u8> {
    if !(1..=4).contains(&p.len()) || !p.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let max = 16u32.pow(p.len() as u32) - 1;
    u8::try_from((u32::from_str_radix(p, 16).ok()? * 255 + max / 2) / max).ok()
}

/// Parses an OSC 11 reply body such as `rgb:1e1e/1e1e/1e1e` (1-4 hex digits per channel) or
/// `#rrggbb`; anything else is `None`.
pub fn parse_osc11(s: &str) -> Option<Rgb> {
    let parts = if let Some(h) = s.strip_prefix('#') {
        if h.len() != 6 {
            return None;
        }
        [h.get(0..2)?, h.get(2..4)?, h.get(4..6)?]
    } else {
        let mut it = s.strip_prefix("rgb:")?.split('/');
        let parts = [it.next()?, it.next()?, it.next()?];
        if it.next().is_some() {
            return None;
        }
        parts
    };
    Some((channel(parts[0])?, channel(parts[1])?, channel(parts[2])?))
}

/// The gray whose signed Lc against `bg` is closest to +target (dark text on a light bg) or
/// -target (light text on a dark bg), whichever side of `bg` reaches more contrast. When the
/// target is out of reach this ends at black or white.
fn gray(bg: Rgb, target: f64) -> Rgb {
    let signed = if apca((0, 0, 0), bg) >= -apca((255, 255, 255), bg) { target } else { -target };
    let miss = |g: u8| (apca((g, g, g), bg) - signed).abs();
    let g = (0..=255u8).min_by(|&x, &y| miss(x).total_cmp(&miss(y))).unwrap_or(0);
    (g, g, g)
}

/// VIS-4, VIS-5: the change background against bg, also after each color vision simulation.
const DISTINCT: f64 = 15.0;
/// VIS-4: the text on the change background.
const TEXT_ON_CHANGE: f64 = 75.0;
/// Extra Lc kept above the floors.
const MARGIN: f64 = 0.5;

/// Smallest |Lc| of `c` as text against `bg`, plain and under each color vision simulation.
fn distinct(c: Rgb, bg: Rgb) -> f64 {
    let sim = |k| apca(simulate(c, k), simulate(bg, k)).abs();
    [apca(c, bg).abs(), sim(Cvd::Protan), sim(Cvd::Deutan), sim(Cvd::Tritan)]
        .into_iter()
        .fold(f64::INFINITY, f64::min)
}

/// The most chromatic sRGB color at OKLCH lightness `l` and hue `HUE`
/// (bisection: along a fixed (l, h) the gamut is an interval starting at zero chroma).
fn most_chromatic(l: f64) -> Option<Rgb> {
    let (mut lo, mut hi) = (0.0, 0.4);
    for _ in 0..12 {
        let mid = (lo + hi) / 2.0;
        if oklch(l, mid, HUE).is_some() {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    oklch(l, lo, HUE)
}

/// The change background for `bg`, searched along the edge of the sRGB gamut at hue 250 (the
/// most chromatic color at each lightness step of 0.005, so chroma stays high). It must stay
/// at least 15 + MARGIN Lc away from bg, also under each color vision simulation (VIS-4, VIS-5).
/// Of what passes, the one nearest to bg wins: a calm, clearly visible tint, not a shout.
fn change(bg: Rgb) -> Rgb {
    let all = || (1..200).filter_map(|i| most_chromatic(f64::from(i) / 200.0));
    let calm = |&a: &Rgb, &b: &Rgb| apca(a, bg).abs().total_cmp(&apca(b, bg).abs());
    // Nothing passes (a mid-gray bg): take the most distinguishable one.
    let most = || all().max_by(|&a, &b| distinct(a, bg).total_cmp(&distinct(b, bg)));
    all().filter(|&c| distinct(c, bg) >= DISTINCT + MARGIN).min_by(calm).or_else(most).unwrap_or(bg)
}

/// The gray nearest to `primary`, on its side, with |Lc| of `TEXT_ON_CHANGE` + MARGIN against
/// `change`; if that is out of reach, black or white, whichever reads better on `change`.
fn change_text(bg: Rgb, primary: Rgb, change: Rgb) -> Rgb {
    let lc = |g: u8| apca((g, g, g), change).abs();
    let reach = |g: &u8| lc(*g) >= TEXT_ON_CHANGE + MARGIN;
    let g = if apca(primary, bg) < 0.0 {
        (primary.0..=255).find(reach).unwrap_or(255)
    } else {
        (0..=primary.0).rev().find(reach).unwrap_or(0)
    };
    let g = if lc(255 - g) > lc(g) && !reach(&g) { 255 - g } else { g };
    (g, g, g)
}

/// The palette for the terminal background `bg`.
pub fn palette(bg: Rgb) -> Palette {
    let (primary, change) = (gray(bg, 90.0), change(bg));
    let change_text = change_text(bg, primary, change);
    Palette { bg, primary, secondary: gray(bg, 60.0), change, change_text }
}
