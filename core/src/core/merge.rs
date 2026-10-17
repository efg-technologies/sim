//! DISK-3, DISK-4, DISK-5, VIS-2
//! diff3 and the word-level diff.

use similar::{Algorithm, DiffOp, capture_diff_slices_deadline};
use std::{ops::Range, time::Instant};
use unicode_width::UnicodeWidthChar;

/// The lines of a text, each keeping its '\n' (the last one may lack it). "" has no lines.
pub fn lines(s: &str) -> Vec<&str> {
    s.split_inclusive('\n').collect()
}

/// A change taken in from theirs without conflict, to be marked: lines `lines` of the merged text
/// (empty = where something was deleted) replaced `old` (empty = pure addition).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mark {
    pub lines: Range<usize>,
    pub old: String,
}

/// A merged text, the changes of theirs that were taken in (to be marked), and the conflicts: for
/// each, the lines of the merged text that are mine where theirs differs (maybe none: mine has no
/// line there, and the range is the point between two lines).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Merged {
    pub text: String,
    pub marks: Vec<Mark>,
    pub conflicts: Vec<Range<usize>>,
}

/// A change of one side: (base lines, that side's lines) that replace each other.
type Change = (Range<usize>, Range<usize>);

/// The changes from `base` to `side`, in order, with an unchanged line between any two.
fn changes(base: &[&str], side: &[&str], deadline: Option<Instant>) -> Vec<Change> {
    let mut out: Vec<Change> = Vec::new();
    if base == side {
        return out;
    }
    for op in capture_diff_slices_deadline(Algorithm::Myers, base, side, deadline) {
        if matches!(op, DiffOp::Equal { .. }) {
            continue;
        }
        let (old, new) = (op.old_range(), op.new_range());
        match out.last_mut() {
            Some(last) if last.0.end == old.start => (last.0.end, last.1.end) = (old.end, new.end),
            _ => out.push((old, new)),
        }
    }
    out
}

/// Whether a change over base lines `r` joins the cluster over `start..end`, given that changes
/// come in order of start. Sharing a base line overlaps; so does an insertion strictly inside the
/// cluster, or at the same point as another insertion. Merely touching does not, unless the cluster
/// ends in a last line without a newline (`open`): nothing may be put after that, or two lines
/// would run together.
fn joins(start: usize, end: usize, open: bool, r: &Range<usize>) -> bool {
    open || if r.is_empty() {
        (start < r.start && r.start < end) || (start == end && r.start == start)
    } else {
        r.start < end
    }
}

