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
use std::collections::HashMap;
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

/// MD-10: the two-letter last labels that end the name of a source file far more often than a
/// country's host (`main.rs`, `README.md`, `Makefile.am`, `Node.js`): a name that ends so is a file's.
/// Any other two letters are taken for a country code.
const EXTS: &[&str] = &[
    "rs", "md", "py", "sh", "pl", "ps", "cc", "so", "mk", "mm", "am", "ac", "ml", "pm", "tf", "gd",
    "kt", "sv", "hs", "ts", "js", "cs", "rb", "go", "el", "ex", "jl", "vb",
];

/// MD-10: how many lines of a document are scanned for the definitions of reference links.
pub const REF_LINES: usize = 2_000;

/// MD-10: what a word of a link's text names: a host (as it is compared, and as it is written), a URL
/// with an authority that cannot be read, or a path (and whether it is absolute).
enum Names {
    Host(String, String),
    Odd,
    Path(String, bool),
}

/// MD-10: where a target goes: to a host, to a host that cannot be read, or nowhere (relative).
enum Dest {
    Host(String),
    Odd,
    Relative,
}

fn scheme_ok(s: &str) -> bool {
    s.starts_with(|c: char| c.is_ascii_alphabetic())
        && s.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.'))
}

/// MD-10: the host of an authority as a browser reads it: what follows the last `@`, then an optional
/// port (digits), in lower case without a trailing dot or `www.`. Only ASCII letters, digits, `-`, `_`
/// and `.` (or an IPv6 address in brackets) are a host: a `%`, a `\`, a control char, a blank, an
/// entity or a char that is not ASCII is odd, and an odd host is never taken for another.
fn authority(a: &str) -> Dest {
    let hp = a.rsplit('@').next().unwrap_or_default();
    let (host, port) = match hp.rsplit_once(':') {
        Some((h, p)) if !hp.starts_with('[') || h.ends_with(']') => (h, Some(p)),
        _ => (hp, None),
    };
    if port.is_some_and(|p| !p.bytes().all(|b| b.is_ascii_digit())) {
        return Dest::Odd;
    }
    if let Some(v6) = host.strip_prefix('[').and_then(|h| h.strip_suffix(']')) {
        let ok =
            !v6.is_empty() && v6.bytes().all(|b| b.is_ascii_hexdigit() || matches!(b, b':' | b'.'));
        return if ok { Dest::Host(host.to_ascii_lowercase()) } else { Dest::Odd };
    }
    let h = host.strip_suffix('.').unwrap_or(host);
    let valid = |b: u8| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.');
    if h.is_empty() || !h.bytes().all(valid) {
        return Dest::Odd;
    }
    let h = h.to_ascii_lowercase();
    Dest::Host(h.strip_prefix("www.").map_or(h.clone(), str::to_string))
}

/// MD-10: `raw` without the tab and newline a browser drops, and without the controls and blanks it
/// trims at the ends.
fn cleaned(raw: &str) -> String {
    let t: String = raw.chars().filter(|c| !matches!(c, '\t' | '\n' | '\r')).collect();
    t.trim_matches(|c: char| c.is_control() || c == ' ').to_string()
}

