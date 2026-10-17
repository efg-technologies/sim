//! SYN-1, SYN-5, LOOK-2, LOOK-3, LOOK-5, LOOK-6, LOOK-9, UI-1, UI-12, COLOR-1
//! The syntax colors (pure): the terminal's own ANSI colors (OSC 4), moved in OKLCH lightness until
//! they read against the background that si's palette is made from (`Event::Bg`, VIS-4), the
//! selection's gray, and what puts the terminal's replies together out of the key stream. APCA,
//! OKLCH to sRGB and the parsing of a reply are si's (`core::color`).
use crate::paint::Look;
use crate::sim::SimEvent;
use crossterm::event::{Event as Ev, KeyCode, KeyModifiers};
use si::core::color::{Palette, Rgb, apca, oklch, parse_osc11};
use si::io::term::Reply;
use std::cell::RefCell;
use std::collections::HashMap;

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
    thread_local!(static MEMO: RefCell<HashMap<(Rgb, Rgb, u64), Rgb>> = RefCell::default());
    let key = (fg, bg, f.to_bits());
    if let Some(c) = MEMO.with(|m| m.borrow().get(&key).copied()) {
        return c;
    }
    let (p, q) = (to_lab(fg), to_lab(bg));
    let reads = |t: f64| apca(lerp(p, q, t), bg).abs() >= 40.0;
    let step = |(lo, hi): (f64, f64), _| {
        let mid = (lo + hi) / 2.0;
        if reads(mid) { (mid, hi) } else { (lo, mid) }
    };
    let c = if !reads(0.0) {
        fg
    } else {
        lerp(p, q, if reads(f) { f } else { (0..5).fold((0.0, f), step).0 })
    };
    MEMO.with(|m| m.borrow_mut().insert(key, c));
    c
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
fn text_on(primary: Rgb, c: Rgb) -> Rgb {
    contrast(primary, c, 75.5)
}

/// The author's `scale_hex` (his `init.lua`): each channel times `f`, clipped, on a dark page; on a
/// light one each is divided by `f`, so the result is always a step away from the page and never
/// past it (a white page times 1.18 would be white).
pub fn scale(c: Rgb, f: f64) -> Rgb {
    let dark = to_lch(c)[0] < 0.6;
    let go = |v: u8| {
        let x = f64::from(v);
        (if dark { x * f } else { x / f }).round().clamp(0.0, 255.0) as u8
    };
    (go(c.0), go(c.1), go(c.2))
}

/// A color as far from the page `bg` as the author's `scale(bg, f)` is (and at least `least` of
/// OKLab lightness away, so that a black page has a step too), but made of the page and `to`, so
/// that it has no more hue than the page itself: scaling a page with a tint (solarized's) scales
/// the tint, and a blue handle would be the accent's (COLOR-1).
pub fn stepped(bg: Rgb, to: Rgb, f: f64, least: f64) -> Rgb {
    let (l0, goal) = (to_lch(bg)[0], to_lch(scale(bg, f))[0]);
    let want = if goal >= l0 { goal.max(l0 + least) } else { goal.min(l0 - least) };
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..24 {
        let mid = (lo + hi) / 2.0;
        let l = to_lch(mix(bg, to, mid))[0];
        let short = if want >= l0 { l < want } else { l > want };
        *(if short { &mut lo } else { &mut hi }) = mid;
    }
    mix(bg, to, hi)
}

/// UI-1: the hues the interface has, in the order of `Tones::fg`. A hue has one meaning (see
/// `Role`): the chrome asks for a role (`change_fg`, `accent_fg`, `alert_fg`), never for a hue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Accent {
    Red,
    Green,
    Amber,
    Blue,
    Purple,
}

impl Accent {
    pub const ALL: [Accent; 5] =
        [Accent::Red, Accent::Green, Accent::Amber, Accent::Blue, Accent::Purple];

    /// The ANSI color the terminal is asked for, and the OKLCH hue that stands in for it.
    fn class(self) -> (usize, f64) {
        [(1, 25.0), (2, 145.0), (3, 85.0), (4, 250.0), (5, 320.0)][self as usize]
    }
}

