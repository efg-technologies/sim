//! SYN-1, SYN-5, LOOK-2, LOOK-3, LOOK-5, LOOK-6, LOOK-9
//! The syntax colors (pure): the terminal's own ANSI colors (OSC 4), moved in OKLCH lightness until
//! they read against the background that si's palette is made from (`Event::Bg`, VIS-4), the
//! selection's gray, and what puts the terminal's replies together out of the key stream. APCA,
//! OKLCH to sRGB and the parsing of a reply are si's (`core::color`).
use crate::paint::Look;
use crate::sim::SimEvent;
use crossterm::event::{Event as Ev, KeyCode, KeyModifiers};
use si::core::color::{Palette, Rgb, apca, oklch, parse_osc11};
use si::io::term::Reply;

/// What is asked of the terminal besides its background (which si's `term::enter` asks, VIS-4): the
/// five ANSI colors that carry syntax (red, green, yellow, magenta, cyan), each ended with ST.
pub const QUERY: &str =
    "\x1b]4;1;?\x1b\\\x1b]4;2;?\x1b\\\x1b]4;3;?\x1b\\\x1b]4;5;?\x1b\\\x1b]4;6;?\x1b\\";

/// How many replies `QUERY` asks for.
const REPLIES: usize = 5;

/// sRGB to OKLab (Ottosson): lightness and the two opponent axes.
fn to_lab((r, g, b): Rgb) -> [f64; 3] {
    let [r, g, b] = [r, g, b].map(|v| {
        let v = f64::from(v) / 255.0;
        if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    });
    let l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b).cbrt();
    let m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b).cbrt();
    let s = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b).cbrt();
    [
        0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s,
        1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s,
        0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s,
    ]
}

/// sRGB to OKLCH: lightness, chroma, hue in degrees.
pub fn to_lch(c: Rgb) -> [f64; 3] {
    let [l, a, b] = to_lab(c);
    [l, a.hypot(b), b.atan2(a).to_degrees()]
}

/// OKLab to sRGB, clipped to the gamut: what a point between two colors can need.
fn from_lab([l, a, b]: [f64; 3]) -> Rgb {
    let cube = |x: f64| x.powi(3);
    let (l_, m_, s_) = (
        cube(l + 0.3963377774 * a + 0.2158037573 * b),
        cube(l - 0.1055613458 * a - 0.0638541728 * b),
        cube(l - 0.0894841775 * a - 1.2914855480 * b),
    );
    let [r, g, b] = [
        4.0767416621 * l_ - 3.3077115913 * m_ + 0.2309699292 * s_,
        -1.2684380046 * l_ + 2.6097574011 * m_ - 0.3413193965 * s_,
        -0.0041960863 * l_ - 0.7034186147 * m_ + 1.7076147010 * s_,
    ]
    .map(|v| {
        let v = v.clamp(0.0, 1.0);
        let e = if v <= 0.0031308 { 12.92 * v } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 };
        (e * 255.0).round() as u8
    });
    (r, g, b)
}

/// The point `t` of the way from `a` to `b`, mixed in OKLab (so lightness moves evenly).
pub fn mix(a: Rgb, b: Rgb, t: f64) -> Rgb {
    lerp(to_lab(a), to_lab(b), t)
}

fn lerp(p: [f64; 3], q: [f64; 3], t: f64) -> Rgb {
    from_lab([0, 1, 2].map(|i| p[i] + (q[i] - p[i]) * t))
}

/// LOOK-6: `fg` faded `f` of the way into the page `bg`, but no further than where it still reads at
/// |Lc| 40 against it (found by bisecting the fade, five steps). Text that is under 40 already
/// is left as it is.
pub fn fade(fg: Rgb, bg: Rgb, f: f64) -> Rgb {
    let (p, q) = (to_lab(fg), to_lab(bg));
    let reads = |t: f64| apca(lerp(p, q, t), bg).abs() >= 40.0;
    if f <= 0.0 || !reads(0.0) {
        return fg;
    }
    let (mut lo, mut hi) = (0.0, f);
    if reads(f) {
        lo = f;
    } else {
        for _ in 0..5 {
            let mid = (lo + hi) / 2.0;
            *(if reads(mid) { &mut lo } else { &mut hi }) = mid;
        }
    }
    lerp(p, q, lo)
}

/// The most chromatic sRGB color at OKLCH lightness `l` and hue `hue` (bisection on the chroma:
/// along a fixed lightness and hue the gamut is an interval that starts at zero).
fn most_chromatic(l: f64, hue: f64) -> Option<Rgb> {
    let (mut lo, mut hi) = (0.0, 0.4);
    for _ in 0..12 {
        let mid = (lo + hi) / 2.0;
        *(if oklch(l, mid, hue).is_some() { &mut lo } else { &mut hi }) = mid;
    }
    oklch(l, lo, hue)
}

