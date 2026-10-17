//! PICK-1, PICK-3, PICK-4, PICK-5
//! The picker's window (pure): where it is for a screen's size, the rows of its parts put together
//! (border, prompt, list, preview, count), and how it is laid over the rows of a screen, whose cells
//! it covers. What it shows is `sim/live.rs`'s state; the colors are the budget's (COLOR-1).
use crate::color::Tones;
use crate::grep::PRow;
use crate::layout::{Line, border, fit, tail};
use crate::paint::{Look, Looks, Paint, Paints};
use si::core::view::{Frame, Row, Style, width};
use std::ops::Range;

/// PICK-5: the frames of the spinner.
pub const SPIN: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

/// PICK-5: a count with a comma for each thousand.
pub fn thousands(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// A part of the screen: its first column and row, and its size.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
}

/// PICK-1: where the picker is on a screen: the window (border included), and inside it the prompt,
/// the list and the preview.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Win {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
    /// The preview is under the list (a window under 100 columns), not beside it.
    pub stacked: bool,
    pub prompt: Rect,
    pub list: Rect,
    pub pane: Rect,
}

impl Win {
    /// The window for a screen of `(columns, rows)`, the last row being the status line: 80% of the
    /// width and of the rows above it, centered; none under 20 columns or 8 rows.
    pub fn new((sw, sh): (u16, u16)) -> Option<Win> {
        if sw < 20 || sh < 8 {
            return None;
        }
        let (sw, rows) = (u32::from(sw), u32::from(sh) - 1);
        let (w, h) = ((sw * 4 / 5).max(sw.min(24)), (rows * 4 / 5).max(rows.min(8)));
        let (x, y) = ((sw - w) / 2, (rows - h) / 2);
        let stacked = w < 100;
        let (prompt, list, pane);
        if stacked {
            let (iw, space) = (w - 2, h - 5);
            let n = (space * 2 / 5).max(3).min(space);
            prompt = (x + 1, y + 1, iw, 1);
            list = (x + 1, y + 3, iw, n);
            pane = (x + 1, y + 4 + n, iw, space - n);
        } else {
            let inner = w - 3;
            let lw = (inner * 2 / 5).max(20).min(inner);
            prompt = (x + 1, y + 1, lw, 1);
            list = (x + 1, y + 3, lw, h - 4);
            pane = (x + 2 + lw, y + 1, inner - lw, h - 2);
        }
        let rect = |(x, y, w, h): (u32, u32, u32, u32)| Rect {
            x: x as u16,
            y: y as u16,
            w: w as u16,
            h: h as u16,
        };
        Some(Win {
            x: x as u16,
            y: y as u16,
            w: w as u16,
            h: h as u16,
            stacked,
            prompt: rect(prompt),
            list: rect(list),
            pane: rect(pane),
        })
    }
}

/// PICK-4: the matching line of the preview: the rows of the frame that are the line's, and the
/// columns of the matched chars in them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Lit {
    pub rows: Range<usize>,
    pub marks: Vec<(usize, Range<u16>)>,
}

/// PICK-4: the preview: the App's frame rendered at the pane's size (its last row is its status
/// line, not shown), the looks of its rows, the line to light, the title, and what to say in place
/// of the text when there is none (a file too big).
pub struct Pane<'a> {
    pub title: String,
    pub frame: &'a Frame,
    pub looks: &'a [Looks],
    pub lit: Option<&'a Lit>,
    pub note: Option<&'a str>,
}

/// What the picker shows: its name, the query, the visible rows of the list, whether to say that
/// there are no matches, the count, the spinner and the preview.
pub struct View<'a> {
    pub title: &'a str,
    /// The root the picker searches, as the panel's header writes it (UI-2).
    pub scope: &'a str,
    pub query: &'a str,
    pub rows: &'a [PRow],
    pub none: bool,
    pub count: String,
    pub spin: Option<char>,
    pub pane: Option<Pane<'a>>,
}

