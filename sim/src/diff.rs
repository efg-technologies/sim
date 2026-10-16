//! UI-5
//! What `git diff -U0 HEAD -- file` says, as the signs of the sign column and the counts of the
//! status line (pure): which lines were added, which changed, and where lines were deleted, as
//! gitsigns reads a hunk.
use std::collections::BTreeMap;

/// The sign of a line: added, changed, lines deleted just below it, or deleted above the first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sign {
    Add,
    Change,
    Del,
    DelTop,
}

/// The signs of a file (by 0-based line) and the lines it has gained, changed and lost against
/// HEAD, as lualine's diff counts them. `lines` is the length of the file the diff was made for.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Diff {
    pub signs: BTreeMap<usize, Sign>,
    pub added: usize,
    pub changed: usize,
    pub deleted: usize,
    pub lines: usize,
    /// Every line is added: a file git does not know yet (no map of 100,000 signs for it).
    pub all: bool,
}

/// The numbers of `-a,b +c,d` of a hunk header: a count that is left out is 1.
fn header(line: &str) -> Option<((usize, usize), (usize, usize))> {
    let rest = line.strip_prefix("@@ -")?;
    let (old, rest) = rest.split_once(" +")?;
    let new = rest.split_once(" @@").map_or(rest, |x| x.0);
    let range = |s: &str| {
        let (a, b) = s.split_once(',').unwrap_or((s, "1"));
        Some((a.parse().ok()?, b.parse().ok()?))
    };
    Some((range(old)?, range(new)?))
}

impl Diff {
    /// Every line of a file of `lines` lines added (a file git does not know yet).
    pub fn all_added(lines: usize) -> Diff {
        Diff { added: lines, lines, all: true, ..Diff::default() }
    }

    /// The signs of `git diff -U0` output for a file that now has `lines` lines. A hunk
    /// `@@ -a,b +c,d @@` is: `b = 0`, the lines `c..c + d` added; `d = 0`, lines deleted below
    /// line `c` (above the first one when `c` is 0); else the first `min(b, d)` lines changed, the
    /// rest added when `d > b`, and lines deleted below the last when `b > d`.
    pub fn parse(out: &[u8], lines: usize) -> Diff {
        let mut diff = Diff { lines, ..Diff::default() };
        let text = String::from_utf8_lossy(out);
        for ((_, b), (c, d)) in text.lines().filter_map(header) {
            let (first, mut put) = (c.saturating_sub(1), |l: usize, s: Sign| {
                if l < lines.max(1) {
                    diff.signs.entry(l).and_modify(|x| *x = (*x).max_by_rank(s)).or_insert(s);
                }
            });
            match (b, d) {
                (0, _) => (first..first + d).for_each(|l| put(l, Sign::Add)),
                (_, 0) if c == 0 => put(0, Sign::DelTop),
                (_, 0) => put(first, Sign::Del),
                _ => {
                    let m = b.min(d);
                    (first..first + m).for_each(|l| put(l, Sign::Change));
                    (first + m..first + d).for_each(|l| put(l, Sign::Add));
                    if b > d {
                        put(first + d - 1, Sign::Del);
                    }
                }
            }
            let m = if b == 0 || d == 0 { 0 } else { b.min(d) };
            diff.changed += m;
            diff.added += if b == 0 { d } else { d.saturating_sub(b) };
            diff.deleted += if d == 0 { b } else { b.saturating_sub(d) };
        }
        diff
    }

    /// The sign of line `l` (0-based) of the file as it is now: `edit` is the line the cursor is
    /// on and `now` the number of lines the file has, so that a line below an edit that added or
    /// removed lines is looked up where it was (the diff is of the saved file).
    pub fn sign(&self, l: usize, edit: usize, now: usize) -> Option<Sign> {
        if self.all {
            return Some(Sign::Add);
        }
        let old =
            if l > edit { l.checked_add_signed(self.lines as isize - now as isize)? } else { l };
        self.signs.get(&old).copied()
    }
}

impl Sign {
    /// A line that is both changed and has a deletion below it shows the change.
    fn max_by_rank(self, o: Sign) -> Sign {
        let rank = |s: Sign| match s {
            Sign::Add => 3,
            Sign::Change => 2,
            Sign::Del => 1,
            Sign::DelTop => 0,
        };
        if rank(o) > rank(self) { o } else { self }
    }
}
