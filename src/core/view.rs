//! VIS-1, VIS-6, VIS-8, VIS-9, VIS-10, VIS-11, VIS-12, VI-5, SAFE-8, PERF-2
//! State to Frame (a grid of characters and styles), wrapping, kinsoku.
use super::app::App;
use super::vi::Mode;
use std::ops::Range;
use std::path::Path;
use unicode_width::UnicodeWidthChar;

#[rustfmt::skip]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Style { Primary, Secondary, Change, Deleted }

/// The cursor shapes, as the numbers of DECSCUSR (steady block, underline, bar).
#[rustfmt::skip]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Shape { Block = 2, Underline = 4, Bar = 6 }

/// One screen row. Spans and underlines are in columns; text outside every span is Primary.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Row {
    pub text: String,
    pub spans: Vec<(Range<u16>, Style)>,
    pub underline: Vec<Range<u16>>,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Frame {
    pub width: u16,
    pub height: u16,
    pub rows: Vec<Row>,
    pub cursor: (u16, u16),
    pub shape: Shape,
}

/// The safe spelling of a char that must never reach the terminal raw (SAFE-8).
#[rustfmt::skip]
fn show(c: char) -> Option<String> {
    match c {
        '\0'..='\x08' | '\n'..='\x1f' | '\x7f' => Some(format!("^{}", (c as u8 ^ 0x40) as char)),
        '\u{80}'..='\u{9f}' | '\u{200b}' | '\u{200e}'..='\u{200f}' | '\u{202a}'..='\u{202e}'
        | '\u{2066}'..='\u{2069}' | '\u{feff}' => Some(format!("<U+{:04X}>", c as u32)),
        _ => None,
    }
}

/// Columns a char takes when it starts at column `col` of its row.
fn width(c: char, col: usize) -> usize {
    match c {
        '\t' => 4 - col % 4,
        _ => show(c).map_or_else(|| c.width().unwrap_or(0), |s| s.len()),
    }
}

/// Where each row of a wrapped line starts (char indexes). A row breaks after a space or
/// between two wide chars when it can, and is cut at the edge otherwise.
fn wrap(cs: &[char], max: usize) -> Vec<usize> {
    let wide = |c: char| c.width() == Some(2);
    let (mut starts, mut s, mut i, mut col, mut brk) = (vec![0], 0, 0, 0, 0);
    while i < cs.len() {
        let w = width(cs[i], col);
        if col + w > max && i > s {
            s = if brk > s { brk } else { i };
            starts.push(s);
            (i, col, brk) = (s, 0, 0);
            continue;
        }
        col += w;
        i += 1;
        if matches!(cs[i - 1], ' ' | '\t') || (i < cs.len() && wide(cs[i - 1]) && wide(cs[i])) {
            brk = i;
        }
    }
    starts
}

/// `cs` as text that is safe to send, cut where it would pass `max` columns, and its width.
fn paint(cs: impl Iterator<Item = char>, max: usize) -> (String, usize) {
    let (mut out, mut col) = (String::new(), 0);
    for c in cs {
        let w = width(c, col);
        if col + w > max {
            break;
        }
        match (c, show(c)) {
            ('\t', _) => out.extend(std::iter::repeat_n(' ', w)),
            (_, Some(s)) => out += &s,
            _ => out.push(c),
        }
        col += w;
    }
    (out, col)
}

fn status(a: &App) -> String {
    let b = &a.buf;
    let mode = match a.vi.mode {
        Mode::Insert => "INSERT",
        Mode::Visual => "VISUAL",
        Mode::VLine => "V-LINE",
        _ => "NORMAL",
    };
    let notes: Vec<&str> =
        [b.dirty().then_some("modified"), b.readonly.as_deref()].into_iter().flatten().collect();
    let notes = if notes.is_empty() { String::new() } else { format!("  {}", notes.join(" · ")) };
    let name = a.path.as_deref().unwrap_or("[no name]");
    format!("{name}  {}:{}  {mode}{notes}", b.line_of(b.cursor) + 1, b.col() + 1)
}