/// LOOK-2: a background of `hue` for the page `bg`, found the way si finds its change color (VIS-4):
/// of the most chromatic colors of that hue, the nearest to `bg` that is at least |Lc| 15 (and a
/// margin of 0.5) away from it, or the farthest when none is. The owner of sim asked for green and
/// red here and let color vision go, so these two are not checked under the simulations.
fn hued(bg: Rgb, hue: f64) -> Rgb {
    let all = || (1..200).filter_map(|i| most_chromatic(f64::from(i) / 200.0, hue));
    let lc = |c: &Rgb| apca(*c, bg).abs();
    let far = all().filter(|c| lc(c) >= 15.5).min_by(|a, b| lc(a).total_cmp(&lc(b)));
    far.or_else(|| all().max_by(|a, b| lc(a).total_cmp(&lc(b)))).unwrap_or(bg)
}

/// The text on `c`, as si's `change_text`: the gray nearest to `primary`, on its side of the page,
/// with |Lc| 75 (and the margin) against `c`; black or white, whichever reads better, if that is
/// out of reach.
fn text_on(bg: Rgb, primary: Rgb, c: Rgb) -> Rgb {
    let lc = |g: u8| apca((g, g, g), c).abs();
    let reach = |g: &u8| lc(*g) >= 75.5;
    let g = if apca(primary, bg) < 0.0 {
        (primary.0..=255).find(reach).unwrap_or(255)
    } else {
        (0..=primary.0).rev().find(reach).unwrap_or(0)
    };
    let g = if lc(255 - g) > lc(g) && !reach(&g) { 255 - g } else { g };
    (g, g, g)
}

/// LOOK-2 to LOOK-6: the colors the looks add to si's palette. Green (OKLCH hue 150) is what comes
/// and red (hue 25) what goes, each with its own text gray; the tints, the card and the shadow are
/// mixed in OKLab from the page's background.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tones {
    pub add: Rgb,
    pub add_text: Rgb,
    pub del: Rgb,
    pub del_text: Rgb,
    /// The tint of a line that gained or changed text (30% toward green) and of a deletion point
    /// (30% toward red).
    pub gain: Rgb,
    pub loss: Rgb,
    /// The tint of a hunk, which has both: neutral, 12% toward secondary; the card's is 18%.
    pub hunk: Rgb,
    pub card: Rgb,
    /// Under the card: 55% toward black.
    pub shadow: Rgb,
    /// The lines of a box, 45% from the page toward secondary; the focused box has `focus`.
    pub line: Rgb,
    pub focus: Rgb,
    /// The title of a box without the focus.
    pub dim: Rgb,
    pub primary: Rgb,
}

impl Tones {
    pub fn new(p: &Palette) -> Tones {
        let (add, del) = (hued(p.bg, 150.0), hued(p.bg, 25.0));
        let by = |c, t| mix(p.bg, c, t);
        Tones {
            add,
            add_text: text_on(p.bg, p.primary, add),
            del,
            del_text: text_on(p.bg, p.primary, del),
            gain: by(add, 0.30),
            loss: by(del, 0.30),
            hunk: by(p.secondary, 0.12),
            card: by(p.secondary, 0.18),
            shadow: by((0, 0, 0), 0.55),
            line: by(p.secondary, 0.45),
            focus: p.secondary,
            dim: mix(p.secondary, p.bg, 0.35),
            primary: p.primary,
        }
    }
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
/// plain keys, and `ESC \` or BEL as Alt+`\` or Ctrl-g. While replies are still to come (the
/// five that `QUERY` asks for, however late: over a slow link they take seconds, and a reply that
/// is not put together here is typed into the text); a key that does not belong to a reply goes on
/// as a key, in order (si's `Reply` holds an Alt+`]` for the answer to OSC 11 just as long). Mouse
/// events and Shift-Tab, which si has no key for, are sim's.
pub struct Input {
    reply: Reply,
    held: Option<Ev>,
    osc: Option<String>,
    /// How many replies to OSC 4 are still to come.
    due: usize,
}

impl Input {
    pub fn new(truecolor: bool) -> Input {
        Input {
            reply: Reply::default(),
            held: None,
            osc: None,
            due: if truecolor { REPLIES } else { 0 },
        }
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
        let (plain, on) = (!mods.intersects(alt | ctrl), self.due > 0);
        let end = matches!(code, KeyCode::Char('\\' | 'g')) && !plain;
        match (self.osc.take(), self.held.take(), code) {
            (Some(s), ..) if end => {
                self.due = self.due.saturating_sub(1);
                vec![SimEvent::Osc(s)]
            }
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
