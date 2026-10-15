//! SYN-1, SYN-5, FIND-1
//! What sim draws beyond si's four styles: the looks a char can have, the Screen that carries
//! them with si's Frame, and the Painter that writes a Screen to the terminal, row by row.
//! si's own `Term` knows only its four styles, so sim has this painter (see NOTES in SPEC).
use crate::color::Palette;
use si::core::view::{Frame, Row, Shape, Style};
use std::io::{self, Write};
use std::ops::Range;
use unicode_width::UnicodeWidthChar;

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

/// A Frame with the looks of each of its rows and the palette they are drawn with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Screen {
    pub frame: Frame,
    pub looks: Vec<Looks>,
    pub pal: Palette,
}

/// The SGR sequence for a char with this style, underline and look. The change style wins: on
/// it a look can only be bold or italic, never a color (P3: the change is the only chromatic
/// background, and reverse video would turn a text color into a background).
fn sgr(style: Style, underline: bool, look: Option<Look>, pal: &Palette) -> String {
    let change = style == Style::Change;
    let (base, ul) = (["", ";2", ";7", ";2;9"][style as usize], ["", ";4"][usize::from(underline)]);
    let mut s = format!("\x1b[0{base}{ul}");
    let (attr, hue) = match look {
        Some(Look::Kw) => (";1", Some(1)),
        Some(Look::Str) => ("", Some(2)),
        Some(Look::Ty) => ("", Some(3)),
        Some(Look::Num) => ("", Some(5)),
        Some(Look::Fun) => ("", Some(6)),
        Some(Look::Com) => (";2;3", None),
        Some(Look::Bold) => (";1", None),
        _ => ("", None),
    };
    s += attr;
    if let (Some(i), Some(l), false) = (hue, look, change) {
        match pal.hue(l) {
            Some([r, g, b]) => s += &format!(";38;2;{r};{g};{b}"),
            None => s += &format!(";3{i}"),
        }
    }
    if let (Some(Look::Sel), false, Some((bg, t))) = (look, change, pal.sel()) {
        s += &format!(";48;2;{};{};{};38;2;{};{};{}", bg[0], bg[1], bg[2], t[0], t[1], t[2]);
    }
    s + "m"
}

/// The text of a row with an SGR sequence wherever style, underline or look changes.
fn paint(b: &mut Vec<u8>, row: &Row, looks: &Looks, pal: &Palette) {
    let (mut now, mut col) = (None, 0u16);
    for c in row.text.chars() {
        let style =
            row.spans.iter().find(|(r, _)| r.contains(&col)).map_or(Style::Primary, |s| s.1);
        let underline = row.underline.iter().any(|r| r.contains(&col));
        let look = looks.iter().find(|(r, _)| r.contains(&col)).map(|l| l.1);
        if now != Some((style, underline, look)) {
            b.extend(sgr(style, underline, look, pal).as_bytes());
            now = Some((style, underline, look));
        }
        b.extend(c.encode_utf8(&mut [0; 4]).as_bytes());
        col += c.width().unwrap_or(0) as u16;
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
        let same = self
            .prev
            .as_ref()
            .filter(|p| (p.frame.width, p.frame.height, p.pal) == (f.width, f.height, s.pal));
        // A frame that rewrites anything is one synchronized update, so nothing tears.
        let sync = same.is_none_or(|p| p.frame.rows != f.rows || p.looks != s.looks);
        if sync {
            b.extend(b"\x1b[?2026h");
        }
        if same.is_none() {
            b.extend(b"\x1b[2J");
        }
        for (i, row) in f.rows.iter().enumerate() {
            if same.is_some_and(|p| p.frame.rows[i] == *row && p.looks[i] == s.looks[i]) {
                continue;
            }
            write!(b, "\x1b[{};1H", i + 1)?;
            paint(&mut b, row, &s.looks[i], &s.pal);
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
