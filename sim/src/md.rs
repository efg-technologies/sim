//! MD-1, MD-2, MD-3, MD-4, MD-7, MD-8, MD-11
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
    /// A char of a callout's marker, `[!NOTE]`, which the kind's word replaces (MD-11).
    Callout,
}

/// MD-2: whether `c` may be left out for `why`. The one place that says so: a char shown in
/// another form (SAFE-8), or any char of text, is none of these.
pub fn may_hide(c: char, why: Why) -> bool {
    match why {
        Why::Markup => matches!(c, '*' | '_' | '`'),
        Why::Blank => c == ' ',
        Why::Rule => matches!(c, '-' | ':' | ' '),
        Why::Callout => c.is_ascii_alphabetic() || matches!(c, '[' | '!' | ']'),
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
    /// MD-11: the callouts of the screen (`callouts`), and whether their kind has an icon (`zi`).
    pub calls: Vec<Block>,
    pub icons: bool,
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

/// MD-11: the kinds of a callout, GitHub's alerts, in the order GitHub lists them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Callout {
    Note,
    Tip,
    Important,
    Warning,
    Caution,
}

impl Callout {
    pub const ALL: [Callout; 5] =
        [Callout::Note, Callout::Tip, Callout::Important, Callout::Warning, Callout::Caution];

    /// The name as GitHub draws it.
    pub fn word(self) -> &'static str {
        ["Note", "Tip", "Important", "Warning", "Caution"][self as usize]
    }

    /// GitHub's own octicon for it, in a Nerd Font: info, light-bulb, report, alert and stop.
    pub fn icon(self) -> char {
        ['\u{f449}', '\u{f400}', '\u{f50a}', '\u{f421}', '\u{f46e}'][self as usize]
    }
}

/// MD-11: a callout on the screen: the lines of its quote, the kind, and the chars of the marker in
/// the first of them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    pub lines: Range<usize>,
    pub kind: Callout,
    pub marker: Range<usize>,
}

/// MD-11: `> [!NOTE]`, as GitHub's renderer reads it (measured with its API, SPEC): a bar in the first
/// column, up to four blanks, `[!`, one of the five names in any case, `]` and then nothing but blanks.
fn marker(cs: &[char]) -> Option<(Callout, Range<usize>)> {
    let skip = cs.get(1..)?.iter().take_while(|&&c| matches!(c, ' ' | '\t')).count();
    let at = 1 + skip;
    if cs.first() != Some(&'>') || skip > 4 || cs.get(at..at + 2)? != ['[', '!'] {
        return None;
    }
    let close = at + cs[at..].iter().position(|&c| c == ']')?;
    let name: String = cs[at + 2..close].iter().collect::<String>().to_ascii_lowercase();
    let kind = *Callout::ALL.iter().find(|k| k.word().to_ascii_lowercase() == name)?;
    cs[close + 1..].iter().all(|c| matches!(c, ' ' | '\t' | '\r')).then_some((kind, at..close + 1))
}

/// Whether line `l` is a line of a quote: a bar in the first column.
fn quoted(text: Text, l: usize) -> bool {
    text(l).is_some_and(|s| s.starts_with('>'))
}

/// MD-11: the callouts among `lines` (the lines on the screen), made as GitHub's renderer makes its
/// alerts: the first line of a quote (the quote runs while lines start with a bar) is a marker, the
/// highlighter calls its bar a quote's (so a line of a fence is none), and a later line has text.
pub fn callouts<'a>(
    lines: Range<usize>,
    text: Text,
    spans: &dyn Fn(usize) -> Option<&'a Spans>,
) -> Vec<Block> {
    let (mut out, mut l) = (vec![], lines.start);
    while l < lines.end {
        if !quoted(text, l) {
            l += 1;
            continue;
        }
        let mut s = l;
        while s > 0 && l - s < MAX_ROWS && quoted(text, s - 1) {
            s -= 1;
        }
        let mut e = l + 1;
        while e - l < MAX_ROWS && quoted(text, e) {
            e += 1;
        }
        let first: Vec<char> = text(s).unwrap_or_default().chars().collect();
        let bar =
            spans(s).is_some_and(|x| x.iter().any(|(r, k)| *k == Look::Quote && r.start == 0));
        let body = (s + 1..e).any(|k| text(k).is_some_and(|t| !t[1..].trim().is_empty()));
        if let Some((kind, marker)) = marker(&first).filter(|_| bar && body) {
            out.push(Block { lines: s..e, kind, marker });
        }
        l = e;
    }
    out
}

/// MD-11: the ops of a callout's marker line: each char of the marker is left out, and as many of
/// the chars of the icon (with `icons`), a blank and the kind's word are put before them, one for each.
/// The word is never longer than the marker, whose brackets and bang it takes the room of.
fn word(ops: &mut [Op], cs: &[char], b: &Block, icons: bool) {
    let icon = icons.then_some([b.kind.icon(), ' ']).into_iter().flatten();
    let put: Vec<char> = icon.chain(b.kind.word().chars()).collect();
    for (n, i) in b.marker.clone().enumerate() {
        let Some(&c) = cs.get(i) else { continue };
        ops[i].hide = may_hide(c, Why::Callout);
        ops[i].fill = put.get(n).map(|&c| (c, 1));
    }
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
        Read { raw, front, tables, need, calls: vec![], icons: false }
    }

    /// How line `l` (`cs`, with the highlighter's `sp`) is drawn, if it is drawn otherwise than raw.
    pub fn plan(&self, l: usize, cs: &[char], sp: &Spans) -> Option<Plan> {
        if self.raw.contains(&l) || l < self.front || sp.first().is_some_and(|s| s.1 == Look::Band)
        {
            return None;
        }
        let mut ops = vec![Op::default(); cs.len() + 1];
        marks(cs, sp, &mut ops);
        if let Some(b) = self.calls.iter().find(|b| b.lines.start == l) {
            word(&mut ops, cs, b, self.icons);
        }
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

    /// The new columns of what is put in other than blanks, and the char it is made of: the rule of a
    /// delimiter row (`─`), or a callout's word (MD-11).
    pub fn puts(&self) -> Vec<(Range<u16>, char)> {
        let (mut at, mut out) = (0, vec![]);
        for p in &self.pieces {
            match p {
                Piece::Keep { cols, .. } => at += cols.end - cols.start,
                Piece::Fill(c, n) => {
                    out.extend((*c != ' ').then(|| (at..at + n, *c)));
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
