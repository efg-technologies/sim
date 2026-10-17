//! GREP-2, GREP-3, GREP-5, PICK-3
//! Searching the contents of files (pure): the literal smart-case query, the matching lines it
//! finds in a text, the rows of the panel made of them, and the reading of ripgrep's output. The
//! threads that run a search are in io.rs, the state of one search in sim/live.rs.
use crate::find::fold;
use crate::layout::{fit, short};
use si::core::view::width;
use std::cmp::Ordering;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;

/// GREP-5: a search stops after this many matching lines.
pub const MAX_LINES: usize = 1000;
/// GREP-2: a file with a NUL in this many first bytes is binary.
pub const HEAD: usize = 8 * 1024;
/// GREP-3: the most chars of a line that are kept, and how many of them come before the first match.
const CLIP: usize = 200;
const LEAD: usize = 40;
/// GREP-3: chars shown before a match that the width of the panel would cut off.
const BEFORE: usize = 4;

/// A matching line as the panel keeps it: its number (from 1), the column of the first match in
/// the line on disk, the text (trimmed, clipped) and the char ranges of the matches in that text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub no: usize,
    pub col: usize,
    pub text: String,
    pub marks: Vec<Range<usize>>,
}

/// The matching lines of one file, in file order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    pub path: String,
    pub lines: Vec<Line>,
}

/// GREP-5: what the shell is asked to search. `first` are the files of the list with a history
/// (likeliest first), which are searched before the rest of `files`. An empty `query` only stops
/// the older searches.
#[derive(Clone, Debug)]
pub struct Request {
    pub generation: u64,
    pub root: PathBuf,
    pub query: String,
    pub first: Vec<String>,
    pub files: Arc<Vec<String>>,
}

impl Request {
    pub fn stop(generation: u64) -> Request {
        Request {
            generation,
            root: PathBuf::new(),
            query: String::new(),
            first: vec![],
            files: Arc::default(),
        }
    }
}

/// What a search sends: whole groups, the generation they belong to, whether the search is over
/// and whether it stopped at the cap.
#[derive(Debug)]
pub struct Batch {
    pub generation: u64,
    pub groups: Vec<Group>,
    pub done: bool,
    pub capped: bool,
}

/// GREP-2: a literal, smart-case: one uppercase letter makes it exact, else chars are compared
/// lowercased one by one, as FIND-2's are.
pub struct Query {
    chars: Vec<char>,
    needle: String,
    exact: bool,
}

impl Query {
    pub fn new(query: &str) -> Option<Query> {
        let exact = query.chars().any(char::is_uppercase);
        let chars: Vec<char> = query.chars().map(|c| fold(c, exact)).collect();
        (!chars.is_empty()).then(|| Query { needle: chars.iter().collect(), chars, exact })
    }

    /// The char ranges of every match in `line`, left to right, not overlapping.
    pub fn ranges(&self, line: &str) -> Vec<Range<usize>> {
        let (cs, n) =
            (line.chars().map(|c| fold(c, self.exact)).collect::<Vec<_>>(), self.chars.len());
        let (mut out, mut i) = (vec![], 0);
        while i + n <= cs.len() {
            if cs[i..i + n] == self.chars[..] {
                out.push(i..i + n);
                i += n;
            } else {
                i += 1;
            }
        }
        out
    }

    /// GREP-3: line `no` as the panel keeps it: trimmed, a tab a space, and a line of more than
    /// `CLIP` chars cut to a window that begins `LEAD` chars before its first match (after a `…`).
    pub fn line(&self, no: usize, raw: &str) -> Line {
        let t = raw.trim();
        let marks = self.ranges(t);
        let first = marks.first().map_or(0, |m| m.start);
        let col = raw.chars().count() - raw.trim_start().chars().count() + first;
        let cs: Vec<char> = t.chars().map(|c| if c == '\t' { ' ' } else { c }).collect();
        let from = if cs.len() > CLIP { first.saturating_sub(LEAD) } else { 0 };
        let (end, dots) = ((from + CLIP).min(cs.len()), usize::from(from > 0));
        let text = "…".repeat(dots) + &cs[from..end].iter().collect::<String>();
        let in_window = marks.into_iter().filter(|m| m.start < end && m.end > from);
        let marks =
            in_window.map(|m| m.start.max(from) - from + dots..m.end.min(end) - from + dots);
        Line { no, col, text, marks: marks.collect() }
    }

