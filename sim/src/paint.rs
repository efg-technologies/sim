//! SYN-1, SYN-5, FIND-1, LOOK-2, LOOK-3, LOOK-5, LOOK-6, LOOK-7
//! What sim draws beyond si's four styles: the looks a char can have, the Screen that carries
//! them with si's Frame, and the Painter that writes a Screen to the terminal, row by row. The
//! four styles are si's own (`term::sgr` with the Frame's palette, VIS-4); the looks go on top.
use crate::color::{Syntax, fade};
use si::core::color::{Palette, Rgb};
use si::core::view::{Frame, Row, Shape, Style, width};
use si::io::term::{self, sgr as base};
use std::io::{self, Write};
use std::ops::Range;

/// Mouse reporting (PTR-1): presses and releases (1000), movement with a button down (1002), in
/// SGR coding (1006). `MOUSE_OFF` undoes it exactly, in reverse.
pub const MOUSE_ON: &[u8] = b"\x1b[?1000h\x1b[?1002h\x1b[?1006h";
pub const MOUSE_OFF: &[u8] = b"\x1b[?1006l\x1b[?1002l\x1b[?1000l";

/// Gives the terminal back (si's `term::leave`) after letting go of the mouse. Errors are ignored:
/// it also runs from the panic hook and after SIGHUP.
pub fn leave(out: &mut impl Write) {
    let _ = out.write_all(MOUSE_OFF).and_then(|()| out.flush());
    term::leave(out);
}

/// What a char looks like on top of its style. The order is the precedence where the scopes of
/// a char disagree (the first wins), so a keyword inside a comment or a string stays a comment
/// or a string. `Bold` is a search match in the panel, `Sel` the Visual selection (SYN-5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Look {
    Com,
    Str,
    Kw,
    Num,
    Ty,
    Fun,
    Bold,
    Sel,
}

/// Column ranges of a row, in the columns of the Frame.
pub type Looks = Vec<(Range<u16>, Look)>;

/// What a run of cells has beside its style and look (LOOK-2 to LOOK-7): colors of its own, which
/// `layout` has worked out from the palette (a tint behind the text, the color of a box's line),
/// and how far its text fades toward the page, in percent.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Paint {
    pub fg: Option<Rgb>,
    pub bg: Option<Rgb>,
    pub fade: u8,
}

/// Column ranges of a row and their paints; the first range a column is in decides.
pub type Paints = Vec<(Range<u16>, Paint)>;

/// A Frame (which carries si's palette) with the looks and the paints of each of its rows (no
/// paints at all in calm) and the syntax colors they are drawn with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Screen {
    pub frame: Frame,
    pub looks: Vec<Looks>,
    pub paints: Vec<Paints>,
    pub syn: Syntax,
}

/// The SGR sequence for a char with this style, underline and look. The change style wins: on
/// it a look can only be bold or italic, never a color (P3: the change is the only chromatic
/// background, and reverse video would turn a text color into a background). Comments are the
/// secondary gray and italic, or faint and italic where there is no palette.
fn sgr(
    style: Style,
    underline: bool,
    look: Option<Look>,
    paint: Paint,
    pal: Option<Palette>,
    syn: &Syntax,
) -> String {
    let change = style == Style::Change;
    let fg = |(r, g, b): (u8, u8, u8)| format!(";38;2;{r};{g};{b}");
    let mut s = format!("\x1b[0{}{}", base(style, pal), ["", ";4"][usize::from(underline)]);
    let (attr, hue) = match look {
        Some(Look::Kw) => (";1", Some(1)),
        Some(Look::Str) => ("", Some(2)),
        Some(Look::Ty) => ("", Some(3)),
        Some(Look::Num) => ("", Some(5)),
        Some(Look::Fun) => ("", Some(6)),
        Some(Look::Com) => (if pal.is_some() { ";3" } else { ";2;3" }, None),
        Some(Look::Bold) => (";1", None),
        _ => ("", None),
    };
    s += attr;
    if let (Some(Look::Com), Some(p), false) = (look, pal, change) {
        s += &fg(p.secondary);
    }
    if let (Some(i), Some(l), false) = (hue, look, change) {
        s += &syn.hue(l).map_or(format!(";3{i}"), fg);
    }
    // LOOK-2 to LOOK-7: a background of its own (the tint, green or red, over the change style's
    // blue too: layout only puts one where it means it), a color of its own, and the fade of the
    // color the cell has by now (the change style never fades).
    let shown = match look {
        _ if change => None,
        Some(Look::Com) => pal.map(|p| p.secondary),
        Some(l @ (Look::Kw | Look::Str | Look::Ty | Look::Num | Look::Fun)) => syn.hue(l),
        _ => pal.and_then(|p| match style {
            Style::Primary => Some(p.primary),
            Style::Secondary | Style::Deleted => Some(p.secondary),
            Style::Change => None,
        }),
    };
    if let Some((r, g, b)) = paint.bg {
        s += &format!(";48;2;{r};{g};{b}");
    }
    if let (Some(c), Some(p), true) = (shown, pal, paint.fade > 0) {
        s += &fg(fade(c, p.bg, f64::from(paint.fade) / 100.0));
    }
    s += &paint.fg.map_or(String::new(), fg);
    if let (Some(Look::Sel), false, Some((bg, t))) = (look, change, syn.sel()) {
        s += &format!(";48;2;{};{};{}{}", bg.0, bg.1, bg.2, fg(t));
    }
    s + "m"
}

