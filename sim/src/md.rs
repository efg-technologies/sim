//! MD-1, MD-2, MD-3, MD-4, MD-7, MD-8, MD-10
//! The read view of a Markdown file (pure): which chars of a line are not drawn and what is put
//! in their place (`Plan`), made from the line's chars and the highlighter's spans; the tables of
//! a screen, found once per frame (`Read`); and the map from the columns of a row as the core made
//! it to the columns drawn (`Remap`), which a click is taken back through. Everything that is left
//! out passes through `may_hide`.
//! A link is left whole (its brackets, text and URL are drawn as UI-8 draws them), so that a mark on
//! a link whose text and URL differ has all of it to work with.
use crate::paint::Look;
use crate::syn::{Spans, Text};
use sim_core::core::view::width;
use std::ops::Range;

/// A table of more rows than this is not read: finding its end would cost more than a key may.
const MAX_ROWS: usize = 400;
/// YAML front matter is looked for only if it closes within this many lines.
const FRONT: usize = 200;

/// Why a char may be left out of a line (MD-2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Why {
    /// A marker of emphasis or of inline code.
    Markup,
    /// A blank around a table's cell text, which the aligned cell replaces.
    Blank,
    /// A char of a delimiter row, which the rule of the aligned cells replaces.
    Rule,
}

/// MD-2: whether `c` may be left out for `why`. The one place that says so: a char shown in
/// another form (SAFE-8), or any char of text, is none of these.
pub fn may_hide(c: char, why: Why) -> bool {
    match why {
        Why::Markup => matches!(c, '*' | '_' | '`'),
        Why::Blank => c == ' ',
        Why::Rule => matches!(c, '-' | ':' | ' '),
    }
}

/// What is done at a char of a line: it is left out, and `fill` chars are put before it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Op {
    pub hide: bool,
    pub fill: Option<(char, u16)>,
}

/// How one line is drawn in the read view: an `Op` for each of its chars, and one for its end.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Plan {
    ops: Vec<Op>,
}