    /// GREP-2: the matching lines of `text`, at most `limit` and one more (which says there are
    /// more). An ASCII query (or an exact one) is looked for in the whole text, and only the
    /// lines it is found in are read; a lowercase query with a letter beyond ASCII goes line by line.
    pub fn lines(&self, text: &str, limit: usize) -> Vec<Line> {
        if !self.exact && !self.needle.is_ascii() {
            let all = text.split('\n').enumerate().map(|(i, l)| self.line(i + 1, l));
            return all.filter(|l| !l.marks.is_empty()).take(limit + 1).collect();
        }
        let lowered;
        let hay = if self.exact {
            text
        } else {
            lowered = text.to_ascii_lowercase();
            &lowered
        };
        let (mut out, mut no, mut counted, mut next) = (vec![], 1, 0, 0);
        while out.len() <= limit
            && let Some(p) = hay[next..].find(&self.needle)
        {
            let at = next + p;
            let start = hay[..at].rfind('\n').map_or(0, |i| i + 1);
            let end = hay[at..].find('\n').map_or(hay.len(), |i| at + i);
            no += hay[counted..start].matches('\n').count();
            (counted, next) = (start, end);
            out.extend(Some(self.line(no, &text[start..end])).filter(|l| !l.marks.is_empty()));
        }
        out
    }
}

/// GREP-2: whether a file whose first bytes are these is binary (a NUL in the first 8 KiB).
pub fn is_binary(head: &[u8]) -> bool {
    head[..head.len().min(HEAD)].contains(&0)
}

/// GREP-5: the path, line number and text of a line of rg's output with `--null --line-number`
/// (`path NUL line:text`). The notice about a binary file has no NUL and is not one.
pub fn parse_rg(out: &[u8]) -> Option<(&[u8], usize, &[u8])> {
    let (path, rest) = out.split_at(out.iter().position(|&b| b == 0)?);
    let rest = &rest[1..];
    let colon = rest.iter().position(|&b| b == b':')?;
    let no = std::str::from_utf8(&rest[..colon]).ok()?.parse().ok()?;
    Some((path, no, &rest[colon + 1..]))
}

/// GREP-3: the order of files in the panel: the likelier first (MARK-2), then by path.
pub fn rank(a: (&str, f64), b: (&str, f64)) -> Ordering {
    b.1.total_cmp(&a.1).then_with(|| a.0.cmp(b.0))
}

/// A row of the picker's list: its text (at most as wide as the list), the columns that are bold
/// (the matches), the columns that are in the secondary gray (a path and a line number) and
/// whether it is the selected one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PRow {
    pub text: String,
    pub bold: Vec<Range<u16>>,
    pub dim: Vec<Range<u16>>,
    pub sel: bool,
}

/// The column each char starts in, and the end.
fn columns(cs: &[char]) -> Vec<usize> {
    let mut v = vec![0];
    (0..cs.len()).for_each(|i| v.push(v[i] + width(cs, i, v[i])));
    v
}

/// GREP-3: a matching line as a row of `max` columns: `{path}:{no}  {text}`, the path made to fit
/// in two fifths of the row. When the first match would be cut off by the width, the text starts
/// at a `…` and `BEFORE` chars before it.
pub fn row(path: &str, l: &Line, max: usize) -> PRow {
    let head = format!("{}:{}  ", short(path, (max * 2 / 5).max(1)), l.no);
    let head_cols = fit(&head, usize::MAX).1;
    let room = max.saturating_sub(head_cols);
    let mut cs: Vec<char> = fit(&l.text, usize::MAX).0.chars().collect();
    let (mut marks, cols) = (l.marks.clone(), columns(&cs));
    if let Some(m) = marks.first().filter(|m| cols[m.end.min(cs.len())] > room) {
        let skip = m.start.saturating_sub(BEFORE).min(cs.len());
        if skip > 0 {
            cs = std::iter::once('…').chain(cs[skip..].iter().copied()).collect();
            let kept = marks.iter().filter(|r| r.end > skip);
            marks = kept.map(|r| r.start.max(skip) - skip + 1..r.end - skip + 1).collect();
        }
    }
    let cols = columns(&cs);
    let kept = cols.iter().rposition(|&c| c <= room).unwrap_or(0);
    let at = |i: usize| (head_cols + cols[i.min(kept)]).min(max) as u16;
    let bold = marks.iter().map(|r| at(r.start)..at(r.end)).filter(|r| r.start < r.end);
    PRow {
        text: fit(&(head + &cs[..kept].iter().collect::<String>()), max).0,
        bold: bold.collect(),
        dim: std::iter::once(0..(head_cols - 2).min(max) as u16).collect(),
        sel: false,
    }
}
