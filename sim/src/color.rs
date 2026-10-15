//! SYN-1, SYN-5
//! The syntax colors (pure): the terminal's own ANSI colors (OSC 4), moved in OKLCH lightness until
//! they read against the background that si's palette is made from (`Event::Bg`, VIS-4), the
//! selection's gray, and what puts the terminal's replies together out of the key stream. APCA,
//! OKLCH to sRGB and the parsing of a reply are si's (`core::color`).
use crate::paint::Look;
use crate::sim::SimEvent;
use crossterm::event::{Event as Ev, KeyCode, KeyModifiers};
use si::core::color::{Rgb, apca, oklch, parse_osc11};
use si::io::term::Reply;
use std::time::{Duration, Instant};

/// What is asked of the terminal besides its background (which si's `term::enter` asks, VIS-4): the
/// five ANSI colors that carry syntax (red, green, yellow, magenta, cyan), each ended with ST.
pub const QUERY: &str =
    "\x1b]4;1;?\x1b\\\x1b]4;2;?\x1b\\\x1b]4;3;?\x1b\\\x1b]4;5;?\x1b\\\x1b]4;6;?\x1b\\";

/// sRGB to OKLCH (Ottosson): lightness, chroma, hue in degrees.
pub fn to_lch((r, g, b): Rgb) -> [f64; 3] {
    let [r, g, b] = [r, g, b].map(|v| {
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

/// OKLCH to sRGB (si's `oklch`). A color outside the gamut loses chroma until it fits, keeping
/// hue and lightness; a lightness past the ends is black or white.
fn from_lch([l, c, h]: [f64; 3]) -> Rgb {
    let mut shrinking = (0..).map(|i| c * 0.95f64.powi(i)).take_while(|&c| c > 1e-4);
    let fit = shrinking.find_map(|c| oklch(l, c, h)).or_else(|| oklch(l, 0.0, h));
    fit.unwrap_or(if l > 0.5 { (255, 255, 255) } else { (0, 0, 0) })
}

/// `c` with its hue and chroma, its lightness moved away from `bg` (lighter on a dark one) in
/// steps of 0.01 until |Lc| against `bg` reaches `min`, or lightness runs out.
pub fn contrast(c: Rgb, bg: Rgb, min: f64) -> Rgb {
    let (mut lch, up) = (to_lch(c), apca((255, 255, 255), bg).abs() >= apca((0, 0, 0), bg).abs());
    let mut out = c;
    while apca(out, bg).abs() < min && (0.0..1.0).contains(&lch[0]) {
        lch[0] += if up { 0.01 } else { -0.01 };
        out = from_lch(lch);
    }
    out
}

/// The colors sim derives from what the terminal told (SYN-1, SYN-5), beside si's palette. Without
/// `truecolor` (the COLORTERM variable) nothing is derived: SGR 31-36 and underline stand in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Syntax {
    pub truecolor: bool,
    bg: Option<Rgb>,
    ansi: [Option<Rgb>; 8],
    /// Red, green, yellow, magenta, cyan, moved until |Lc| >= 75 against the background.
    hues: [Option<Rgb>; 5],
    /// The selection's background and the text on it.
    sel: Option<(Rgb, Rgb)>,
}

impl Syntax {
    pub fn new(truecolor: bool) -> Syntax {
        Syntax { truecolor, ..Syntax::default() }
    }

    /// The terminal's background (si's `Event::Bg`).
    pub fn bg(&mut self, bg: Rgb) {
        self.bg = Some(bg);
        self.derive();
    }

    /// The payload of an OSC 4 reply, `4;N;rgb:...`. Anything else is ignored.
    pub fn reply(&mut self, osc: &str) {
        if let ["4", i, c] = osc.splitn(3, ';').collect::<Vec<_>>()[..]
            && let Some(a) = i.parse::<usize>().ok().and_then(|i| self.ansi.get_mut(i))
        {
            *a = parse_osc11(c);
            self.derive();
        }
    }

    fn derive(&mut self) {
        let bg = self.bg.filter(|_| self.truecolor);
        self.hues = [1, 2, 3, 5, 6].map(|i| Some(contrast(self.ansi[i]?, bg?, 75.0)));
        // SYN-5: a neutral gray (lightness only) at |Lc| >= 15, and gray text on it at >= 75.
        self.sel = bg.map(|bg| {
            let gray = contrast(from_lch([to_lch(bg)[0], 0.0, 0.0]), bg, 15.0);
            (gray, contrast((128, 128, 128), gray, 75.0))
        });
    }

    /// Whether replies are still to come (the quit then waits for the terminal to finish, VIS-4).
    pub fn pending(&self) -> bool {
        self.truecolor && self.hues.iter().any(Option::is_none)
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

/// What the terminal sends, as sim's events. si's `Reply` puts together the answer to the
/// background query (OSC 11); the answers to OSC 4 are put together here, ahead of it: an Alt+`]`
/// is held until the next key says which it is. crossterm reads `ESC ]` as Alt+`]`, the payload as
/// plain keys, and `ESC \` or BEL as Alt+`\` or Ctrl-g. For two seconds after the start only (a
/// typed Esc and `]` must not wait for ever); a key that does not belong to a reply goes on as a
/// key, in order. Mouse events and Shift-Tab, which si has no key for, are sim's.
pub struct Input {
    reply: Reply,
    held: Option<Ev>,
    osc: Option<String>,
    until: Option<Instant>,
}

impl Input {
    pub fn new(truecolor: bool) -> Input {
        let until = truecolor.then(|| Instant::now() + Duration::from_secs(2));
        Input { reply: Reply::default(), held: None, osc: None, until }
    }

    /// Keys on to si's `Reply`, which turns them into keys and the answer to OSC 11.
    fn pass(&mut self, evs: impl IntoIterator<Item = Ev>) -> Vec<SimEvent> {
        evs.into_iter().flat_map(|e| self.reply.feed(e)).map(SimEvent::Term).collect()
    }

    pub fn feed(&mut self, ev: Ev) -> Vec<SimEvent> {
        let (alt, ctrl) = (KeyModifiers::ALT, KeyModifiers::CONTROL);
        let (code, mods) = match &ev {
            Ev::Key(k) => (k.code, k.modifiers),
            Ev::Mouse(m) => return vec![SimEvent::Mouse(*m)],
            _ => (KeyCode::Null, KeyModifiers::NONE),
        };
        if code == KeyCode::BackTab {
            return vec![SimEvent::BackTab];
        }
        let (plain, on) =
            (!mods.intersects(alt | ctrl), self.until.is_some_and(|t| Instant::now() < t));
        let end = matches!(code, KeyCode::Char('\\' | 'g')) && !plain;
        match (self.osc.take(), self.held.take(), code) {
            (Some(s), ..) if end => vec![SimEvent::Osc(s)],
            (Some(mut s), _, KeyCode::Char(c)) if plain && s.len() < 96 => {
                s.push(c);
                self.osc = Some(s);
                vec![]
            }
            (None, Some(_), KeyCode::Char('4')) if plain => {
                self.osc = Some("4".into());
                vec![]
            }
            (None, None, KeyCode::Char(']')) if on && mods.contains(alt) => {
                self.held = Some(ev);
                vec![]
            }
            (_, held, _) => self.pass(held.into_iter().chain([ev])),
        }
    }
}
