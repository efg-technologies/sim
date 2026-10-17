//! SYN-1, SYN-5, FIND-1, LOOK-2, LOOK-3, LOOK-5, LOOK-6, LOOK-7, UI-8, COLOR-1, COLOR-2
//! What sim draws beyond the core's four styles: the looks a char can have, the Screen that carries
//! them with the core's Frame, and the Painter that writes a Screen to the terminal, row by row. The
//! four styles are the core's own (`term::sgr` with the Frame's palette, VIS-4); the looks go on top.
use crate::color::{NUMBER_FLOOR, Syntax, TEXT_FLOOR, fade_to};
use sim_core::core::color::{Palette, Rgb};
use sim_core::core::view::{Frame, Shape, Style, width};
use sim_core::io::term::{self, sgr as base};
use std::collections::HashMap;
use std::io::{self, Write};
use std::ops::Range;

/// Mouse reporting (PTR-1): presses and releases (1000), movement with a button down (1002), in
/// SGR coding (1006). `MOUSE_OFF` undoes it exactly, in reverse.
pub const MOUSE_ON: &[u8] = b"\x1b[?1000h\x1b[?1002h\x1b[?1006h";
pub const MOUSE_OFF: &[u8] = b"\x1b[?1006l\x1b[?1002l\x1b[?1000l";

/// Gives the terminal back (the core's `term::leave`) after letting go of the mouse. Errors are ignored:
/// it also runs from the panic hook and after SIGHUP.
pub fn leave(out: &mut impl Write) {
    let _ = out.write_all(MOUSE_OFF).and_then(|()| out.flush());
    term::leave(out);
}

/// What a char looks like on top of its style. The order is the precedence where the scopes of
/// a char disagree (the first wins), so a keyword inside a comment or a string stays a comment
/// or a string, and a marker of markdown stays dim inside a heading. `Bold` is a search match in
/// the panel or markdown's strong text, `Sel` the Visual selection (SYN-5). From `Tick` to `Band`
/// are the looks of markdown (UI-8): the backticks of inline code, the language of a fence, a
/// marker (`#`, `**`, a fence, a bracket), a URL, a link's text, inline code, the six levels of
/// heading, a bullet, a quote's `>`, emphasis, and (zero-width, at the start of a line) a line of
/// a fenced block.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Look {
    Com,
    Str,
    Kw,
    Num,
    Ty,
    Fun,
    Tick,
    Lang,
    Mark,
    Url,
    Link,
    Code,
    H1,
    H2,
    H3,
    H4,
    H5,
    H6,
    Sub,
    Bullet,
    Quote,
    Em,
    Done,
    Rule,
    Band,
    Bold,
    Sel,
}

/// How a look is drawn (before a paint's colors): its SGR attributes, the syntax look whose hue it
/// takes (SGR 31-36 stands for it without a palette), and whether it is the secondary gray. Only
/// the looks of code have a hue (SYN-1); markdown's are weight, slant, underline and grays: a
/// heading is bold in the text's own color whatever its level (flat and depth give the hue and take
/// the weight of levels 4 to 6 by `Sub`, UI-8), a bullet, a quote's bar, a marker
/// and the language of a fence are secondary, a link is underlined (its blue is the accent's, laid
/// on by `layout` in flat and depth; calm has none, P3). One hue, one meaning (UI-1).
fn made_of(look: Look, pal: bool) -> (&'static str, Option<Look>, bool) {
    use Look::*;
    match look {
        Kw => (";1", Some(Kw), false),
        Str | Ty | Num | Fun => ("", Some(look), false),
        H1 | H2 | H3 | H4 | H5 | H6 => (";1", None, false),
        Sub => ("", None, false),
        Bullet | Quote | Lang | Done | Rule => ("", None, true),
        Link => (";4", None, false),
        Com => (if pal { ";3" } else { ";2;3" }, None, true),
        Mark | Tick | Url => (if pal { "" } else { ";2" }, None, true),
        Em => (";3", None, false),
        Bold => (";1", None, false),
        Code | Band | Sel => ("", None, false),
    }
}