impl Plan {
    pub fn op(&self, i: usize) -> Op {
        self.ops.get(i).copied().unwrap_or_default()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

/// A run of lines that start with a pipe: its lines, and its columns (the widest cell and the
/// alignment of each) if it is a table that can be drawn whole (MD-4); else it is drawn raw.
#[derive(Debug)]
pub struct Table {
    lines: Range<usize>,
    cols: Option<Vec<(usize, Align)>>,
}

/// The read view of the lines on a screen.
#[derive(Debug)]
pub struct Read {
    raw: Range<usize>,
    front: usize,
    tables: Vec<Table>,
    /// The lines of the tables that are raw only for want of spans (MD-4): to be asked for.
    pub need: Option<Range<usize>>,
}

/// The pipes of a table's row, and whether text follows the last of them.
struct Cells {
    first: usize,
    pipes: Vec<usize>,
    tail: bool,
}

impl Cells {
    /// A row starts with a pipe, after at most three blanks; a pipe is a cell's edge unless a
    /// backslash escapes it.
    fn of(cs: &[char]) -> Option<Cells> {
        let first = cs.iter().position(|&c| c != ' ')?;
        if first > 3 || cs[first] != '|' {
            return None;
        }
        let escaped = |i: usize| cs[..i].iter().rev().take_while(|&&c| c == '\\').count() % 2 == 1;
        let pipes: Vec<usize> =
            (first..cs.len()).filter(|&i| cs[i] == '|' && !escaped(i)).collect();
        let tail = cs[pipes.last()? + 1..].iter().any(|&c| c != ' ');
        Some(Cells { first, pipes, tail })
    }

    fn count(&self) -> usize {
        (self.pipes.len() - 1) + usize::from(self.tail)
    }

    /// Cell `k` is between its pipe and the next, or the end of the line.
    fn cell(&self, k: usize, len: usize) -> Range<usize> {
        self.pipes[k] + 1..self.pipes.get(k + 1).copied().unwrap_or(len)
    }
}

/// MD-2: the markers the highlighter calls so, a run of one `*` or `_` (emphasis) or backtick
/// (inline code) at a time, and not next to a joiner (the cluster's width stays the core's).
fn marks(cs: &[char], sp: &Spans, ops: &mut [Op]) {
    for (r, look) in sp {
        let ok: &[char] = match look {
            Look::Mark => &['*', '_'],
            Look::Tick => &['`'],
            _ => continue,
        };
        let Some(run) = cs.get(r.clone()).filter(|x| !x.is_empty()) else { continue };
        let joins = |c: Option<&char>| matches!(c, Some('\u{200d}' | '\u{fe0f}'));
        if !ok.contains(&run[0]) || run.iter().any(|&c| c != run[0]) {
            continue;
        }
        if joins(cs.get(r.start.wrapping_sub(1))) || joins(cs.get(r.end)) {
            continue;
        }
        r.clone().for_each(|i| ops[i].hide = may_hide(cs[i], Why::Markup));
    }
}

/// A cell's text between its blanks, and its width with the markers it hides out. A cell that
/// the markers would leave empty keeps them.
fn cell(cs: &[char], ops: &mut [Op], at: Range<usize>) -> (Range<usize>, usize) {
    let used = |i: &usize| cs[*i] != ' ';
    let (a, b) = (at.clone().find(used), at.clone().rev().find(used));
    let Some((a, b)) = a.zip(b).map(|(a, b)| (a, b + 1)) else { return (at.end..at.end, 0) };
    if (a..b).all(|i| ops[i].hide || cs[i] == ' ') {
        (a..b).for_each(|i| ops[i].hide = false);
    }
    (a..b, (a..b).filter(|&i| !ops[i].hide).map(|i| width(cs, i, 0)).sum())
}

/// The alignment a delimiter row's cell asks for, if it is one: `-` between at most two colons.
fn aligned(cell: &[char]) -> Option<Align> {
    let t: Vec<char> = cell.iter().copied().filter(|&c| c != ' ').collect();
    let inner = t.strip_prefix(&[':']).unwrap_or(&t);
    let inner = inner.strip_suffix(&[':']).unwrap_or(inner);
    if inner.is_empty() || inner.iter().any(|&c| c != '-') {
        return None;
    }
    Some(match (t.first() == Some(&':'), t.last() == Some(&':') && t.len() > 1) {
        (true, true) => Align::Center,
        (false, true) => Align::Right,
        _ => Align::Left,
    })
}

fn pipe_row(s: &str) -> bool {
    let rest = s.trim_start_matches(' ');
    rest.starts_with('|') && s.len() - rest.len() <= 3
}

/// MD-1: the lines of YAML front matter (a first line `---`, then up to the next `---` or `...`):
/// the highlighter has no scope for it, and a `*` in a value is a value. The count of its lines,
/// the closing one included; none if it does not close.
fn front_matter(text: Text) -> usize {
    if text(0).is_none_or(|s| s.trim_end() != "---") {
        return 0;
    }
    (1..FRONT)
        .map_while(|l| text(l).map(|s| (l, s)))
        .find(|(_, s)| matches!(s.trim_end(), "---" | "..."))
        .map_or(0, |(l, _)| l + 1)
}

/// MD-3, MD-4: the columns of the table whose rows are `run`, if it can be drawn whole.
fn table<'a>(
    run: &Range<usize>,
    pane: usize,
    text: Text,
    spans: &dyn Fn(usize) -> Option<&'a Spans>,
) -> Option<Vec<(usize, Align)>> {
    if run.len() < 2 || run.len() > MAX_ROWS {
        return None;
    }
    let (mut cols, mut indent) = (vec![], 0);
    for (k, l) in run.clone().enumerate() {
        let cs: Vec<char> = text(l)?.chars().collect();
        let sp = spans(l)?;
        if sp.first().is_some_and(|s| s.1 == Look::Band) || cs.contains(&'\t') {
            return None;
        }
        let cells = Cells::of(&cs)?;
        let n = cells.count();
        if k == 0 {
            (indent, cols) = (cells.first, vec![(1, Align::Left); n]);
        }
        // (a row that wraps, or is indented otherwise, or has a cell too many, is not aligned)
        let raw: usize = (0..cs.len()).map(|i| width(&cs, i, 0)).sum();
        if n == 0 || cells.first != indent || raw > pane || n > cols.len() {
            return None;
        }
        if k == 1 {
            let marked = sp.iter().any(|(r, l)| *l == Look::Mark && r.contains(&cells.pipes[0]));
            let al: Option<Vec<Align>> =
                (0..n).map(|c| aligned(&cs[cells.cell(c, cs.len())])).collect();
            let rule = cs.iter().all(|&c| matches!(c, '|' | '-' | ':' | ' '));
            let al = al.filter(|a| marked && rule && a.len() == cols.len())?;
            cols.iter_mut().zip(al).for_each(|(c, a)| c.1 = a);
            continue;
        }
        let mut ops = vec![Op::default(); cs.len() + 1];
        marks(&cs, sp, &mut ops);
        for (c, col) in cols.iter_mut().enumerate().take(n) {
            col.0 = col.0.max(cell(&cs, &mut ops, cells.cell(c, cs.len())).1);
        }
    }
    let aligned: usize = indent + 1 + cols.iter().map(|c| c.0 + 3).sum::<usize>();
    (aligned <= pane).then_some(cols)
}

impl Read {
    /// The read view of `lines` (the lines on the screen), for a pane of `pane` columns. `raw` are
    /// the lines drawn as they are: the cursor's and the selected ones (MD-1).
    pub fn new<'a>(
        lines: Range<usize>,
        raw: Range<usize>,
        pane: usize,
        text: Text,
        spans: &dyn Fn(usize) -> Option<&'a Spans>,
    ) -> Read {
        let front = if lines.start < FRONT { front_matter(text) } else { 0 };
        let (mut tables, mut l, mut need) = (vec![], lines.start.max(front), None);
        let piped = |l: usize| text(l).is_some_and(|s| pipe_row(&s));
        while l < lines.end {
            if !piped(l) {
                l += 1;
                continue;
            }
            let (mut s, mut e) = (l, l + 1);
            while s > front && l - s < MAX_ROWS && piped(s - 1) {
                s -= 1;
            }
            while e - l < MAX_ROWS && piped(e) {
                e += 1;
            }
            let cols = table(&(s..e), pane, text, spans);
            if cols.is_none() && (s..e).any(|l| spans(l).is_none()) {
                need = Some(need.map_or(s..e, |n: Range<usize>| n.start.min(s)..n.end.max(e)));
            }
            tables.push(Table { lines: s..e, cols });
            l = e;
        }
        Read { raw, front, tables, need }
    }