/// PICK-5: the keys of the bottom border, in the order they are written; `KEEP` is the order they
/// are kept in when the border is short: moving goes first, closing next, opening last.
const KEYS: [&str; 3] = ["↵ open", "^n/^p move", "esc close"];
const KEEP: [&[usize]; 3] = [&[0, 1, 2], &[0, 2], &[0]];

/// A glyph in a paint.
fn glyph(s: &str, p: Paint) -> Line {
    let mut l = Line::default();
    l.put(s, p);
    l
}

/// The prompt, `width` columns: ` ❯ ` and the end of the query. Also the columns the query takes.
fn prompt(width: u16, query: &str, t: Option<&Tones>) -> (Line, usize) {
    let mut l = Line::default();
    l.put(" ", Paint::default());
    l.put("❯", Paint { fg: t.map(|t| t.focus), ..Paint::default() });
    l.put(" ", Paint::default());
    let shown = tail(query, usize::from(width).saturating_sub(4));
    l.put(&shown, Paint::default());
    let cols = fit(&shown, usize::MAX).1;
    l.pad(width);
    (l, cols)
}

/// A row of the list, `width` columns: the bar of the selected row, the text, and what the row
/// says about its chars. The selected row is the accent's block (underlined where there are no tones).
fn list_line(row: Option<&PRow>, width: u16, t: Option<&Tones>) -> Line {
    let mut l = Line::default();
    let Some(r) = row else {
        l.pad(width);
        return l;
    };
    let accent = t.map(Tones::accent_fg);
    l.put(if r.sel { "▎" } else { " " }, Paint { fg: accent, ..Paint::default() });
    l.put(&r.text, Paint::default());
    l.pad(width);
    let shift = |c: &Range<u16>| c.start + 1..c.end + 1;
    l.row.spans.extend(r.dim.iter().map(|c| (shift(c), Style::Secondary)));
    l.looks.extend(r.bold.iter().map(|c| (shift(c), Look::Bold)));
    let hit = Paint { fg: accent, bold: true, ..Paint::default() };
    l.paints.extend(t.map(|_| r.bold.iter().map(|c| (shift(c), hit))).into_iter().flatten());
    match (r.sel, t) {
        (true, Some(t)) => {
            l.paints.push((0..width, Paint { bg: Some(t.accent_block().0), ..Paint::default() }))
        }
        (true, None) => l.row.underline.push(0..width),
        _ => {}
    }
    l
}

/// Row `k` of the preview, `width` columns.
fn pane_line(p: Option<&Pane>, k: usize, width: u16, t: Option<&Tones>) -> Line {
    let mut l = Line::default();
    let Some(p) = p else {
        l.pad(width);
        return l;
    };
    if let (Some(note), 0) = (p.note, k) {
        l.put(&format!(" {}", fit(note, usize::from(width).saturating_sub(1)).0), Paint::default());
        l.row.spans.push((0..l.cols, Style::Secondary));
    } else if k + 1 < p.frame.rows.len() && p.note.is_none() {
        let src = &p.frame.rows[k];
        let cols = if src.text.chars().all(narrow) {
            src.text.chars().count()
        } else {
            fit(&src.text, usize::MAX).1
        } as u16;
        l.row = Row { text: src.text.clone(), spans: src.spans.clone(), underline: vec![] };
        l.cols = cols;
        l.looks = p.looks.get(k).cloned().unwrap_or_default();
        let lit = p.lit.filter(|lit| lit.rows.contains(&k));
        let marks = lit.iter().flat_map(|lit| lit.marks.iter().filter(|m| m.0 == k));
        l.looks.extend(marks.map(|m| (m.1.clone(), Look::Bold)));
        if let (Some(t), Some(lit)) = (t, lit) {
            let (back, _) = t.accent_block();
            let (solid, on) = t.accent_solid();
            let found = Paint { bg: Some(solid), fg: Some(on), bold: true, ..Paint::default() };
            l.paints.extend(lit.marks.iter().filter(|m| m.0 == k).map(|m| (m.1.clone(), found)));
            l.paints.push((0..width, Paint { bg: Some(back), ..Paint::default() }));
        }
    }
    l.pad(width);
    l
}