/// The syntax look whose hue a look is drawn in, if it has one: the looks of code only (UI-1).
pub fn syntax_hue(look: Look) -> Option<Look> {
    made_of(look, true).1
}

/// The SGR 31-36 that stands for a syntax look where there is no palette or no truecolor: red for
/// keywords, green for strings and numbers (both are literal values; magenta, 35, is the alert's
/// alone, COLOR-1), yellow for types, cyan for functions.
pub fn ansi_of(look: Look) -> Option<u8> {
    Some(match syntax_hue(look)? {
        Look::Kw => 1,
        Look::Str | Look::Num => 2,
        Look::Ty => 3,
        _ => 6,
    })
}

/// Column ranges of a row, in the columns of the Frame.
pub type Looks = Vec<(Range<u16>, Look)>;

/// COLOR-2: what a paint is where there is no color. `layout` says it where the paint stands for
/// more than a surface: a solid block (a badge, the current match, a word that came) is inverse, a
/// lit match among others is underlined; a tint, a surface, a text color alone say nothing and are
/// dropped.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Cue {
    #[default]
    None,
    Under,
    Solid,
}

/// What a run of cells has beside its style and look (LOOK-2 to LOOK-7): colors of its own, which
/// `layout` has worked out from the palette (a tint behind the text, the color of a box's line),
/// how far its text fades toward the page, in percent, and whether it is bold (a title).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Paint {
    pub fg: Option<Rgb>,
    pub bg: Option<Rgb>,
    pub fade: u8,
    pub bold: bool,
    /// The fade stops at |Lc| 40, not 60: the line numbers, and what is only decoration (LOOK-6).
    pub soft: bool,
    /// The last word for its cells: they are drawn with this paint and nothing else, whatever look,
    /// selection, underline, fade or other paint they have (UI-15: a hidden char).
    pub sealed: bool,
    pub cue: Cue,
}

/// Column ranges of a row and their paints; the first range a column is in decides.
pub type Paints = Vec<(Range<u16>, Paint)>;

/// A Frame (which carries the core's palette) with the looks and the paints of each of its rows (no
/// paints at all in calm) and the syntax colors they are drawn with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Screen {
    pub frame: Frame,
    pub looks: Vec<Looks>,
    pub paints: Vec<Paints>,
    pub syn: Syntax,
}

/// COLOR-2: the SGR sequence where there is no color. The style is what it is without a palette
/// (faint for the secondary level, reverse for the change), a look adds its weight, slant and
/// underline (and faint for what the palette would have given the secondary gray), and the paint
/// its cue (`Cue`): inverse for a solid block, bold too where the block is a hidden char.
fn mono_sgr(style: Style, underline: bool, look: Option<Look>, paint: Paint) -> String {
    let mut attrs = base(style, None);
    let cue = paint.cue;
    attrs += [";4", ""][usize::from(!underline && cue != Cue::Under)];
    let (attr, _, dim) = look.map_or(("", None, false), |l| made_of(l, false));
    attrs += attr;
    attrs += [";1", ""][usize::from(!paint.bold && !(paint.sealed && cue == Cue::Solid))];
    attrs += [";2", ""][usize::from(!dim || attrs.contains(";2"))];
    let reversed = matches!(style, Style::Change | Style::Hidden);
    attrs += [";7", ""][usize::from(reversed || cue != Cue::Solid)];
    format!("\x1b[0{attrs}m")
}