    /// How line `l` (`cs`, with the highlighter's `sp`) is drawn, if it is drawn otherwise than raw.
    pub fn plan(&self, l: usize, cs: &[char], sp: &Spans) -> Option<Plan> {
        if self.raw.contains(&l) || l < self.front || sp.first().is_some_and(|s| s.1 == Look::Band)
        {
            return None;
        }
        let mut ops = vec![Op::default(); cs.len() + 1];
        marks(cs, sp, &mut ops);
        let at = self.tables.partition_point(|t| t.lines.end <= l);
        let tab = self.tables.get(at).filter(|t| t.lines.contains(&l));
        // (a run of lines that start with a pipe is a table that is aligned, or it is raw)
        match tab.map(|t| t.cols.as_ref().map(|c| (t.lines.start, c))) {
            Some(None) => return None,
            Some(Some((start, cols))) => row(cols, l - start, cs, &Cells::of(cs)?, &mut ops),
            // (a line that would be left with nothing visible keeps its markers)
            None if cs.iter().zip(&ops).all(|(c, o)| o.hide || c.is_whitespace()) => return None,
            None => {}
        }
        ops.iter().any(|o| o.hide || o.fill.is_some()).then_some(Plan { ops })
    }
}

/// MD-3: the ops of row `k` of a table of `cols`: the blanks of its cells go, and the aligned
/// cells' come; the delimiter row becomes the rule of the widths.
fn row(cols: &[(usize, Align)], k: usize, cs: &[char], cells: &Cells, ops: &mut [Op]) {
    let fill = |ops: &mut [Op], at: usize, c: char, n: usize| ops[at].fill = Some((c, n as u16));
    for (c, &(w, al)) in cols.iter().enumerate().take(cells.count()) {
        let at = cells.cell(c, cs.len());
        if k == 1 {
            at.clone().filter(|&i| may_hide(cs[i], Why::Rule)).for_each(|i| ops[i].hide = true);
            fill(ops, at.end, '─', w + 2);
            continue;
        }
        let (text, used) = cell(cs, ops, at.clone());
        for i in at.clone().filter(|i| !text.contains(i)) {
            ops[i].hide = may_hide(cs[i], Why::Blank);
        }
        if text.is_empty() {
            fill(ops, at.end, ' ', w + 2);
            continue;
        }
        let extra = w - used;
        let left = match al {
            Align::Left => 0,
            Align::Center => extra / 2,
            Align::Right => extra,
        };
        fill(ops, text.start, ' ', 1 + left);
        fill(ops, at.end, ' ', 1 + extra - left);
    }
    // blanks after the last pipe, when no text follows it
    if !cells.tail {
        let end = cells.pipes.last().map_or(cs.len(), |p| p + 1);
        (end..cs.len()).for_each(|i| ops[i].hide = may_hide(cs[i], Why::Blank));
    }
}

