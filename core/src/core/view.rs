//! CLI-5, VIS-1, VIS-2, VIS-3, VIS-6, VIS-7, VIS-8, VIS-9, VIS-10, VIS-11, VIS-12, VI-5, VI-12, SAFE-8, PERF-2, FILT-1
//! State to Frame (a grid of characters and styles), wrapping, kinsoku.
use super::app::{App, plural};
use super::buffer::Buffer;
use super::color::Palette;
use super::vi::Mode;
use std::ops::Range;
use std::path::Path;
use unicode_width::UnicodeWidthChar;

#[rustfmt::skip]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Style { Primary, Secondary, Change, Hidden }

/// The cursor shapes, as the numbers of DECSCUSR (steady block, bar).
#[rustfmt::skip]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Shape { Block = 2, Bar = 6 }

/// One screen row. Spans and underlines are in columns; text outside every span is Primary.
#[rustfmt::skip]
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Row { pub text: String, pub spans: Vec<(Range<u16>, Style)>, pub underline: Vec<Range<u16>> }

#[rustfmt::skip]
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Frame { pub width: u16, pub height: u16, pub rows: Vec<Row>, pub cursor: (u16, u16), pub shape: Shape, pub pal: Option<Palette> }

/// The safe spelling of a char that must never reach the terminal raw (SAFE-8).
#[rustfmt::skip]
fn show(c: char) -> Option<String> {
    match c {
        '\0'..='\x08' | '\n'..='\x1f' | '\x7f' => Some(format!("^{}", (c as u8 ^ 0x40) as char)),
        '\u{80}'..='\u{9f}' | '\u{61c}' | '\u{200b}' | '\u{200e}'..='\u{200f}'
        | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{feff}' => Some(format!("<U+{:04X}>", c as u32)),
        _ => None,
    }
}

/// Columns of `cs[i]` when it starts at column `col` of its row. A char (not a blank) followed by
/// U+FE0F takes 2 if it takes 1 (an emoji presentation sequence), and an emoji after a ZWJ that
/// follows an emoji joins the cluster before it and takes none, as grapheme-clustering terminals
/// draw them (VIS-7). An emoji is a pictograph that is wide, or followed by U+FE0F; before the
/// ZWJ it is a wide one, or a narrow one with its U+FE0F (so the width stays what `unicode-width`
/// gives such a string, or more).
pub fn width(cs: &[char], i: usize, col: usize) -> usize {
    #[rustfmt::skip]
    let pict = |c| matches!(c, '\u{a9}' | '\u{ae}' | '\u{203c}' | '\u{2049}' | '\u{2122}' | '\u{2139}' | '\u{2190}'..='\u{2bff}' | '\u{1f000}'..='\u{1faff}');
    let wide = |j: usize| pict(cs[j]) && cs[j].width() == Some(2);
    let right = wide(i) || pict(cs[i]) && cs.get(i + 1) == Some(&'\u{fe0f}');
    let left = |k: usize| {
        wide(k - 1) || k > 1 && cs[k - 1] == '\u{fe0f}' && pict(cs[k - 2]) && !wide(k - 2)
    };
    let joined = i > 1 && cs[i - 1] == '\u{200d}' && right && left(i - 1);
    match (cs[i], show(cs[i])) {
        ('\t', _) => 4 - col % 4,
        (_, Some(s)) => s.len(),
        _ if joined => 0,
        (c, _) => match c.width().unwrap_or(0) {
            1 if !c.is_whitespace() && cs.get(i + 1) == Some(&'\u{fe0f}') => 2,
            w => w,
        },
    }
}

/// Kinsoku (VIS-7): these never start a row, and these never end one.
const NOT_START: &str = "、。，．）」』】〕〉》ー…‥・！？";
const NOT_END: &str = "（「『【〔〈《";