/// The SGR sequence for a char with this style, underline and look. The change style wins: on
/// it a look can only be bold or italic, never a color (P3: the change is the only chromatic
/// background, and reverse video would turn a text color into a background). Comments are the
/// secondary gray and italic, or faint and italic where there is no palette.
///
/// Each color is written once, the last that was meant (the style's, then the look's, then the
/// paint's): tmux drops a whole SGR sequence of more than 20 parameters (3.7c, measured), and one
/// that repeated a color in turn (`38;2;..;38;2;..;48;2;..;38;2;..`) lost the colors of a fenced
/// block's code in the owner's tmux.
fn sgr(
    style: Style,
    underline: bool,
    look: Option<Look>,
    paint: Paint,
    pal: Option<Palette>,
    syn: &Syntax,
) -> String {
    if syn.mono {
        return mono_sgr(style, underline, look, paint);
    }
    let change = style == Style::Change;
    let rgb = |k: u8, (r, g, b): (u8, u8, u8)| format!(";{k}8;2;{r};{g};{b}");
    // What the style gives: attributes, and with a palette the colors (the core's `term::sgr`, VIS-4).
    let (mut attrs, mut fg, mut bg) = (String::new(), None, None);
    match (pal, style) {
        (None, _) => attrs += &base(style, None),
        (Some(p), Style::Primary) => fg = Some(p.primary),
        (Some(p), Style::Secondary) => fg = Some(p.secondary),
        (Some(p), Style::Change) => (bg, fg) = (Some(p.change), Some(p.change_text)),
        (Some(p), Style::Hidden) => (fg, attrs) = (Some(p.primary), ";7".into()),
    }
    attrs += [";4", ""][usize::from(!underline)];
    let (attr, kin, dim) = look.map_or(("", None, false), |l| made_of(l, pal.is_some()));
    attrs += attr;
    attrs += [";1", ""][usize::from(!paint.bold)];
    // The look's color: the secondary gray, or the syntax hue (SGR 31-36 where there is none).
    let mut ansi = None;
    if let (false, Some(p)) = (change, pal) {
        if dim {
            fg = Some(p.secondary);
        } else if let Some(k) = kin {
            (fg, ansi) = (syn.hue(k), syn.hue(k).map_or(ansi_of(k), |_| None));
        }
    } else if let (false, None, Some(k)) = (change, pal, kin) {
        ansi = ansi_of(k);
    }
    // LOOK-2 to LOOK-7: a background of its own (the tint, green or red, over the change style's
    // blue too: layout only puts one where it means it), a color of its own, and the fade of the
    // color the cell has by now (the change style never fades).
    bg = paint.bg.or(bg);
    if let Some(c) = paint.fg {
        (fg, ansi) = (Some(c), None);
    }
    // The secondary gray is rank and the fog is distance: only the gutter's numbers fade it (LOOK-6).
    if let (Some(c), Some(p), true, false) = (fg, pal, paint.fade > 0, change)
        && (paint.soft || c != p.secondary)
    {
        let floor = if paint.soft { NUMBER_FLOOR } else { TEXT_FLOOR };
        fg = Some(fade_to(c, p.bg, f64::from(paint.fade) / 100.0, floor));
    }
    let mut out = format!("\x1b[0{attrs}");
    if let (Some(Look::Sel), false, Some((sbg, text))) = (look, change, syn.sel()) {
        (bg, fg, ansi) = (Some(sbg), Some(text), None);
    }
    if let Some(i) = ansi {
        out += &format!(";3{i}");
    } else if let Some(c) = fg {
        out += &rgb(3, c);
    }
    if let Some(c) = bg {
        out += &rgb(4, c);
    }
    out + "m"
}

/// What one column of a row is: the first range of each list that holds it decides (the paints
/// are merged field by field: the first one that has a color gives it).
#[derive(Clone, Copy, Default)]
struct Cell {
    style: Option<Style>,
    underline: bool,
    look: Option<Look>,
    paint: Paint,
}