/// A piece of a row as the core made it, or what is put between pieces.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Piece {
    /// Columns and chars of the old row that stay.
    Keep {
        cols: Range<u16>,
        chars: Range<usize>,
    },
    Fill(char, u16),
}

/// MD-1: a row of the screen as the read view draws it, from the row as the core made it: what
/// stays of it in order, and what is put between. Columns and chars are of the whole row, its
/// gutter first.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Remap {
    pieces: Vec<Piece>,
}

impl Remap {
    pub fn keep(&mut self, cols: Range<u16>, chars: Range<usize>) {
        if let Some(Piece::Keep { cols: c, chars: h }) = self.pieces.last_mut()
            && c.end == cols.start
            && h.end == chars.start
        {
            (c.end, h.end) = (cols.end, chars.end);
            return;
        }
        self.pieces.push(Piece::Keep { cols, chars });
    }

    pub fn fill(&mut self, ch: char, n: u16) {
        self.pieces.push(Piece::Fill(ch, n));
    }

    /// The new row's text, from the old one's; the delimiter row's rule is made of `rule`.
    pub fn text(&self, old: &str, rule: char) -> String {
        let cs: Vec<char> = old.chars().collect();
        let mut out = String::new();
        for p in &self.pieces {
            match p {
                Piece::Keep { chars, .. } => {
                    out.extend(&cs[chars.start.min(cs.len())..chars.end.min(cs.len())]);
                }
                Piece::Fill(c, n) => {
                    out.extend(std::iter::repeat_n(if *c == '─' { rule } else { *c }, (*n).into()));
                }
            }
        }
        out
    }

    /// Where the old columns `r` are in the new row; none if all of them are left out.
    pub fn range(&self, r: Range<u16>) -> Option<Range<u16>> {
        let (mut at, mut span) = (0, None::<Range<u16>>);
        for p in &self.pieces {
            match p {
                Piece::Keep { cols, .. } => {
                    let (a, b) = (cols.start.max(r.start), cols.end.min(r.end));
                    if a < b {
                        let (s, e) = (at + a - cols.start, at + b - cols.start);
                        span = Some(span.map_or(s..e, |x| x.start..e));
                    }
                    at += cols.end - cols.start;
                }
                Piece::Fill(_, n) => at += n,
            }
        }
        span
    }

    /// Where the old column `c` is in the new row; none if it is left out. A column past the old
    /// row's end keeps its distance from it.
    pub fn col(&self, c: u16) -> Option<u16> {
        let (mut at, mut end) = (0, 0);
        for p in &self.pieces {
            match p {
                Piece::Keep { cols, .. } => {
                    if cols.contains(&c) {
                        return Some(at + c - cols.start);
                    }
                    (at, end) = (at + cols.end - cols.start, cols.end);
                }
                Piece::Fill(_, n) => at += n,
            }
        }
        (c >= end).then(|| at + c - end)
    }

