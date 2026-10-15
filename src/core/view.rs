//! CLI-5, VIS-1, VIS-2, VIS-3, VIS-6, VIS-7, VIS-8, VIS-9, VIS-10, VIS-11, VIS-12, VI-5, VI-12, SAFE-8, PERF-2, PROP-5, PROP-8
//! State to Frame (a grid of characters and styles), wrapping, kinsoku.
use super::app::{App, plural};
use super::buffer::{Buffer, Hunk};
use super::proposal::clip;
use super::vi::Mode;
use std::mem::take;
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
#[rustfmt::skip]
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Row { pub text: String, pub spans: Vec<(Range<u16>, Style)>, pub underline: Vec<Range<u16>> }

#[rustfmt::skip]
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Frame { pub width: u16, pub height: u16, pub rows: Vec<Row>, pub cursor: (u16, u16), pub shape: Shape }

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

/// Where each row of a wrapped line starts (char indexes). A row breaks after a space or
/// between two wide chars when it can, and is cut at the edge otherwise.
fn wrap(cs: &[char], max: usize) -> Vec<usize> {
    let wide = |c: char| c.width() == Some(2);
    let (mut starts, mut s, mut i, mut col, mut brk) = (vec![0], 0, 0, 0, 0);
    while i < cs.len() {
        let w = width(cs, i, col);
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

/// What the status line says in REVIEW after its path, position and `REVIEW i/n` (PROP-8).
const REVIEW_KEYS: &str = "y accept · n reject · Y all · N none · Esc back";

fn status(a: &App) -> String {
    let b = &a.buf;
    #[rustfmt::skip]
    let mode = match a.vi.mode {
        Mode::Insert => "INSERT".into(), Mode::Visual => "VISUAL".into(), Mode::VLine => "V-LINE".into(),
        Mode::Review => format!("REVIEW {}/{}", a.current() + 1, b.hunks.len()), _ => "NORMAL".into(),
    };
    // A change on disk is a run of marked lines next to each other.
    let runs = (0..b.marks.len()).filter(|&i| i == 0 || b.marks[i - 1].line + 1 < b.marks[i].line);
    let count = |n: usize, one: &str, many: &str| (n > 0).then(|| plural(n, one, many));
    let notes: Vec<String> = [
        b.dirty().then(|| "modified".into()),
        b.readonly.clone(),
        a.gone.then(|| "deleted on disk".into()),
        count(runs.count(), "change on disk", "changes on disk"),
        count(b.hunks.len(), "proposal", "proposals"),
        a.filter
            .as_ref()
            .map(|r| format!("filter {}s · Ctrl-c cancel", a.tick.saturating_sub(r.at) / 1000)),
    ]
    .into_iter()
    .flatten()
    .collect();
    let notes = if a.vi.mode == Mode::Review {
        format!("  {REVIEW_KEYS}")
    } else if notes.is_empty() {
        String::new()
    } else {
        format!("  {}", notes.join(" · "))
    };
    let name = a.path.as_deref().unwrap_or("[no name]");
    format!("{name}  {}:{}  {mode}{notes}", b.line_of(b.cursor) + 1, b.col() + 1)
}

/// A line as it is drawn before wrapping: its chars with a style each, the gutter symbol, and the
/// number shown (VIS-3, VIS-11). A buffer line is one of them; in REVIEW a hunk makes several.
#[rustfmt::skip]
struct Disp { cs: Vec<char>, sty: Vec<Style>, sym: char, num: Option<usize> }

/// Buffer line `l` as it is drawn (VIS-2, VIS-3): the gutter symbol (a blank if nothing changed
/// there), and the change style on the marks of changes from disk and on the words of mine that a
/// hunk would replace. `dim`: in REVIEW a line nothing changed in is secondary.
fn plain(b: &Buffer, hunks: &[&Hunk], l: usize, dim: bool) -> Disp {
    let cs: Vec<char> = b.text.line(l).chars().take(b.line_len(l)).collect();
    let (mut sym, mut paints) = (' ', vec![]);
    if let Ok(i) = b.marks.binary_search_by_key(&l, |m| m.line) {
        sym = b.marks[i].kind;
        match &b.marks[i].words {
            Some(w) => paints.extend(w.iter().cloned()),
            None => paints.push(0..cs.len()),
        }
    }
    for h in hunks {
        if h.mine.contains(&l) {
            let at = b.start(l) - b.start(h.mine.start);
            match &h.words {
                Some(w) => paints.extend(w.0.iter().filter_map(|r| clip(r, at, cs.len()))),
                None => paints.push(0..cs.len()),
            }
            sym = '~';
        } else if h.mine.is_empty() && h.mine.start.min(b.lines() - 1) == l {
            sym = '~';
        }
    }
    let mut sty = vec![if dim && sym == ' ' { Style::Secondary } else { Style::Primary }; cs.len()];
    for r in paints {
        if let Some(x) = sty.get_mut(r.start.min(cs.len())..r.end.min(cs.len())) {
            x.fill(Style::Change);
        }
    }
    Disp { cs, sty, sym, num: Some(l + 1) }
}

/// A hunk as REVIEW draws it (VIS-3): mine and theirs woven into one text by the word diff, cut
/// into rows. Words of mine that would go are Deleted, words of theirs that would come are on the
/// Change background, the rest is as it is. Each row comes with the line it is drawn at: a row of
/// theirs alone has a `+` and no number, and follows the row before it. At most `cap` lines of
/// each side are looked at (a hunk as big as the file).
fn inline(b: &Buffer, h: &Hunk, cap: usize) -> Vec<(usize, Disp)> {
    let s = h.mine.start;
    let mine: Vec<char> = (s..h.mine.end.min(b.lines()).min(s.saturating_add(cap)))
        .flat_map(|l| b.text.line(l).chars().take(b.line_len(l)).chain(['\n']))
        .collect();
    let theirs: Vec<char> = h.theirs.split_inclusive('\n').take(cap).flat_map(str::chars).collect();
    // Which chars of each side the word diff calls changed (all of them without one).
    let flags = |rs: Option<&Vec<Range<usize>>>, n: usize| {
        let mut v = vec![rs.is_none(); n];
        for r in rs.into_iter().flatten().take_while(|r| r.start < n) {
            if let Some(x) = v.get_mut(r.start.min(n)..r.end.min(n)) {
                x.fill(true);
            }
        }
        v
    };
    let w = h.words.as_deref();
    let (gone, come) = (flags(w.map(|w| &w.0), mine.len()), flags(w.map(|w| &w.1), theirs.len()));
    // The weave: changed chars of mine first, then of theirs, and the chars both sides share side by
    // side (what does not pair up counts as changed), cut into rows. `line` is the next line of mine
    // to end, and `fresh` whether the row starts it.
    let (mut rows, mut line, mut fresh, mut last) = (vec![], s, true, s);
    let (mut cs, mut sty, mut has, mut p, mut q) = (vec![], vec![], false, 0, 0);
    while p < mine.len() || q < theirs.len() {
        let (c, y, m) = if p < mine.len() && (gone[p] || q == theirs.len()) {
            p += 1;
            (mine[p - 1], Style::Deleted, true)
        } else if q < theirs.len() && (come[q] || p == mine.len()) {
            q += 1;
            (theirs[q - 1], Style::Change, false)
        } else {
            (p, q) = (p + 1, q + 1);
            (mine[p - 1], Style::Primary, true)
        };
        if c != '\n' {
            cs.push(c);
            sty.push(y);
            has |= m;
            continue;
        }
        // A row has something of a line of mine if it holds a char of it, or is an empty one.
        let own = has || (m && fresh);
        let owner = if own { line } else { last };
        #[rustfmt::skip]
        let sym = if !own { '+' } else if fresh { '~' } else { ' ' };
        let num = (own && fresh).then_some(line + 1);
        rows.push((owner, Disp { cs: take(&mut cs), sty: take(&mut sty), sym, num }));
        (last, has) = (owner, false);
        if m {
            (line, fresh) = (line + 1, true);
        } else if own {
            fresh = false;
        }
    }
    rows
}

/// REVIEW (VIS-3): every hunk is drawn in place of its lines of mine, and the rows of theirs alone
/// follow the row before them (an empty hunk's go before the line after it, or after the last one).
fn weave(b: &Buffer, hunks: &[&Hunk], disp: &mut [Vec<Disp>], lo: usize, rows: usize) {
    let (n, hi) = (b.lines(), lo + disp.len());
    for h in hunks {
        let (s, e) = (h.mine.start.min(n), h.mine.end.min(n));
        let rs = inline(b, h, hi.saturating_sub(s) + rows);
        if s == e {
            if let Some(g) = s.min(n - 1).checked_sub(lo).and_then(|i| disp.get_mut(i)) {
                let at = if s < n { 0 } else { g.len() };
                g.splice(at..at, rs.into_iter().map(|r| r.1));
            }
            continue;
        }
        (s.max(lo)..e.min(hi)).for_each(|l| disp[l - lo].clear());
        for (owner, d) in rs {
            if let Some(g) = owner.checked_sub(lo).and_then(|i| disp.get_mut(i)) {
                g.push(d);
            }
        }
    }
    for (i, g) in disp.iter_mut().enumerate().filter(|(_, g)| g.is_empty()) {
        g.push(plain(b, &[], lo + i, true));
    }
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
    // While a search is typed the screen is drawn around its first match (VI-14).
    let peek = a.pv.filter(|_| a.vi.mode == Mode::Cmd).map(|p| p.min(b.text.len_chars()));
    let cursor = peek.unwrap_or(b.cursor);
    let cl = b.line_of(cursor);
    let (lo, hi) = (cl.saturating_sub(rows), (cl + rows + 1).min(b.lines()));
    // The hunks that can touch a row of this screen, found once: they are sorted by their first
    // line, and there may be thousands of them.
    let near = b.hunks.partition_point(|h| h.mine.start <= hi);
    let hunks: Vec<&Hunk> = b.hunks[..near].iter().filter(|h| h.mine.end >= lo).collect();
    let review = a.vi.mode == Mode::Review;
    let shown: &[&Hunk] = if review { &[] } else { &hunks };
    let mut disp: Vec<Vec<Disp>> = (lo..hi).map(|l| vec![plain(b, shown, l, review)]).collect();
    if review {
        weave(b, &hunks, &mut disp, lo, rows);
    }
    let wrapped = |d: Disp| (wrap(&d.cs, max), d);
    let lay: Vec<Vec<(Vec<usize>, Disp)>> =
        disp.into_iter().map(|g| g.into_iter().map(wrapped).collect()).collect();
    let height = |l: usize| lay[l - lo].iter().map(|g| g.0.len()).sum::<usize>();
    // In REVIEW the cursor stands at the start of the hunk's first row.
    let (st, d0) = &lay[cl - lo][0];
    let ci = if review { 0 } else { (cursor - b.start(cl)).min(d0.cs.len()) };
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
        // The rows of this line, after the first `first`: its wrapped rows, or its group's in REVIEW.
        let parts = lay[l - lo].iter().flat_map(|(st, d)| (0..st.len()).map(move |i| (st, d, i)));
        for (st, d, i) in parts.skip(first) {
            let (s, e) = (st[i], st.get(i + 1).copied().unwrap_or(d.cs.len()));
            let seg = |x: usize, y: usize| paint(&d.cs, x..y, max);
            let col = |x: usize| (gutter + seg(s, x).1) as u16;
            // Only the line the cursor is on has its number in primary (VIS-11).
            #[rustfmt::skip]
            let (head, mut spans) = match (i, d.num) {
                (0, Some(n)) => (format!("{}{n:>digits$} ", d.sym), if n == cl + 1 { vec![] } else { vec![(1..1 + digits as u16, Style::Secondary)] }),
                (0, None) => (format!("{}{:digits$} ", d.sym, ""), vec![]),
                _ => (" ".repeat(gutter), vec![]),
            };
            if i == 0 && d.sym != ' ' {
                spans.push((0..1, Style::Change));
            }
            let mut x = s;
            while x < e {
                let end = (x..e).find(|&z| d.sty[z] != d.sty[x]).unwrap_or(e);
                if d.sty[x] != Style::Primary {
                    spans.push((col(x)..col(end), d.sty[x]));
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
        Mode::Review => Shape::Underline,
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
        (fit(&a.msg.clone().unwrap_or_else(|| status(a)), w), cur.0)
    };
    out.push(Row { text, ..Row::default() });
    if peek.is_none() {
        (a.top, a.skip) = (top, skip);
    }
    let y = if a.vi.mode == Mode::Cmd { rows } else { cur.1.min(rows - 1) };
    let cursor = (x.min(w - 1) as u16, y as u16);
    Frame { width: w as u16, height: h as u16, rows: out, cursor, shape }
}

/// CLI-5: what si is and the keys to start with, centered below the first row, as many lines as fit.
fn intro(out: &mut [Row], w: usize) {
    #[rustfmt::skip]
    const KEYS: [(&str, &str); 6] = [
        ("i / Esc", "type / stop typing"), ("Ctrl-s", "save"), ("ZZ", "save and quit"), ("q", "quit"),
        ("ZQ", "quit without saving"), ("]c then y / n", "review a change: accept / reject"),
    ];
    let title = format!("si {}", env!("CARGO_PKG_VERSION"));
    let tagline = "A vi for reading, where only the changes light up.";
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