/// MD-10: a URL as the WHATWG standard reads it, so that sim compares the host a browser goes to: the
/// special schemes (http, https, ws, wss, ftp, file) take any number of `/` and `\` after the colon, or
/// none, and end the authority at `/`, `\`, `?` or `#`; a target with no scheme that starts with two
/// slashes of either kind is scheme-relative; the userinfo is what precedes the last `@`. Where it goes,
/// and the path after the authority (the whole target where it goes nowhere).
fn url(raw: &str) -> (Dest, String) {
    let t = cleaned(raw.trim().trim_matches(['<', '>']));
    let (scheme, rest) = match t.split_once(':') {
        Some((s, r)) if scheme_ok(s) => (Some(s.to_ascii_lowercase()), r),
        _ => (None, t.as_str()),
    };
    let special = scheme
        .as_deref()
        .is_some_and(|s| matches!(s, "http" | "https" | "ws" | "wss" | "ftp" | "file"));
    let slash = |c: char| c == '/' || c == '\\';
    let mut cs = rest.chars();
    let two = cs.next().is_some_and(slash) && cs.next().is_some_and(slash);
    let (from, back) = match (special, scheme.is_some(), two) {
        (true, ..) => (rest.trim_start_matches(slash), true),
        (_, false, true) => (&rest[2..], true),
        (_, true, _) if rest.starts_with("//") => (&rest[2..], false),
        _ => return (Dest::Relative, rest.to_string()),
    };
    let end = from
        .find(|c: char| matches!(c, '/' | '?' | '#') || (back && c == '\\'))
        .unwrap_or(from.len());
    let (auth, path) = (&from[..end], from[end..].to_string());
    match authority(auth) {
        Dest::Odd if scheme.as_deref() == Some("file") && matches!(auth, "" | "localhost") => {
            (Dest::Relative, path)
        }
        d => (d, path),
    }
}

/// MD-10: whether a target runs code or carries data (`javascript:`, `data:`, `vbscript:`), through
/// tabs, blanks, controls and case, or has an entity before its first `/` that a browser would decode
/// into one (`javascript&colon;`) and that sim cannot.
fn runs_code(raw: &str) -> bool {
    let t = cleaned(raw).to_ascii_lowercase();
    if ["javascript:", "data:", "vbscript:"].iter().any(|s| t.starts_with(s)) {
        return true;
    }
    let head = t.split('/').next().unwrap_or_default();
    head.match_indices('&').any(|(i, _)| {
        let rest = &head[i + 1..];
        rest.split_once(';').is_some_and(|(e, _)| {
            !e.is_empty() && e.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'#')
        })
    })
}

/// MD-10: the part of `s` before a `?` or a `#`, without its leading `./`, `../`, `/` and `~/` and its
/// trailing `/`, as paths are compared.
fn trimmed(s: &str) -> &str {
    let s = s.split(['?', '#']).next().unwrap_or_default();
    s.trim_start_matches(['.', '/', '~']).trim_end_matches('/')
}

/// MD-10: the words of a link's text, without the brackets, quotes and markers that surround one and
/// the punctuation that ends one.
fn words(text: &str) -> impl Iterator<Item = &str> {
    let (open, shut) = (
        ['(', '[', '{', '<', '"', '\'', '*', '_', '`'],
        [')', ']', '}', '>', '"', '\'', '*', '_', '`'],
    );
    text.split(char::is_whitespace)
        .map(move |w| {
            w.trim_start_matches(open)
                .trim_end_matches(shut)
                .trim_end_matches([',', ';', ':', '!', '?', '.'])
        })
        .filter(|w| !w.is_empty())
        .take(32)
}

/// MD-10: what a word of a link's text names, if it looks like an address: a URL, a `www.` name, a host
/// of a common top-level domain or of a country (any two letters but the ending of a source file), or a
/// path (`/…`, `~/…`, `./…`, `../…`, or a name with a `/` and an extension).
fn address(w: &str) -> Option<Names> {
    if w.split_once("://").is_some_and(|(s, _)| scheme_ok(s)) {
        return Some(match url(w).0 {
            Dest::Host(h) => Names::Host(h, w.to_string()),
            _ => Names::Odd,
        });
    }
    let host = &w[..w.find(['/', '\\', '?', '#']).unwrap_or(w.len())];
    let labels: Vec<&str> = host.split('.').collect();
    let label =
        |l: &&str| !l.is_empty() && l.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-');
    let last = labels.last().map(|l| l.to_ascii_lowercase()).unwrap_or_default();
    let www = host.get(..4).is_some_and(|p| p.eq_ignore_ascii_case("www."));
    let country = last.len() == 2
        && last.bytes().all(|b| b.is_ascii_alphabetic())
        && !EXTS.contains(&last.as_str());
    if let (true, Dest::Host(h)) = (
        labels.len() > 1
            && labels.iter().all(label)
            && (www || country || TLDS.contains(&last.as_str())),
        authority(host),
    ) {
        return Some(Names::Host(h, w.to_string()));
    }
    let ext = w.rsplit('/').next().unwrap_or_default().rsplit_once('.').map_or("", |p| p.1);
    let abs = w.starts_with(['/', '~']);
    let rooted = abs || w.starts_with("./") || w.starts_with("../");
    let named = w.contains('/')
        && (1..=8).contains(&ext.len())
        && ext.bytes().all(|b| b.is_ascii_alphanumeric());
    ((rooted || named) && !trimmed(w).is_empty()).then(|| Names::Path(trimmed(w).to_string(), abs))
}