    /// The new columns of what is put in other than blanks (the rule of a delimiter row).
    pub fn rules(&self) -> Vec<Range<u16>> {
        let (mut at, mut out) = (0, vec![]);
        for p in &self.pieces {
            match p {
                Piece::Keep { cols, .. } => at += cols.end - cols.start,
                Piece::Fill(c, n) => {
                    out.extend((*c != ' ').then_some(at..at + n));
                    at += n;
                }
            }
        }
        out
    }

    /// Where the old char `i` of the row is in the new text; none if it is left out. A char past
    /// the old row's end keeps its distance from it.
    pub fn idx(&self, i: usize) -> Option<usize> {
        let (mut at, mut end) = (0, 0);
        for p in &self.pieces {
            match p {
                Piece::Keep { chars, .. } => {
                    if chars.contains(&i) {
                        return Some(at + i - chars.start);
                    }
                    (at, end) = (at + chars.len(), chars.end);
                }
                Piece::Fill(_, n) => at += usize::from(*n),
            }
        }
        (i >= end).then(|| at + i - end)
    }

    /// PTR-1, MD-7: the old column that the new column `x` is over. In what is put in, the next
    /// char that stays; past the end, the distance from it.
    pub fn back(&self, x: u16) -> u16 {
        let (mut at, mut over, mut end) = (0, false, 0);
        for p in &self.pieces {
            match p {
                Piece::Keep { cols, .. } => {
                    let len = cols.end - cols.start;
                    if over {
                        return cols.start;
                    }
                    if x < at + len {
                        return cols.start + x - at;
                    }
                    (at, end) = (at + len, cols.end);
                }
                Piece::Fill(_, n) => {
                    over |= x >= at && x < at + n;
                    at += n;
                }
            }
        }
        end + x.saturating_sub(at)
    }
}

/// MD-10: the top-level domains that make a dotted name a host (a file or a product has a dot too).
const TLDS: &[&str] = &[
    "com", "org", "net", "edu", "gov", "io", "dev", "app", "ai", "co", "jp", "uk", "de", "fr",
    "info", "xyz", "me", "us", "eu", "ru", "cn", "in", "br", "ca", "au", "nl", "se", "ch",
    "example",
];

/// MD-10: what a link's text names: a host (lower case, no `www.`) or a path (and whether it is absolute).
enum Names {
    Host(String),
    Path(String, bool),
}

/// MD-10: the host of an authority (`user@host:port`), as it is compared.
fn host_of(authority: &str) -> String {
    let host = authority.rsplit('@').next().unwrap_or_default();
    let host = host
        .rsplit_once(':')
        .filter(|p| p.1.bytes().all(|b| b.is_ascii_digit()))
        .map_or(host, |p| p.0);
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    host.strip_prefix("www.").map_or(host.clone(), str::to_string)
}

/// MD-10: the part of `s` before a `?` or a `#`, without its leading `./`, `../`, `/` and `~/` and its
/// trailing `/`, as paths are compared.
fn trimmed(s: &str) -> &str {
    let s = s.split(['?', '#']).next().unwrap_or_default();
    s.trim_start_matches(['.', '/', '~']).trim_end_matches('/')
}

/// MD-10: what the text of a link names, if it looks like an address: it has no blank and is a URL, a
/// `www.` name, a host of a common top-level domain, or a path.
fn names(text: &str) -> Option<Names> {
    let t = text.trim_matches(|c: char| c.is_whitespace() || matches!(c, '*' | '_' | '`'));
    if t.is_empty() || t.contains(char::is_whitespace) {
        return None;
    }
    let scheme = t.split_once("://").filter(|(s, _)| {
        s.starts_with(|c: char| c.is_ascii_alphabetic())
            && s.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.'))
    });
    if let Some((_, rest)) = scheme {
        let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
        return Some(Names::Host(host_of(authority)));
    }
    let host = t.split(['/', '?', '#']).next().unwrap_or_default();
    let labels: Vec<&str> = host.split('.').collect();
    let label =
        |l: &&str| !l.is_empty() && l.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-');
    let last = labels.last().map(|l| l.to_ascii_lowercase()).unwrap_or_default();
    let www = host.len() > 4 && host[..4].eq_ignore_ascii_case("www.");
    if labels.len() > 1 && labels.iter().all(label) && (www || TLDS.contains(&last.as_str())) {
        return Some(Names::Host(host_of(host)));
    }
    let file = t.rsplit('/').next().unwrap_or_default();
    let ext = file.rsplit_once('.').map_or("", |p| p.1);
    let abs = t.starts_with(['/', '~']);
    let rooted = abs || t.starts_with("./") || t.starts_with("../");
    let named = t.contains('/')
        && (1..=8).contains(&ext.len())
        && ext.bytes().all(|b| b.is_ascii_alphanumeric());
    ((rooted || named) && !trimmed(t).is_empty()).then(|| Names::Path(trimmed(t).to_string(), abs))
}