/// PICK-1, PICK-3, PICK-4, PICK-5: the rows of the window, `w.w` columns each (the window's top row
/// first), and the columns the query takes in the prompt. `t` is none where there are no tones:
/// the rows have no paints then.
pub fn compose(w: &Win, v: &View, t: Option<&Tones>) -> (Vec<Line>, usize) {
    let ink = |c: Option<si::core::color::Rgb>| Paint { fg: c, ..Paint::default() };
    let (frame, rule, faint) =
        (ink(t.map(|t| t.focus)), ink(t.map(|t| t.line)), ink(t.map(|t| t.dim)));
    let bold = Paint { bold: t.is_some(), ..Paint::default() }; // (calm has no paints)
    // The mode's name and the root after it, which is cut from the left to what the border has
    // left (the border's corner, a rule, the name, ` · `, a blank and a rule, in the part of it that holds the title).
    let seg = if w.stacked { w.w } else { w.list.w + 1 };
    let room = usize::from(seg).saturating_sub(4 + 4 + fit(v.title, usize::MAX).1 + 1);
    let scope = (room >= 2 && !v.scope.is_empty()).then(|| tail(v.scope, room));
    let title = match scope {
        Some(s) => vec![(format!(" {}", v.title), bold), (format!(" · {s} "), frame)],
        None => vec![(format!(" {} ", v.title), bold)],
    };
    let count_text = match v.spin {
        Some(c) => format!(" {} {c} ", v.count),
        None => format!(" {} ", v.count),
    };
    let taken = count_text.chars().count() as u16;
    let count = [(count_text, frame)];
    let pane_title = [(
        format!(" {} ", v.pane.as_ref().map_or(String::new(), |p| p.title.clone())),
        if v.pane.is_some() { bold } else { Paint::default() },
    )];
    let (q, qcols) = prompt(w.prompt.w, v.query, t);
    let mut list: Vec<Line> =
        (0..usize::from(w.list.h)).map(|i| list_line(v.rows.get(i), w.list.w, t)).collect();
    if let (true, Some(first)) = (v.none, list.first_mut()) {
        let mut l = Line::default();
        l.put(" no matches", Paint::default());
        l.row.spans.push((0..l.cols, Style::Secondary));
        l.pad(w.list.w);
        *first = l;
    }
    let pane = |k: usize| pane_line(v.pane.as_ref(), k, w.pane.w, t);
    // the keys, in the border's right end where they fit next to what is at its left
    let hint = |room: u16| -> Vec<(String, Paint)> {
        for keep in KEEP {
            let keys: Vec<&str> = keep.iter().map(|&k| KEYS[k]).collect();
            let text = format!(" {} ", keys.join("  "));
            if usize::from(room) >= text.chars().count() + 6 {
                return vec![(text, faint)];
            }
        }
        vec![]
    };
    let wall = |p: Paint| glyph("│", p);
    let mut rows = vec![];
    if w.stacked {
        rows.push(border(w.w, ['╭', '╮'], frame, &title, &[]));
        let boxed = |l: Line| {
            let mut line = wall(frame);
            line.add(l);
            line.add(wall(frame));
            line
        };
        rows.push(boxed(q));
        let mut fill = Line::default();
        fill.put(&"─".repeat(usize::from(w.list.w)), rule);
        let mut line = glyph("├", frame);
        line.add(fill);
        line.add(glyph("┤", frame));
        rows.push(line);
        rows.extend(list.into_iter().map(boxed));
        rows.push(border(w.w, ['├', '┤'], frame, &pane_title, &[]));
        rows.extend((0..usize::from(w.pane.h)).map(|k| boxed(pane(k))));
        rows.push(border(w.w, ['╰', '╯'], frame, &count, &hint(w.w.saturating_sub(taken))));
    } else {
        let (lw, pw) = (w.list.w, w.pane.w);
        let mut top = border(lw + 1, ['╭', '─'], frame, &title, &[]);
        top.add(border(pw + 2, ['┬', '╮'], frame, &pane_title, &[]));
        rows.push(top);
        let mut fill = Line::default();
        fill.put(&"─".repeat(usize::from(lw)), rule);
        for (k, left) in [q, fill].into_iter().chain(list).enumerate() {
            let sep = k == 1;
            let mut line = glyph(if sep { "├" } else { "│" }, frame);
            line.add(left);
            line.add(glyph(if sep { "┤" } else { "│" }, rule));
            line.add(pane(k));
            line.add(wall(frame));
            rows.push(line);
        }
        let mut bottom = border(lw + 1, ['╰', '─'], frame, &count, &[]);
        bottom.add(border(pw + 2, ['┴', '╯'], frame, &[], &hint(pw)));
        rows.push(bottom);
    }
    debug_assert_eq!(rows.len(), usize::from(w.h));
    if let Some(t) = t {
        for l in &mut rows {
            l.paints.push((0..w.w, Paint { bg: Some(t.cursor), ..Paint::default() }));
        }
    }
    (rows, qcols)
}