/// UI-1: what a color of the interface means. Neutral (the primary, secondary and faint grays, and
/// weight) is all structure; Change is green for what comes, amber for what changed and red for
/// what goes, and nothing else; Accent, blue, is "here, where you act"; Alert, magenta, is only
/// what is hidden or dangerous. One hue, one meaning, so that a change is the one thing that pops
/// out (CONCEPT finding 8).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Neutral,
    Change,
    Accent,
    Alert,
}

/// The three meanings of Change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Chg {
    Add,
    Mod,
    Del,
}

/// LOOK-2 to LOOK-6, UI-1: the colors the looks add to si's palette. Green (OKLCH hue 150) is what
/// comes and red (hue 25) what goes, each with its own text gray; the tints, the card and the
/// shadow are mixed in OKLab from the page's background. The interface has six accents (text
/// that reads on every neutral surface), a block of each with its text, and neutral surfaces.
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
    /// UI-1: the surfaces of the interface, 6, 9, 10 and 14% of the way from the page toward
    /// secondary (the cursor's line, a fenced block, the bars, inline code).
    pub cursor: Rgb,
    pub band: Rgb,
    pub bar: Rgb,
    pub code: Rgb,
    /// The neutral block of the mode in Normal mode (30% toward secondary) and its text.
    pub gray: Rgb,
    pub gray_on: Rgb,
    /// UI-6, UI-11: the author's `ColorColumn` (the page times 1.18) and `SatelliteBar` (times 1.8):
    /// the page's own lightness moved as far from it, made of the page and the secondary gray (`stepped`).
    pub cc: Rgb,
    pub thumb: Rgb,
    fg: [Rgb; 5],
    block: [Rgb; 5],
    on: [Rgb; 5],
}

impl Tones {
    /// The tones for a palette, the accents derived from their hues (no reply of the terminal).
    pub fn new(p: &Palette) -> Tones {
        Tones::with(p, &Syntax::default())
    }

    /// The tones for a palette and the colors the terminal told (`Syntax`).
    pub fn with(p: &Palette, syn: &Syntax) -> Tones {
        let (add, del) = (hued(p.bg, 150.0), hued(p.bg, 25.0));
        let by = |c, t| mix(p.bg, c, t);
        let card = by(p.secondary, 0.18);
        let fg = Accent::ALL.map(|a| contrast(syn.raw(a), card, 75.5));
        // A block has the hue of its accent as the terminal has it (the stand-in's when it is gray).
        let hue = |a: Accent| {
            let [_, chroma, h] = to_lch(syn.raw(a));
            if chroma < 0.02 { a.class().1 } else { h }
        };
        let block = Accent::ALL.map(|a| hued(p.bg, hue(a)));
        Tones {
            add,
            add_text: text_on(p.primary, add),
            del,
            del_text: text_on(p.primary, del),
            gain: by(add, 0.30),
            loss: by(del, 0.30),
            hunk: by(p.secondary, 0.12),
            card,
            shadow: by((0, 0, 0), 0.55),
            line: by(p.secondary, 0.45),
            focus: p.secondary,
            dim: mix(p.secondary, p.bg, 0.35),
            primary: p.primary,
            cursor: by(p.secondary, 0.06),
            band: by(p.secondary, 0.09),
            bar: by(p.secondary, 0.10),
            code: by(p.secondary, 0.14),
            cc: stepped(p.bg, p.secondary, 1.18, 0.02),
            thumb: stepped(p.bg, p.secondary, 1.8, 0.07),
            gray: by(p.secondary, 0.30),
            gray_on: text_on(p.primary, by(p.secondary, 0.30)),
            fg,
            block,
            on: block.map(|b| text_on(p.primary, b)),
        }
    }

    /// The text color of a Change: green, amber or red, |Lc| >= 75.5 against the card.
    pub fn change_fg(&self, c: Chg) -> Rgb {
        self.fg(match c {
            Chg::Add => Accent::Green,
            Chg::Mod => Accent::Amber,
            Chg::Del => Accent::Red,
        })
    }

    /// The Accent: blue as text, as a block and the text on the block.
    pub fn accent_fg(&self) -> Rgb {
        self.fg(Accent::Blue)
    }

    pub fn accent_block(&self) -> (Rgb, Rgb) {
        (self.block(Accent::Blue), self.on(Accent::Blue))
    }