/// MD-10: whether a link of `text` to `target` says one place and goes to another: the text looks like
/// an address that the target is not (another host; no host where the text has one; an absolute path
/// and a target with a host; a path that the target's does not end with, or neither ends with the other
/// where the target has no host), or the target runs code or carries data (`javascript:`, `data:`, `vbscript:`).
pub fn deceptive(text: &str, target: &str) -> bool {
    let target = target.trim().trim_matches(['<', '>']);
    let lower = target.trim_start_matches(|c: char| c.is_control()).to_ascii_lowercase();
    if ["javascript:", "data:", "vbscript:"].iter().any(|s| lower.starts_with(s)) {
        return true;
    }
    let Some(said) = names(text).filter(|_| !target.is_empty()) else { return false };
    let rest = target.split_once("://").map(|p| p.1).or_else(|| target.strip_prefix("//"));
    let (host, path) =
        rest.map_or(("", target), |r| r.split_at(r.find(['/', '?', '#']).unwrap_or(r.len())));
    match said {
        Names::Host(h) => host.is_empty() || host_of(host) != h,
        Names::Path(p, abs) => {
            let q = trimmed(path);
            // (a path the text says is on this machine goes nowhere by a host; a relative one may be a
            // repository's: the target's path then ends with it)
            q.is_empty()
                || if host.is_empty() {
                    !(p.ends_with(q) || q.ends_with(&p))
                } else {
                    abs || !q.ends_with(&p)
                }
        }
    }
}

/// MD-10: the chars of the line `cs` that are the target of an inline link whose text is deceptive:
/// what follows a `](` up to the blank or the `)` that ends it (a title is not a target, and a `<…>` is
/// trimmed by the judgment; an image's
/// `![` opens no link's text; a reference or an autolink has no `](`). Found by the chars, since the
/// highlighter does not take a URL in strong text for a link's, and only in a line that the
/// highlighter calls Markdown (it has a link or a URL) and outside inline code.
pub fn deceptive_targets(cs: &[char], sp: &Spans) -> Vec<Range<usize>> {
    let md = sp.iter().any(|s| matches!(s.1, Look::Link | Look::Url));
    let code =
        |i: usize| sp.iter().any(|(r, l)| matches!(l, Look::Code | Look::Band) && r.contains(&i));
    let mut out: Vec<Range<usize>> = vec![];
    for i in (0..cs.len().saturating_sub(1)).filter(|&i| md && cs[i] == ']' && cs[i + 1] == '(') {
        let from = out.last().map_or(0, |r| r.end);
        let Some(open) = cs[from..i].iter().rposition(|&c| c == '[').map(|j| from + j) else {
            continue;
        };
        let (start, mut end, mut parens) = (i + 2, i + 2, 0);
        while let Some(&c) = cs.get(end) {
            if c.is_whitespace() || (c == ')' && parens == 0) {
                break;
            }
            parens += i32::from(c == '(') - i32::from(c == ')');
            end += 1;
        }
        let image = open > 0 && cs[open - 1] == '!';
        let (text, target): (String, String) =
            (cs[open + 1..i].iter().collect(), cs[start..end].iter().collect());
        if start < end && !image && !code(i) && deceptive(&text, &target) {
            out.push(start..end);
        }
    }
    out
}