/// The pieces of a range that lie outside `a..z`.
fn outside(r: &Range<u16>, a: u16, z: u16) -> impl Iterator<Item = Range<u16>> {
    let left = (r.start < a).then(|| r.start..r.end.min(a));
    let right = (r.end > z).then(|| r.start.max(z)..r.end);
    left.into_iter().chain(right).filter(|r| !r.is_empty())
}

/// Cuts the ranges of `v` at `a..z`, in place and in order (the first range that has a cell
/// decides it): those inside are gone, those that reach into it are shortened, and one that
/// covers it all becomes two, side by side.
fn clip<T: Clone>(v: &mut Vec<(Range<u16>, T)>, a: u16, z: u16) {
    let (mut kept, mut right) = (0, vec![]);
    v.retain_mut(|(r, x)| {
        let keep = if r.start >= r.end || (r.start >= a && r.end <= z) {
            false
        } else {
            if r.start < a && r.end > z {
                right.push((kept, (z..r.end, x.clone())));
            }
            if r.start < a {
                r.end = r.end.min(a);
            } else if r.end > z {
                r.start = r.start.max(z);
            }
            true
        };
        kept += usize::from(keep);
        keep
    });
    for (n, (at, piece)) in right.into_iter().enumerate() {
        v.insert(at + 1 + n, piece);
    }
}

/// Whether a char takes exactly one column whatever is around it (so a row of them is cut by
/// counting): Latin text, box drawing, blocks and Braille, but not a char that can start an emoji
/// sequence (`©` `®`) and not a control.
fn narrow(c: char) -> bool {
    ((' '..'\u{300}').contains(&c) && !c.is_control() && c != '\u{a9}' && c != '\u{ae}')
        || ('\u{2500}'..='\u{259f}').contains(&c)
        || ('\u{2800}'..='\u{28ff}').contains(&c)
}

