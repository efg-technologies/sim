//! SYN-1, SYN-5
//! The terminal's palette (pure): OKLCH, APCA, and the replies to OSC 4, 10 and 11. This is a
//! minimal copy of what si's M5 (VIS-4) will have as `core::color`: unify the two then.
use crate::paint::Look;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub type Rgb = [u8; 3];

/// What the terminal is asked for at startup: its background and the five colors that carry
/// syntax (ANSI red, green, yellow, magenta, cyan), each ended with ST.
pub const QUERY: &str = "\x1b]11;?\x1b\\\x1b]4;1;?\x1b\\\x1b]4;2;?\x1b\\\x1b]4;3;?\x1b\\\x1b]4;5;?\x1b\\\x1b]4;6;?\x1b\\";

/// APCA-W3 0.0.98G-4g: the Lc of text `txt` on background `bg` (positive: dark on light).
pub fn lc(txt: Rgb, bg: Rgb) -> f64 {
    let y = |c: Rgb| {
        let [r, g, b] = c.map(|v| (f64::from(v) / 255.0).powf(2.4));
        let y = 0.2126729 * r + 0.7151522 * g + 0.0721750 * b;
        if y < 0.022 { y + (0.022 - y).powf(1.414) } else { y }
    };
    let (t, b) = (y(txt), y(bg));
    let s = if b > t { b.powf(0.56) - t.powf(0.57) } else { b.powf(0.65) - t.powf(0.62) } * 1.14;
    if (b - t).abs() < 0.0005 || s.abs() < 0.1 { 0.0 } else { (s - 0.027f64.copysign(s)) * 100.0 }
}

/// sRGB to OKLCH (Ottosson): lightness, chroma, hue in degrees.
pub fn to_lch(c: Rgb) -> [f64; 3] {
    let [r, g, b] = c.map(|v| {
        let v = f64::from(v) / 255.0;
        if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    });
    let l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b).cbrt();
    let m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b).cbrt();
    let s = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b).cbrt();
    let a = 1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s;
    let b = 0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s;
    [0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s, a.hypot(b), b.atan2(a).to_degrees()]
}

/// OKLCH to sRGB. A color outside the gamut loses chroma until it fits, keeping hue and lightness.
fn from_lch([l, c, h]: [f64; 3]) -> Rgb {
    let rgb = |c: f64| {
        let (a, b) = (c * h.to_radians().cos(), c * h.to_radians().sin());
        let [l_, m_, s_] = [
            l + 0.3963377774 * a + 0.2158037573 * b,
            l - 0.1055613458 * a - 0.0638541728 * b,
            l - 0.0894841775 * a - 1.2914855480 * b,
        ]
        .map(|v| v * v * v);
        [
            4.0767416621 * l_ - 3.3077115913 * m_ + 0.2309699292 * s_,
            -1.2684380046 * l_ + 2.6097574011 * m_ - 0.3413193965 * s_,
            -0.0041960863 * l_ - 0.7034186147 * m_ + 1.7076147010 * s_,
        ]
    };
    let mut c = c;
    while c > 1e-4 && !rgb(c).iter().all(|x| (-1e-4..=1.0001).contains(x)) {
        c *= 0.95;
    }
    rgb(c).map(|v| {
        let v = v.clamp(0.0, 1.0);
        (255.0 * if v <= 0.0031308 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 }).round()
            as u8
    })
}

/// `c` with its hue and chroma, its lightness moved away from `bg` (lighter on a dark one) in
/// steps of 0.01 until |Lc| against `bg` reaches `min`, or lightness runs out.
pub fn contrast(c: Rgb, bg: Rgb, min: f64) -> Rgb {
    let (mut lch, up) = (to_lch(c), lc([255; 3], bg).abs() >= lc([0; 3], bg).abs());
    let mut out = c;
    while lc(out, bg).abs() < min && (0.0..1.0).contains(&lch[0]) {
        lch[0] += if up { 0.01 } else { -0.01 };
        out = from_lch(lch);
    }
    out
}