/// MD-10: whether a link of `text` to `target` says one place and goes to another: some word of the
/// text looks like an address that the target is not (another host, a host that cannot be read, no host
/// where the text has one unless the target is a file that ends with it; an absolute path and a target
/// with a host; a path that the target's does not end with, or that neither ends with where the target
/// has no host), or the target runs code or carries data.
pub fn deceptive(text: &str, target: &str) -> bool {
    // what a reader sees and where a renderer goes are what is left after the backslash escapes and the
    // entities are decoded; both the written and the decoded forms are judged
    let (t, g) = (unescape(text), unescape(target));
    let texts: &[&str] = if t == text { &[text] } else { &[text, &t] };
    let targets: &[&str] = if g == target { &[target] } else { &[target, &g] };
    texts.iter().any(|a| targets.iter().any(|b| deceptive_as_written(a, b)))
}

/// MD-10: the named entities that are ASCII punctuation; a name of another kind is left as it is (sim
/// has no table of them).
#[rustfmt::skip]
const NAMED: [(&str, char); 17] = [
    ("amp", '&'), ("lt", '<'), ("gt", '>'), ("quot", '"'), ("apos", '\''), ("period", '.'),
    ("sol", '/'), ("bsol", '\\'), ("colon", ':'), ("num", '#'), ("commat", '@'), ("percnt", '%'),
    ("quest", '?'), ("lowbar", '_'), ("hyphen", '-'), ("Tab", '\t'), ("NewLine", '\n'),
];

/// MD-10: the char an entity stands for, if it is a numeric one or one of `NAMED`.
fn entity(name: &str) -> Option<char> {
    let Some(n) = name.strip_prefix('#') else {
        return NAMED.iter().find(|p| p.0 == name).map(|p| p.1);
    };
    let v = match n.strip_prefix(['x', 'X']) {
        Some(h) => u32::from_str_radix(h, 16).ok(),
        None => n.parse().ok(),
    };
    v.and_then(char::from_u32).filter(|&c| c != '\0')
}