/// Builds the screen and scrolls it (VIS-12). Only the lines that can be on screen together
/// with the cursor line are laid out: the work is proportional to the screen height, never to
/// the length of the file.
pub fn render(a: &mut App) -> Frame {
    let (w, h) = (usize::from(a.size.0), usize::from(a.size.1));
    let rows = h - 1;
    let b = &a.buf;
    let digits = b.lines().to_string().len().max(3);
    let gutter = digits + 2;
    let ext = a.path.as_deref().and_then(|p| Path::new(p).extension());
    let cap = if ext.is_none_or(|e| e == "md" || e == "txt") { 72 } else { usize::MAX };
    let max = cap.min(w.saturating_sub(gutter)).max(1);
    let cl = b.line_of(b.cursor);
    let (lo, hi) = (cl.saturating_sub(rows), (cl + rows + 1).min(b.lines()));
    let lay: Vec<(Vec<char>, Vec<usize>)> = (lo..hi)
        .map(|l| {
            let cs: Vec<char> = b.text.line(l).chars().take(b.line_len(l)).collect();
            let st = wrap(&cs, max);
            (cs, st)
        })
        .collect();
    let height = |l: usize| lay[l - lo].1.len();
    let (cs, st) = &lay[cl - lo];
    let ci = (b.cursor - b.start(cl)).min(cs.len());
    let r = st.partition_point(|&s| s <= ci) - 1;
    // Keep min(8, ...) rows of context around the cursor, or centre it when asked to.
    let ctx = 8.min((rows - 1) / 2);
    let above = |t: usize| (t..cl).map(height).sum::<usize>() + r;
    let tail = height(cl) - 1 - r + (cl + 1..hi).map(height).sum::<usize>();
    let mut top = a.top.clamp(lo, cl);
    if std::mem::take(&mut a.center) {
        top = (lo..=cl).rev().find(|&t| above(t) >= (rows - 1) / 2).unwrap_or(lo);
    } else {
        top = (lo..=top).rev().find(|&t| above(t) >= ctx).unwrap_or(lo);
        top = (top..=cl).find(|&t| above(t) + ctx.min(tail) < rows).unwrap_or(cl);
    }
    let sel = a.selection();
    let mut out: Vec<Row> = vec![];
    let mut cur = (0, 0);
    for l in top..hi {
        if out.len() >= rows {
            break;
        }
        let (cs, st) = &lay[l - lo];
        let (ls, seg) =
            (b.start(l), |x: usize, y: usize| paint(cs[x..y].iter().copied(), usize::MAX));
        if l == cl {
            cur = (gutter + seg(st[r], ci).1, out.len() + r);
        }
        for (i, &s) in st.iter().enumerate() {
            let e = st.get(i + 1).copied().unwrap_or(cs.len());
            let (head, spans) = match i {
                0 => (
                    format!(" {:>digits$} ", l + 1),
                    vec![(1..1 + digits as u16, Style::Secondary)],
                ),
                _ => (" ".repeat(gutter), vec![]),
            };
            let at = |x: usize| (gutter + seg(s, x - ls).1) as u16;
            let picked =
                sel.map(|(x, y)| ((ls + s).max(x), (ls + e).min(y))).filter(|(x, y)| x < y);
            let underline = picked.map(|(x, y)| at(x)..at(y)).into_iter().collect();
            out.push(Row { text: head + &seg(s, e).0, spans, underline });
        }
    }
    out.resize(rows, Row::default());
    let cmd = a.vi.mode == Mode::Cmd;
    let line = if cmd { a.vi.line.clone() } else { a.msg.clone().unwrap_or_else(|| status(a)) };
    let shape =
        if matches!(a.vi.mode, Mode::Insert | Mode::Cmd) { Shape::Bar } else { Shape::Block };
    out.push(Row { text: paint(line.chars(), w).0, ..Row::default() });
    a.top = top;
    let (x, y) = if cmd {
        (paint(line.chars().take(a.vi.pos), w).1, rows)
    } else {
        (cur.0, cur.1.min(rows - 1))
    };
    let cursor = (x.min(w - 1) as u16, y as u16);
    Frame { width: w as u16, height: h as u16, rows: out, cursor, shape }
}