/// Where each row of a wrapped line starts (char indexes). A row breaks after a space or between
/// two wide chars when it can, else wherever kinsoku allows, else at the edge (VIS-7).
pub fn wrap(cs: &[char], max: usize) -> Vec<usize> {
    let wide = |c: char| c.width() == Some(2);
    // Never inside a cluster (a joined emoji or a variation selector takes no columns of its own).
    let ok = |p: usize| {
        width(cs, p, 0) > 0 && !NOT_START.contains(cs[p]) && !NOT_END.contains(cs[p - 1])
    };
    let soft = |p: usize| matches!(cs[p - 1], ' ' | '\t') || (wide(cs[p - 1]) && wide(cs[p]));
    let (mut starts, mut s, mut i, mut col) = (vec![0], 0, 0, 0);
    while i < cs.len() {
        let w = width(cs, i, col);
        if col + w > max && i > s {
            let back = || (s + 1..=i).rev();
            s = back().find(|&p| ok(p) && soft(p)).or_else(|| back().find(|&p| ok(p))).unwrap_or(i);
            starts.push(s);
            (i, col) = (s, 0);
            continue;
        }
        col += w;
        i += 1;
    }
    starts
}

/// `cs[r]` as text that is safe to send, cut where it would pass `max` columns, and its width.
fn paint(cs: &[char], r: Range<usize>, max: usize) -> (String, usize) {
    let (mut out, mut col) = (String::new(), 0);
    for i in r {
        let w = width(cs, i, col);
        if col + w > max {
            break;
        }
        match (cs[i], show(cs[i])) {
            ('\t', _) => out.extend(std::iter::repeat_n(' ', w)),
            (_, Some(s)) => out += &s,
            // A variation selector after `^[` would widen its `[`: it shows nothing anyway.
            ('\u{fe0f}', _) if i > 0 && (show(cs[i - 1]).is_some() || cs[i - 1] == '\u{fe0f}') => {}
            (c, _) => out.push(c),
        }
        col += w;
    }
    (out, col)
}

/// The chars of `s` as `paint` wants them, and painted whole up to `max` columns.
fn fit(s: &str, max: usize) -> String {
    let cs: Vec<char> = s.chars().collect();
    paint(&cs, 0..cs.len(), max).0
}

/// The columns `s` takes once painted.
fn cols(s: &str) -> usize {
    let cs: Vec<char> = s.chars().collect();
    paint(&cs, 0..cs.len(), usize::MAX).1
}

/// `path` for a status line of `w` columns whose other words are `tail` (VIS-9): the whole path if
/// the line fits, else `…/` and as many of its last components as fit, the file name at least.
pub fn shorten(path: &str, tail: &str, w: usize) -> String {
    let parts: Vec<_> = path.split('/').filter(|p| !p.is_empty()).collect();
    let last = |k: usize| format!("…/{}", parts[parts.len() - k..].join("/"));
    let mut all = std::iter::once(path.to_string()).chain((1..parts.len()).rev().map(last));
    all.clone().find(|p| cols(p) + cols(tail) <= w).or_else(|| all.next_back()).unwrap_or_default()
}

fn status(a: &App, w: usize) -> String {
    let b = &a.buf;
    #[rustfmt::skip]
    let mode = match a.vi.mode {
        Mode::Insert => "INSERT", Mode::Visual => "VISUAL", Mode::VLine => "V-LINE", _ => "NORMAL",
    };
    // A change on disk is a run of marked lines next to each other.
    let runs = (0..b.marks.len()).filter(|&i| i == 0 || b.marks[i - 1].line + 1 < b.marks[i].line);
    let count = |n: usize, one: &str, many: &str| (n > 0).then(|| plural(n, one, many));
    let notes: Vec<String> = [
        b.dirty().then(|| "modified".into()),
        b.readonly.clone(),
        a.gone.then(|| "deleted on disk".into()),
        count(runs.count(), "change on disk", "changes on disk"),
        a.diverged().then(|| "conflicts with the disk".into()),
        a.plain.then(|| "plain".into()),
        a.filter
            .as_ref()
            .map(|r| format!("filter {}s · Ctrl-c cancel", a.tick.saturating_sub(r.at) / 1000)),
    ]
    .into_iter()
    .flatten()
    .collect();
    let notes = if notes.is_empty() { String::new() } else { format!("  {}", notes.join(" · ")) };
    let tail = format!("  {}:{}  {mode}{notes}", b.line_of(b.cursor) + 1, b.col() + 1);
    shorten(a.path.as_deref().unwrap_or("[no name]"), &tail, w) + &tail
}

/// A line as it is drawn before wrapping: its chars with a style each, and the gutter symbol
/// (VIS-3, VIS-11).
#[rustfmt::skip]
struct Disp { cs: Vec<char>, sty: Vec<Style>, sym: char }