/// The cells of a row, one per column, made of its lists of ranges: the work is what the ranges
/// cover, not what each char would have to look for in them.
fn cells(cells: &mut Vec<Cell>, s: &Screen, i: usize, cols: usize) {
    let (row, looks) = (&s.frame.rows[i], &s.looks[i]);
    let paints = s.paints.get(i).map_or(&[][..], Vec::as_slice);
    cells.clear();
    cells.resize(cols, Cell::default());
    let span = |r: &Range<u16>| usize::from(r.start).min(cols)..usize::from(r.end).min(cols);
    for (r, st) in row.spans.iter().rev() {
        cells[span(r)].iter_mut().for_each(|c| c.style = Some(*st));
    }
    for r in &row.underline {
        cells[span(r)].iter_mut().for_each(|c| c.underline = true);
    }
    for (r, l) in looks.iter().rev() {
        cells[span(r)].iter_mut().for_each(|c| c.look = Some(*l));
    }
    for (r, p) in paints.iter().rev() {
        let on = |c: &mut Cell| {
            let q = c.paint;
            let fade = if p.fade > 0 { p.fade } else { q.fade };
            c.paint = Paint {
                fg: p.fg.or(q.fg),
                bg: p.bg.or(q.bg),
                fade,
                bold: p.bold || q.bold,
                soft: p.soft || q.soft,
                sealed: false,
                cue: p.cue.max(q.cue),
            };
        };
        cells[span(r)].iter_mut().for_each(on);
    }
    // UI-15: a hidden char is drawn as itself and nothing else: in flat and depth by a sealed paint,
    // in calm by its style (reverse video), whatever else the cell has (a selection, a match, a
    // surface, the fog, an underline).
    for c in cells.iter_mut().filter(|c| c.style == Some(Style::Hidden)) {
        *c = Cell { style: Some(Style::Hidden), ..Cell::default() };
    }
    for (r, p) in paints.iter().filter(|(_, p)| p.sealed) {
        for c in &mut cells[span(r)] {
            *c = Cell { paint: *p, ..Cell::default() };
        }
    }
}

/// The text of a row with an SGR sequence wherever style, underline, look or paint changes.
fn paint(b: &mut Vec<u8>, s: &Screen, i: usize, memo: &mut Memo, scratch: &mut Vec<Cell>) {
    let (pal, syn) = (s.frame.pal, &s.syn);
    let cs: Vec<char> = s.frame.rows[i].text.chars().collect();
    // The core's own rule for the columns of a char, as in its `Term` (VIS-7)
    let at: Vec<usize> = cs
        .iter()
        .enumerate()
        .scan(0, |col, (k, _)| {
            let at = *col;
            *col += width(&cs, k, at);
            Some(at)
        })
        .collect();
    cells(scratch, s, i, at.last().map_or(0, |&c| c + width(&cs, cs.len() - 1, c)));
    let (mut now, none) = (None, Cell::default());
    for (k, &c) in cs.iter().enumerate() {
        let cell = scratch.get(at[k]).unwrap_or(&none);
        let key = (cell.style, cell.underline, cell.look, cell.paint);
        if now != Some(key) {
            let (style, underline, look, by) =
                (cell.style.unwrap_or(Style::Primary), key.1, key.2, key.3);
            let id = (style as u8, underline, look.map(|l| l as u8), by);
            b.extend(
                memo.entry(id)
                    .or_insert_with(|| sgr(style, underline, look, by, pal, syn))
                    .as_bytes(),
            );
            now = Some(key);
        }
        b.extend(c.encode_utf8(&mut [0; 4]).as_bytes());
    }
}

/// The SGR sequences already made, by style, underline, look and paint: a screen has few of them,
/// and a row asks for dozens.
type Memo = HashMap<(u8, bool, Option<u8>, Paint), String>;

/// Draws Screens, rewriting only the rows that differ from the previous one (the core's `Term::draw`
/// with looks).
pub struct Painter<W: Write> {
    pub out: W,
    prev: Option<Screen>,
    shape: Option<Shape>,
    memo: Memo,
    scratch: Vec<Cell>,
}

impl<W: Write> Painter<W> {
    pub fn new(out: W) -> Painter<W> {
        Painter { out, prev: None, shape: None, memo: Memo::new(), scratch: vec![] }
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
            self.memo.clear(); // (the palette or the syntax colors may be new)
            b.extend(b"\x1b[2J");
        }
        for (i, row) in f.rows.iter().enumerate() {
            let unchanged = |p: &Screen| {
                p.frame.rows[i] == *row
                    && p.looks[i] == s.looks[i]
                    && p.paints.get(i) == s.paints.get(i)
            };
            if same.is_some_and(unchanged) {
                continue;
            }
            write!(b, "\x1b[{};1H", i + 1)?;
            paint(&mut b, &s, i, &mut self.memo, &mut self.scratch);
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