/// `rgb:RRRR/GGGG/BBBB` (one to four hex digits a channel), as terminals answer.
fn parse(s: &str) -> Option<Rgb> {
    let mut it = s.strip_prefix("rgb:")?.split('/');
    let mut ch = || {
        let h = it.next().filter(|h| (1..=4).contains(&h.len()))?;
        let v = f64::from(u32::from_str_radix(h, 16).ok()?);
        Some((v / (16f64.powi(h.len() as i32) - 1.0) * 255.0).round() as u8)
    };
    let c = [ch()?, ch()?, ch()?];
    it.next().is_none().then_some(c)
}

/// The colors the terminal told, and what sim derives from them (SYN-1, SYN-5). Without
/// `truecolor` (the COLORTERM variable) nothing is derived: SGR 31-36 and underline stand in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Palette {
    pub truecolor: bool,
    bg: Option<Rgb>,
    ansi: [Option<Rgb>; 8],
    /// Red, green, yellow, magenta, cyan, moved until |Lc| >= 75 against the background.
    hues: [Option<Rgb>; 5],
    /// The selection's background and the text on it.
    sel: Option<(Rgb, Rgb)>,
}

impl Palette {
    pub fn new(truecolor: bool) -> Palette {
        Palette { truecolor, ..Palette::default() }
    }

    /// The payload of an OSC reply: `11;rgb:...` or `4;N;rgb:...`. Anything else is ignored.
    pub fn reply(&mut self, osc: &str) {
        match osc.splitn(3, ';').collect::<Vec<_>>()[..] {
            ["11", c] => self.bg = parse(c),
            ["4", i, c] => {
                if let Some(a) = i.parse::<usize>().ok().and_then(|i| self.ansi.get_mut(i)) {
                    *a = parse(c);
                }
            }
            _ => return,
        }
        let bg = self.bg.filter(|_| self.truecolor);
        self.hues = [1, 2, 3, 5, 6].map(|i| Some(contrast(self.ansi[i]?, bg?, 75.0)));
        // SYN-5: a neutral gray (lightness only) at |Lc| >= 15, and gray text on it at >= 75.
        self.sel = bg.map(|bg| {
            let gray = contrast(from_lch([to_lch(bg)[0], 0.0, 0.0]), bg, 15.0);
            (gray, contrast([128; 3], gray, 75.0))
        });
    }

    /// The text color of a syntax look, or none (then the terminal's own SGR 31-36 is used).
    pub fn hue(&self, look: Look) -> Option<Rgb> {
        let i = match look {
            Look::Kw => 0,
            Look::Str => 1,
            Look::Ty => 2,
            Look::Num => 3,
            Look::Fun => 4,
            _ => return None,
        };
        self.hues[i]
    }

    /// SYN-5: the selection's background and its text, if the terminal can show them.
    pub fn sel(&self) -> Option<(Rgb, Rgb)> {
        self.sel
    }
}

/// What the input thread does with a key while it may be part of an OSC reply (crossterm reads
/// `ESC ]` as Alt+`]`, the payload as plain keys, and `ESC \` or BEL as Alt+`\` or Ctrl-g):
/// `None` for an ordinary key, `Some(None)` for a key that is part of a reply being collected
/// (it never reaches the App), `Some(Some(payload))` when the reply is complete.
pub fn feed(buf: &mut Option<String>, k: &KeyEvent) -> Option<Option<String>> {
    let (alt, ctrl) =
        (k.modifiers.contains(KeyModifiers::ALT), k.modifiers.contains(KeyModifiers::CONTROL));
    match (buf.as_mut(), k.code) {
        (None, KeyCode::Char(']')) if alt => *buf = Some(String::new()),
        (Some(_), KeyCode::Char('\\' | 'g')) if alt || ctrl => return Some(buf.take()),
        (Some(s), KeyCode::Char(c)) if !alt && !ctrl && s.len() < 96 => s.push(c),
        _ => {
            *buf = None; // not a reply (any more)
            return None;
        }
    }
    Some(None)
}