/// MD-10: `s` as CommonMark reads it: a backslash before an ASCII punctuation char is dropped, and an
/// entity that `entity` knows is the char.
fn unescape(s: &str) -> String {
    let cs: Vec<char> = s.chars().collect();
    let (mut out, mut i) = (String::new(), 0);
    while i < cs.len() {
        let rest = cs.get(i + 1..).unwrap_or_default();
        let named = (cs[i] == '&').then(|| rest.iter().take(12).position(|&c| c == ';')).flatten();
        let name: String = named.map(|e| rest[..e].iter().collect()).unwrap_or_default();
        let plain = name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'#');
        match (cs[i], named.filter(|_| plain).and_then(|e| entity(&name).map(|c| (c, e)))) {
            ('\\', _) if cs.get(i + 1).is_some_and(char::is_ascii_punctuation) => {
                out.push(cs[i + 1]);
                i += 2;
            }
            (_, Some((c, e))) => {
                out.push(c);
                i += e + 2;
            }
            (c, _) => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

fn deceptive_as_written(text: &str, target: &str) -> bool {
    if runs_code(target) {
        return true;
    }
    let (dest, path) = url(target);
    let q = trimmed(&path);
    if cleaned(target.trim().trim_matches(['<', '>'])).is_empty() {
        return false;
    }
    words(text).filter_map(address).any(|n| match n {
        Names::Odd => true,
        Names::Host(h, raw) => match &dest {
            Dest::Host(t) => *t != h,
            Dest::Odd => true,
            Dest::Relative => !q.ends_with(raw.as_str()),
        },
        Names::Path(p, abs) => match dest {
            Dest::Relative => q.is_empty() || !(p.ends_with(q) || q.ends_with(&p)),
            _ => q.is_empty() || abs || !q.ends_with(&p),
        },
    })
}

/// MD-10: the `[` and `]` of a line that close each other, inner pairs first: a bracket that a backslash
/// escapes is none, and a `[` that nothing closes pairs with nothing.
fn pairs(cs: &[char]) -> Vec<(usize, usize)> {
    let escaped = |j: usize| cs[..j].iter().rev().take_while(|&&c| c == '\\').count() % 2 == 1;
    let (mut open, mut out) = (vec![], vec![]);
    for (j, &c) in cs.iter().enumerate() {
        match c {
            '[' if !escaped(j) => open.push(j),
            ']' if !escaped(j) => out.extend(open.pop().map(|o| (o, j))),
            _ => {}
        }
    }
    out
}

/// MD-10: the inline links of the line `cs` whose text is deceptive: the text with its brackets, and
/// the target: what follows a `](` and its blanks, a `<…>` whole (it may hold blanks and tabs) or up to
/// the blank or the `)` that ends it (a title is not a target; an image's `![` is no link's; a
/// reference or an autolink has no `](`). Found by the chars, with the text's brackets nested and
/// escaped as CommonMark finds them, since the highlighter does not take a URL in strong text for a
/// link's, and outside inline code (`sp` has where it is).
fn link_marks(cs: &[char], sp: &[(Range<usize>, Look)]) -> Vec<(Range<usize>, Range<usize>)> {
    let code =
        |i: usize| sp.iter().any(|(r, l)| matches!(l, Look::Code | Look::Band) && r.contains(&i));
    let mut out = vec![];
    for (o, c) in pairs(cs) {
        let image = o > 0 && cs[o - 1] == '!';
        if cs.get(c + 1) != Some(&'(') || image || code(c) {
            continue;
        }
        let start = (c + 2..cs.len()).find(|&j| !cs[j].is_whitespace()).unwrap_or(cs.len());
        let end = if cs.get(start) == Some(&'<') {
            let Some(e) = cs[start..].iter().position(|&x| x == '>') else { continue };
            start + e + 1
        } else {
            let (mut end, mut parens) = (start, 0);
            while let Some(&x) = cs.get(end) {
                if x.is_whitespace() || (x == ')' && parens == 0) {
                    break;
                }
                parens += i32::from(x == '(') - i32::from(x == ')');
                end += 1;
            }
            end
        };
        let (text, target): (String, String) =
            (cs[o + 1..c].iter().collect(), cs[start..end].iter().collect());
        if start < end && deceptive(&text, &target) {
            out.push((o..c + 1, start..end));
        }
    }
    out.sort_by_key(|m| m.1.start);
    out
}

/// MD-10: the chars of the line `cs` that are the target of an inline link whose text is deceptive.
pub fn deceptive_targets(cs: &[char], sp: &[(Range<usize>, Look)]) -> Vec<Range<usize>> {
    link_marks(cs, sp).into_iter().map(|m| m.1).collect()
}

/// MD-10: whether the line `cs` may have a link that goes over lines: a `](` that no `[` of the line
/// opens, or a line that ends with the `](` of one whose target is on the next (`prev` is the line
/// before: its target is on this one).
pub fn crosses(prev: &[char], cs: &[char]) -> bool {
    let ends = |c: &[char]| {
        c.iter()
            .rposition(|x| !x.is_whitespace())
            .is_some_and(|i| i >= 1 && c[i] == '(' && c[i - 1] == ']')
    };
    let closes: Vec<usize> = pairs(cs).iter().map(|p| p.1).collect();
    let open = cs.windows(2).enumerate().any(|(j, w)| w == [']', '('] && !closes.contains(&j));
    open || ends(cs) || ends(prev)
}

/// MD-10: the marks of line `at` of `lines`, the lines of a paragraph (none blank): the links of the
/// paragraph are found as if it were one line, a line break being a blank; what falls on this line is the
/// target of a link that lies, or, where the target is on another line, the part of its text that is on
/// this one, so that each line of such a link has its cue where it is drawn.
pub fn deceptive_across(lines: &[Vec<char>], at: usize) -> Vec<Range<usize>> {
    let (mut joined, mut from) = (vec![], 0);
    for (i, l) in lines.iter().enumerate() {
        if i == at {
            from = joined.len();
        }
        joined.extend(l);
        joined.push('\n');
    }
    let to = from + lines[at].len();
    let mut out = vec![];
    for (text, target) in link_marks(&joined, &[]) {
        let (a, b) = if target.start >= from && target.end <= to {
            (target.start, target.end)
        } else {
            (text.start.max(from), text.end.min(to))
        };
        out.extend((a < b).then(|| a - from..b - from));
    }
    out
}

/// MD-10: a use of a reference: the text, the label it is resolved by, and where `[text]` is.
struct Use {
    text: String,
    label: String,
    at: Range<usize>,
}

/// MD-10: a label as CommonMark matches it: case folded, runs of blanks one.
fn norm(label: &str) -> String {
    label.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

/// MD-10: the references a line uses, in the three forms: `[text][label]`, `[text][]` and `[text]` (the
/// text is the label of the last two). Not an image's, a footnote's, an inline link's, the label part of
/// a full one, nor the label of a definition at the start of a line.
fn uses_in(cs: &[char]) -> Vec<Use> {
    let ps = pairs(cs);
    let mut out = vec![];
    for &(o, c) in &ps {
        let image = o > 0 && cs[o - 1] == '!';
        let label_part = o > 0 && cs[o - 1] == ']' && ps.iter().any(|p| p.1 == o - 1);
        let def = cs.get(c + 1) == Some(&':') && cs[..o].iter().all(|x| x.is_whitespace());
        if image || label_part || def || cs.get(o + 1) == Some(&'^') || cs.get(c + 1) == Some(&'(')
        {
            continue;
        }
        let text: String = cs[o + 1..c].iter().collect();
        let label =
            ps.iter().find(|p| p.0 == c + 1).map(|p| cs[p.0 + 1..p.1].iter().collect::<String>());
        let label = norm(&label.filter(|l| !l.trim().is_empty()).unwrap_or_else(|| text.clone()));
        out.push(Use { text, label, at: o..c + 1 });
    }
    out
}

/// MD-10: the definition a line is, if it is one: `[label]: destination`, with any blanks before it
/// (a definition in code is taken for one too: sim errs to the side of marking). Its label as it is
/// matched, and where its destination is in the line (none where it is on the next line).
fn def_in(cs: &[char]) -> Option<(String, Option<Range<usize>>)> {
    let i = cs.iter().position(|c| !c.is_whitespace()).filter(|&i| cs[i] == '[')?;
    let close = (i + 1..cs.len()).find(|&j| cs[j] == ']' && cs[j - 1] != '\\')?;
    let label: String = cs[i + 1..close].iter().collect();
    let plain = !label.starts_with('^') && !label.contains('[') && !label.trim().is_empty();
    if cs.get(close + 1) != Some(&':') || !plain {
        return None;
    }
    let k = (close + 2..cs.len()).find(|&j| !cs[j].is_whitespace());
    let dest = k.and_then(|k| token(cs, k));
    Some((norm(&label), dest.filter(|_| k.is_some())))
}

/// MD-10: the destination that starts at `k`: a `<…>` whole, or up to the next blank.
fn token(cs: &[char], k: usize) -> Option<Range<usize>> {
    if cs[k] == '<' {
        let e = cs[k..].iter().position(|&x| x == '>')?;
        return Some(k..k + e + 1);
    }
    let e = cs[k..].iter().position(|x| x.is_whitespace()).map_or(cs.len(), |e| k + e);
    Some(k..e)
}

/// MD-10: whether a line has something a reference link could make lie: a definition, or the use of a
/// reference whose text names an address. Only then is the document looked through for the rest.
pub fn ref_candidate(cs: &[char]) -> bool {
    cs.contains(&']')
        && (def_in(cs).is_some()
            || uses_in(cs).iter().any(|u| words(&u.text).any(|w| address(w).is_some())))
}

/// MD-10: the reference links of a document, resolved by their definitions (the first `REF_LINES` lines
/// are looked through, once for a version of the text): the targets of the definitions that a use makes
/// lie, where each is drawn, and what the uses of a line say.
pub struct Refs {
    defs: HashMap<String, Vec<String>>,
    on: HashMap<usize, Vec<Range<usize>>>,
    whole: bool,
}

impl Refs {
    /// The document of `n` lines, `line(l)` the text of line `l`. Every definition of a label counts (the
    /// first wins in CommonMark, and sim does not know which a reader's renderer takes).
    pub fn new(n: usize, line: &dyn Fn(usize) -> Option<String>) -> Refs {
        let mut defs: Vec<(usize, String, Range<usize>, String)> = vec![];
        let (mut kept, mut pending) = (vec![], None::<String>);
        for l in 0..n.min(REF_LINES) {
            let Some(s) = line(l) else { continue };
            if pending.is_none() && !s.contains('[') {
                continue;
            }
            let cs: Vec<char> = s.chars().collect();
            if let Some(label) = pending.take() {
                let at = cs.iter().position(|c| !c.is_whitespace()).and_then(|k| token(&cs, k));
                defs.extend(at.map(|r| (l, label, r.clone(), cs[r].iter().collect())));
            }
            match def_in(&cs) {
                Some((label, Some(r))) => defs.push((l, label, r.clone(), cs[r].iter().collect())),
                Some((label, None)) => pending = Some(label),
                None => {}
            }
            if cs.contains(&']') {
                kept.push((l, cs));
            }
        }
        let mut by: HashMap<String, Vec<String>> = HashMap::new();
        for (_, label, _, dest) in &defs {
            by.entry(label.clone()).or_default().push(dest.clone());
        }
        // the texts that say an address, by label, which some definition of the label is not
        let mut lying: HashMap<String, Vec<String>> = HashMap::new();
        for u in kept.iter().flat_map(|(_, cs)| uses_in(cs)) {
            if by.get(&u.label).is_some_and(|ds| ds.iter().any(|d| deceptive(&u.text, d))) {
                lying.entry(u.label).or_default().push(u.text);
            }
        }
        let mut on: HashMap<usize, Vec<Range<usize>>> = HashMap::new();
        for (l, label, r, dest) in defs {
            if lying.get(&label).is_some_and(|ts| ts.iter().any(|t| deceptive(t, &dest))) {
                on.entry(l).or_default().push(r);
            }
        }
        Refs { defs: by, on, whole: n <= REF_LINES }
    }

    /// The text of each use in the line `cs` that lies, with its brackets: its definition says another
    /// place; or no definition was found in a document that was not looked through whole, and the text
    /// says an address (what cannot be resolved is not trusted).
    pub fn uses(&self, cs: &[char]) -> Vec<Range<usize>> {
        let says = |t: &str| words(t).any(|w| address(w).is_some());
        uses_in(cs)
            .into_iter()
            .filter(|u| match self.defs.get(&u.label) {
                Some(ds) => ds.iter().any(|d| deceptive(&u.text, d)),
                None => !self.whole && says(&u.text),
            })
            .map(|u| u.at)
            .collect()
    }

    /// The targets of the definitions on line `l` that a use of the document makes lie.
    pub fn defs_on(&self, l: usize) -> Vec<Range<usize>> {
        self.on.get(&l).cloned().unwrap_or_default()
    }
}