    /// UI-14, PICK-4: the accent as a solid background, for the match the cursor is on and the
    /// matched chars of a preview: its text color (|Lc| >= 75.5 against the card) and the text on it.
    pub fn accent_solid(&self) -> (Rgb, Rgb) {
        let solid = self.fg(Accent::Blue);
        (solid, text_on(self.primary, solid))
    }

    /// The Alert: magenta, likewise.
    pub fn alert_fg(&self) -> Rgb {
        self.fg(Accent::Purple)
    }

    pub fn alert_block(&self) -> (Rgb, Rgb) {
        (self.block(Accent::Purple), self.on(Accent::Purple))
    }

    /// Every color the chrome has, with the role it has (UI-1): what the budget is checked against.
    pub fn roles(&self) -> Vec<(&'static str, Role, Rgb)> {
        use Role::{Accent as A, Alert, Change, Neutral};
        let (ab, ao) = self.accent_block();
        let (lb, lo) = self.alert_block();
        let (sb, so) = self.accent_solid();
        vec![
            ("primary", Neutral, self.primary),
            ("secondary", Neutral, self.focus),
            ("faint", Neutral, self.dim),
            ("lines", Neutral, self.line),
            ("cursor line", Neutral, self.cursor),
            ("band", Neutral, self.band),
            ("bar", Neutral, self.bar),
            ("code", Neutral, self.code),
            ("hunk", Neutral, self.hunk),
            ("card", Neutral, self.card),
            ("shadow", Neutral, self.shadow),
            ("gray block", Neutral, self.gray),
            ("gray block text", Neutral, self.gray_on),
            ("colorcolumn", Neutral, self.cc),
            ("scrollbar handle", Neutral, self.thumb),
            ("add block", Change, self.add),
            ("add block text", Change, self.add_text),
            ("del block", Change, self.del),
            ("del block text", Change, self.del_text),
            ("gain tint", Change, self.gain),
            ("loss tint", Change, self.loss),
            ("add text", Change, self.change_fg(Chg::Add)),
            ("mod text", Change, self.change_fg(Chg::Mod)),
            ("del text", Change, self.change_fg(Chg::Del)),
            ("accent text", A, self.accent_fg()),
            ("accent block", A, ab),
            ("accent block text", A, ao),
            ("accent solid", A, sb),
            ("accent solid text", A, so),
            ("alert text", Alert, self.alert_fg()),
            ("alert block", Alert, lb),
            ("alert block text", Alert, lo),
        ]
    }

    /// The text color of a hue: |Lc| >= 75.5 against the card, so against every neutral surface.
    pub fn fg(&self, a: Accent) -> Rgb {
        self.fg[a as usize]
    }

    /// The accent as a background (the nearest to the page at |Lc| 15.5), and the text on it.
    pub fn block(&self, a: Accent) -> Rgb {
        self.block[a as usize]
    }

    pub fn on(&self, a: Accent) -> Rgb {
        self.on[a as usize]
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
        // A color the terminal has not told (not every terminal answers OSC 4; tmux relays the reply, SYN-1) is made of
        // the OKLCH hue of its class, red, green, yellow, magenta and cyan, and a moderate chroma.
        let class = [(1, 25.0), (2, 145.0), (3, 85.0), (5, 320.0), (6, 195.0)];
        self.hues = class.map(|(i, hue)| {
            let own = self.ansi[i].unwrap_or_else(|| from_lch([0.5, 0.12, hue]));
            Some(contrast(own, bg?, 75.0))
        });
        // SYN-5: a neutral gray (lightness only) at |Lc| >= 15, and gray text on it at >= 75.
        self.sel = bg.map(|bg| {
            let gray = contrast(from_lch([to_lch(bg)[0], 0.0, 0.0]), bg, 15.0);
            (gray, contrast((128, 128, 128), gray, 75.0))
        });
    }

    /// Whether replies are still to come (the quit then waits for the terminal to finish, VIS-4).
    pub fn pending(&self) -> bool {
        self.truecolor && [1, 2, 3, 5, 6].iter().any(|&i| self.ansi[i].is_none())
    }

    /// UI-1: an accent as the terminal has it (the reply of OSC 4 when it came, else the color of
    /// the OKLCH hue of its class), before it is moved to read against anything.
    pub fn raw(&self, a: Accent) -> Rgb {
        let (i, hue) = a.class();
        self.ansi[i].unwrap_or_else(|| from_lch([0.5, 0.12, hue]))
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