/// The text of a row with an SGR sequence wherever style, underline or look changes.
fn paint(
    b: &mut Vec<u8>,
    row: &Row,
    (looks, paints): (&Looks, &[(Range<u16>, Paint)]),
    (pal, syn): (Option<Palette>, &Syntax),
) {
    let (mut now, mut col) = (None, 0u16);
    let cs: Vec<char> = row.text.chars().collect();
    for (i, &c) in cs.iter().enumerate() {
        let style =
            row.spans.iter().find(|(r, _)| r.contains(&col)).map_or(Style::Primary, |s| s.1);
        let underline = row.underline.iter().any(|r| r.contains(&col));
        let look = looks.iter().find(|(r, _)| r.contains(&col)).map(|l| l.1);
        let by = paints.iter().find(|(r, _)| r.contains(&col)).map(|p| p.1).unwrap_or_default();
        if now != Some((style, underline, look, by)) {
            b.extend(sgr(style, underline, look, by, pal, syn).as_bytes());
            now = Some((style, underline, look, by));
        }
        b.extend(c.encode_utf8(&mut [0; 4]).as_bytes());
        col += width(&cs, i, usize::from(col)) as u16; // si's own rule, as in its `Term` (VIS-7)
    }
}

/// Draws Screens, rewriting only the rows that differ from the previous one (si's `Term::draw`
/// with looks).
pub struct Painter<W: Write> {
    pub out: W,
    prev: Option<Screen>,
    shape: Option<Shape>,
}

impl<W: Write> Painter<W> {
    pub fn new(out: W) -> Painter<W> {
        Painter { out, prev: None, shape: None }
    }

    /// Writes bytes of the terminal's own (the clipboard, the color query) between two frames.
    pub fn write(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.out.write_all(bytes).and_then(|()| self.out.flush())
    }

    pub fn draw(&mut self, s: Screen) -> io::Result<()> {
        let (f, mut b) = (&s.frame, Vec::<u8>::new());
        let same = self.prev.as_ref().filter(|p| {
            (p.frame.width, p.frame.height, p.frame.pal, p.syn) == (f.width, f.height, f.pal, s.syn)
        });
        // A frame that rewrites anything is one synchronized update, so nothing tears.
        let sync = same
            .is_none_or(|p| p.frame.rows != f.rows || p.looks != s.looks || p.paints != s.paints);
        if sync {
            b.extend(b"\x1b[?2026h");
        }
        if same.is_none() {
            b.extend(b"\x1b[2J");
        }
        for (i, row) in f.rows.iter().enumerate() {
            let paints = s.paints.get(i).map_or(&[][..], Vec::as_slice);
            let unchanged = |p: &Screen| {
                p.frame.rows[i] == *row
                    && p.looks[i] == s.looks[i]
                    && p.paints.get(i) == s.paints.get(i)
            };
            if same.is_some_and(unchanged) {
                continue;
            }
            write!(b, "\x1b[{};1H", i + 1)?;
            paint(&mut b, row, (&s.looks[i], paints), (f.pal, &s.syn));
            b.extend(b"\x1b[0m\x1b[K");
        }
        write!(b, "\x1b[{};{}H", f.cursor.1 + 1, f.cursor.0 + 1)?;
        if self.shape != Some(f.shape) {
            write!(b, "\x1b[{} q", f.shape as u8)?;
            self.shape = Some(f.shape);
        }
        if sync {
            b.extend(b"\x1b[?2026l");
        }
        self.prev = Some(s);
        self.out.write_all(&b)?;
        self.out.flush()
    }
}