/// Three-way merge of whole texts by lines (DISK-3): two line diffs from `similar`
/// (base→mine, base→theirs), walked together. Theirs changes that don't overlap mine's are applied
/// and marked (DISK-2), and only conflicts are left out. `deadline` goes to similar (DISK-5).
///
/// The changes of both sides are grouped into clusters: changes that overlap, chained. A cluster
/// that only mine touches is kept; one that both made identically is taken once; one that only
/// theirs touches is applied and marked; otherwise it is a conflict, and mine's version of the
/// whole cluster stays.
pub fn merge3(base: &str, mine: &str, theirs: &str, deadline: Option<Instant>) -> Merged {
    let (b, sides) = (lines(base), [lines(mine), lines(theirs)]);
    let mut all: Vec<(Range<usize>, Range<usize>, usize)> = Vec::new();
    for (side, ls) in sides.iter().enumerate() {
        all.extend(changes(&b, ls, deadline).into_iter().map(|(old, new)| (old, new, side)));
    }
    all.sort_by_key(|c| (c.0.start, c.0.end));
    let mut out = Merged { text: String::new(), marks: Vec::new(), conflicts: Vec::new() };
    let (mut text, mut done, mut off, mut i) = (Vec::new(), 0, [0isize; 2], 0);
    while i < all.len() {
        let (start, mut end, mut open) = (all[i].0.start, all[i].0.start, false);
        let (mut delta, mut count) = ([0isize; 2], [0usize; 2]);
        while i < all.len() && (count == [0, 0] || joins(start, end, open, &all[i].0)) {
            let (old, new, side) = &all[i];
            end = end.max(old.end);
            open |= sides[*side][new.clone()].last().is_some_and(|l| !l.ends_with('\n'));
            delta[*side] += new.len() as isize - old.len() as isize;
            count[*side] += 1;
            i += 1;
        }
        // Each side's lines for base lines start..end: the base is shifted by what the side changed before.
        let region = |s: usize| {
            let first = (start as isize + off[s]) as usize;
            first..(first as isize + (end - start) as isize + delta[s]) as usize
        };
        let (mine_r, theirs_r) = (region(0), region(1));
        (off[0], off[1]) = (off[0] + delta[0], off[1] + delta[1]);
        text.extend_from_slice(&b[done..start]);
        done = end;
        let (mine_l, theirs_l) = (&sides[0][mine_r], &sides[1][theirs_r]);
        let first = text.len();
        if count[1] == 0 || mine_l == theirs_l {
            text.extend_from_slice(mine_l);
        } else if count[0] == 0 {
            text.extend_from_slice(theirs_l);
            out.marks.push(Mark { lines: first..text.len(), old: b[start..end].concat() });
        } else {
            text.extend_from_slice(mine_l);
            out.conflicts.push(first..text.len());
        }
    }
    text.extend_from_slice(&b[done..]);
    out.text = text.concat();
    out
}

/// The part of the char range `r` that lies in the `len` chars starting at `at`, counted from `at`.
pub fn clip(r: &Range<usize>, at: usize, len: usize) -> Option<Range<usize>> {
    let (s, e) = (r.start.max(at), r.end.min(at + len));
    (s < e).then(|| s - at..e - at)
}

/// 1 and 2 are runs (word characters, whitespace); 0 is a token on its own.
fn kind(c: char) -> u8 {
    match c {
        _ if c.width() == Some(2) => 0,
        _ if c.is_alphanumeric() || c == '_' => 1,
        _ if c.is_whitespace() => 2,
        _ => 0,
    }
}

/// The tokens of `s`, and the char index where each starts (and finally the char count).
fn tokens(s: &str) -> (Vec<&str>, Vec<usize>) {
    let (mut toks, mut at, mut rest, mut n) = (Vec::new(), vec![0], s, 0);
    while let Some(c) = rest.chars().next() {
        let k = kind(c);
        let len =
            if k == 0 { c.len_utf8() } else { rest.find(|d| kind(d) != k).unwrap_or(rest.len()) };
        let (tok, tail) = rest.split_at(len);
        (rest, n) = (tail, n + tok.chars().count());
        toks.push(tok);
        at.push(n);
    }
    (toks, at)
}

/// Word-level differences (VIS-2): the changed char ranges in `old` and in `new`. Tokens: a run of
/// alphanumerics and `_` is one word; a char of display width 2 (CJK, full-width) is one word on
/// its own; a run of whitespace is one token; any other char is one token. At the `deadline`
/// (DISK-5) the diff gives up and what is left unmatched counts as changed: coarse, never wrong.
pub fn words(
    old: &str,
    new: &str,
    deadline: Option<Instant>,
) -> (Vec<Range<usize>>, Vec<Range<usize>>) {
    let ((old_t, old_at), (new_t, new_at)) = (tokens(old), tokens(new));
    let (mut old_r, mut new_r) = (Vec::new(), Vec::new());
    for op in capture_diff_slices_deadline(Algorithm::Myers, &old_t, &new_t, deadline) {
        let (o, n) = (op.old_range(), op.new_range());
        if matches!(op, DiffOp::Equal { .. }) {
            continue;
        }
        if !o.is_empty() {
            old_r.push(old_at[o.start]..old_at[o.end]);
        }
        if !n.is_empty() {
            new_r.push(new_at[n.start]..new_at[n.end]);
        }
    }
    (old_r, new_r)
}