/// The text of a row left of column `a` and right of column `z`, and the columns the left one
/// takes, by si's `width`. A wide char the window cuts in two leaves a blank for the half that is
/// outside, and a combining char goes with the char it combines with.
fn walk(text: &str, a: u16, z: u16) -> (String, String, u16) {
    let cs: Vec<char> = text.chars().collect();
    // `side` is where the char before was: 0 left of the window, 1 under it, 2 right of it
    let (mut left, mut right, mut col, mut side, mut lw) =
        (String::new(), String::new(), 0u16, 0u8, 0u16);
    for i in 0..cs.len() {
        let w = width(&cs, i, usize::from(col)) as u16;
        let (s, e) = (col, col + w);
        if w == 0 {
            match side {
                0 => left.push(cs[i]),
                2 => right.push(cs[i]),
                _ => {}
            }
            continue;
        }
        side = if e <= a {
            0
        } else if s >= z {
            2
        } else {
            1
        };
        match side {
            0 => {
                left.push(cs[i]);
                lw = e;
            }
            2 => right.push(cs[i]),
            _ => {
                if s < a {
                    left.extend(std::iter::repeat_n(' ', usize::from(a - s)));
                    lw = a;
                }
                right.extend(std::iter::repeat_n(' ', usize::from(e.saturating_sub(z))));
            }
        }
        col = e;
    }
    (left, right, lw)
}

/// PICK-1: puts the line `o` over the columns `x..x + o.cols` of a screen row: the row's text,
/// styles, looks and paints are cut there and the line's put in, so nothing under the window shows
/// through. A wide char the window cuts in two is a blank (both halves of it), a row shorter than
/// `x` is padded up to it, and a char that combines with one under the window goes with it.
pub fn splice(row: &mut Row, looks: &mut Looks, paints: &mut Paints, x: u16, o: Line) {
    let (a, z) = (x, x + o.cols);
    // A row of chars that each take one column (nearly every row) is cut by counting chars; any
    // other is walked with si's `width`, which knows the wide, the combining and the emoji.
    let (left, right, lw) = if row.text.chars().all(narrow) {
        let at = |n: usize| row.text.char_indices().nth(n).map_or(row.text.len(), |(i, _)| i);
        let n = row.text.chars().count();
        (
            row.text[..at(usize::from(a))].to_string(),
            row.text[at(usize::from(z))..].to_string(),
            n.min(usize::from(a)) as u16,
        )
    } else {
        walk(&row.text, a, z)
    };
    let pad = " ".repeat(usize::from(a.saturating_sub(lw)));
    row.text = left + &pad + &o.row.text + &right;
    clip(&mut row.spans, a, z);
    if !row.underline.is_empty() {
        row.underline = row.underline.iter().flat_map(|r| outside(r, a, z)).collect();
    }
    clip(looks, a, z);
    clip(paints, a, z);
    let go = |r: &Range<u16>| r.start + x..r.end + x;
    row.spans.extend(o.row.spans.iter().map(|(r, s)| (go(r), *s)));
    row.underline.extend(o.row.underline.iter().map(go));
    looks.extend(o.looks.iter().map(|(r, l)| (go(r), *l)));
    paints.extend(o.paints.iter().map(|(r, p)| (go(r), *p)));
}

/// PICK-1: lays the window over the rows of a screen `cols` wide, its lines from `compose`. With
/// `shadow` (depth) the cells one column to its right and one row below it, shifted by one cell,
/// keep their text, fade toward the page and are on the shadow's color.
pub fn lay(
    rows: &mut [Row],
    looks: &mut [Looks],
    paints: &mut [Paints],
    cols: u16,
    w: &Win,
    lines: Vec<Line>,
    shadow: Option<&Tones>,
) {
    for (k, line) in lines.into_iter().enumerate() {
        let y = usize::from(w.y) + k;
        if let (Some(r), Some(l), Some(p)) = (rows.get_mut(y), looks.get_mut(y), paints.get_mut(y))
        {
            splice(r, l, p, w.x, line);
        }
    }
    let Some(t) = shadow else { return };
    let dark = Paint { bg: Some(t.shadow), fade: 70, ..Paint::default() };
    let (right, below) = (w.x + w.w, w.y + w.h);
    for y in usize::from(w.y) + 1..=usize::from(below) {
        let cells = if y == usize::from(below) { w.x + 1..right + 1 } else { right..right + 1 };
        if let Some(p) = paints.get_mut(y).filter(|_| y < rows.len()) {
            p.insert(0, (cells.start..cells.end.min(cols), dark));
        }
    }
}