/// Buffer line `l` as it is drawn (VIS-2, VIS-3): the gutter symbol (a blank if nothing changed
/// there), and the change style on the marks of changes from disk.
fn plain(b: &Buffer, l: usize) -> Disp {
    let cs: Vec<char> = b.text.line(l).chars().take(b.line_len(l)).collect();
    let mut sty = vec![Style::Primary; cs.len()];
    let mut sym = ' ';
    if let Ok(i) = b.marks.binary_search_by_key(&l, |m| m.line) {
        sym = b.marks[i].kind;
        let all = 0..cs.len();
        for r in b.marks[i].words.as_deref().unwrap_or(std::slice::from_ref(&all)) {
            if let Some(x) = sty.get_mut(r.start.min(cs.len())..r.end.min(cs.len())) {
                x.fill(Style::Change);
            }
        }
    }
    Disp { cs, sty, sym }
}

/// Builds the screen and scrolls it (VIS-12). Only the lines that can be on screen together
/// with the cursor line are laid out: the work is proportional to the screen height, never to
/// the length of the file.
pub fn render(a: &mut App) -> Frame {
    let (w, h) = (usize::from(a.size.0), usize::from(a.size.1));
    let (rows, b) = (h - 1, &a.buf);
    let digits = b.lines().to_string().len().max(3);
    let gutter = digits + 2;
    let ext = a.path.as_deref().and_then(|p| Path::new(p).extension());
    let cap = if !a.wide && ext.is_none_or(|e| e == "md" || e == "txt") { 72 } else { usize::MAX };
    let max = cap.min(w.saturating_sub(gutter)).max(1);
    // While a search is typed the screen is drawn around its first match (VI-14).
    let peek = a.pv.filter(|_| a.vi.mode == Mode::Cmd).map(|p| p.min(b.text.len_chars()));
    let cursor = peek.unwrap_or(b.cursor);
    let cl = b.line_of(cursor);
    let (lo, hi) = (cl.saturating_sub(rows), (cl + rows + 1).min(b.lines()));
    let lay: Vec<(Vec<usize>, Disp)> =
        (lo..hi).map(|l| plain(b, l)).map(|d| (wrap(&d.cs, max), d)).collect();
    let height = |l: usize| lay[l - lo].0.len();
    let (st, d0) = &lay[cl - lo];
    let ci = (cursor - b.start(cl)).min(d0.cs.len());
    let r = st.partition_point(|&s| s <= ci) - 1;
    // Keep min(8, ...) rows of context around the cursor, or centre it when asked to.
    let ctx = 8.min((rows - 1) / 2);
    let above = |t: usize| (t..cl).map(height).sum::<usize>() + r;
    let tail = height(cl) - 1 - r + (cl + 1..hi).map(height).sum::<usize>();
    let mut top = a.top.clamp(lo, cl);
    let centered = std::mem::take(&mut a.center) || peek.is_some();
    if centered {
        top = (lo..=cl).rev().find(|&t| above(t) >= (rows - 1) / 2).unwrap_or(lo);
    } else {
        top = (lo..=top).rev().find(|&t| above(t) >= ctx).unwrap_or(lo);
        top = (top..=cl).find(|&t| above(t) + ctx.min(tail) < rows).unwrap_or(cl);
    }
    // A cursor line taller than the screen scrolls inside itself: its first `skip` rows are above
    // the screen. The last frame's `skip` stays while the cursor keeps clear of the context rows.
    let skip = match (top == cl, centered) {
        (false, _) => 0,
        (_, true) => r.saturating_sub((rows - 1) / 2),
        _ => (if a.top == cl { a.skip } else { 0 })
            .max((r + 1 + ctx.min(tail)).saturating_sub(rows))
            .min(r.saturating_sub(ctx)),
    };
    let sel = a.selection();
    let mut out: Vec<Row> = vec![];
    let mut cur = (0, 0);
    for l in top..hi {
        if out.len() >= rows {
            break;
        }
        let first = if l == top { skip } else { 0 };
        if l == cl {
            cur = (gutter + paint(&d0.cs, st[r]..ci, max).1, (out.len() + r).saturating_sub(first));
        }
        // The rows of this line, after the first `first`.
        let (st, d) = &lay[l - lo];
        for i in first..st.len() {
            let (s, e) = (st[i], st.get(i + 1).copied().unwrap_or(d.cs.len()));
            let seg = |x: usize, y: usize| paint(&d.cs, x..y, max);
            let col = |x: usize| (gutter + seg(s, x).1) as u16;
            // Only the line the cursor is on has its number in primary (VIS-11).
            #[rustfmt::skip]
            let (head, mut spans) = match i {
                0 => (format!("{}{:>digits$} ", d.sym, l + 1), if l == cl { vec![] } else { vec![(1..1 + digits as u16, Style::Secondary)] }),
                _ => (" ".repeat(gutter), vec![]),
            };
            if i == 0 && d.sym != ' ' {
                spans.push((0..1, Style::Change));
            }
            // A char shown in another form is Hidden (SAFE-8) whatever its style is: an attacker's
            // change around it must not hide it.
            #[rustfmt::skip]
            let sty = |z: usize| if show(d.cs[z]).is_some() { Style::Hidden } else { d.sty[z] };
            let mut x = s;
            while x < e {
                let end = (x..e).find(|&z| sty(z) != sty(x)).unwrap_or(e);
                if sty(x) != Style::Primary {
                    spans.push((col(x)..col(end), sty(x)));
                }
                x = end;
            }
            let ls = b.start(l);
            let picked =
                sel.map(|(x, y)| ((ls + s).max(x), (ls + e).min(y))).filter(|(x, y)| x < y);
            let underline = picked.map(|(x, y)| col(x - ls)..col(y - ls)).into_iter().collect();
            out.push(Row { text: fit(&(head + &seg(s, e).0), w), spans, underline });
        }
    }
    out.resize(rows, Row::default());
    if a.intro && a.buf.text.len_chars() == 0 {
        intro(&mut out, w);
    }
    let shape = match a.vi.mode {
        Mode::Insert | Mode::Cmd => Shape::Bar,
        _ => Shape::Block,
    };
    let (text, x) = if a.vi.mode == Mode::Cmd {
        // The command line scrolls to keep its cursor on the screen (VI-12).
        let (cs, pos) = (a.vi.line.chars().collect::<Vec<_>>(), a.vi.pos);
        let (mut near, mut off) = (0, pos);
        while off > 0 && near + width(&cs, off - 1, 0) < w {
            off -= 1;
            near += width(&cs, off, 0);
        }
        a.vi.off = a.vi.off.max(off).min(pos.saturating_sub(1)); // the char before it stays too
        (paint(&cs, a.vi.off..cs.len(), w).0, paint(&cs, a.vi.off..pos, usize::MAX).1)
    } else {
        (fit(&a.msg.clone().unwrap_or_else(|| status(a, w)), w), cur.0)
    };
    out.push(Row { text, ..Row::default() });
    if peek.is_none() {
        (a.top, a.skip) = (top, skip);
    }
    let y = if a.vi.mode == Mode::Cmd { rows } else { cur.1.min(rows - 1) };
    let cursor = (x.min(w - 1) as u16, y as u16);
    Frame { width: w as u16, height: h as u16, rows: out, cursor, shape, pal: a.pal }
}

/// CLI-5: what sim is and the keys to start with, centered below the first row, as many lines as fit.
fn intro(out: &mut [Row], w: usize) {
    #[rustfmt::skip]
    const KEYS: [(&str, &str); 6] = [
        ("i / Esc", "type / stop typing"), ("Ctrl-s", "save"), ("ZZ", "save and quit"), ("q", "quit"),
        ("ZQ", "quit without saving"), ("]c / [c", "next / previous change"),
    ];
    let title = format!("sim {}", env!("CARGO_PKG_VERSION"));
    let tagline = "The next-generation Vim that shows even what is hidden.";
    // Each line is its text and where its Secondary part starts; the keys are padded to one width.
    let keys = KEYS.iter().map(|(k, d)| (format!("{k:<15}{d:<32}"), 15));
    let lines = [(title.clone(), title.len()), (tagline.into(), 0), (String::new(), 0)];
    let top = 1 + (out.len() - 1).saturating_sub(KEYS.len() + 3) / 2;
    for (row, (text, sec)) in out[top..].iter_mut().zip(lines.into_iter().chain(keys)) {
        let indent = w.saturating_sub(text.len()) / 2;
        row.text = fit(format!("{:indent$}{text}", "").trim_end(), w);
        let from = (indent + sec).min(row.text.len());
        row.spans = vec![(from as u16..row.text.len() as u16, Style::Secondary)];
    }
}
