//! PROP-1, PROP-2, PROP-6, PROP-8, PROP-9, SAFE-5, SAFE-6
//! What the buffer does when the file changes under it: reload or merge, mark the changes, and
//! keep the conflicts as hunks. The same for what a filter prints.
use super::app::{App, plural};
use super::buffer::{Buffer, Hunk, Mark, Words, hash};
use super::proposal::{Merged, clip, lines, merge3, words};
use std::ops::Range;
use std::sync::Arc;
use std::time::Instant;

/// Bytes of text that word diffs look at in one merge. Past it whole lines count as changed, so
/// that a rewrite of a huge file costs the line diff and nothing more (PROP-7).
const WORDS_BUDGET: usize = 256 << 10;

/// Whether the new conflict `n` is the disk's newer say on all of the old one `o`'s lines (or on
/// the point between lines that an empty `o` is): then `o` is out of date. One that overlaps only
/// in part leaves both, so that no text of the disk is dropped (P1).
#[rustfmt::skip]
fn covers(n: &Range<usize>, o: &Range<usize>) -> bool {
    if o.is_empty() { n == o || (n.start < o.start && o.start < n.end) } else { n.start <= o.start && o.end <= n.end }
}

/// Word diffs that stop being made once `WORDS_BUDGET` bytes of text have been looked at.
fn budgeted(by: Instant) -> impl FnMut(&str, &str) -> Option<Words> {
    let mut budget = WORDS_BUDGET;
    move |a, b| {
        let n = a.len() + b.len();
        (n <= budget).then(|| {
            budget -= n;
            words(a, b, Some(by))
        })
    }
}

/// The hunks of a merge, with their word diffs. `disk`: conflicts with the disk, else a filter's.
fn hunks(m: &Merged, diff: &mut impl FnMut(&str, &str) -> Option<Words>, disk: bool) -> Vec<Hunk> {
    let ls = lines(&m.text);
    let hunk = |h: &super::proposal::Hunk| {
        let words = diff(&ls[h.mine.clone()].concat(), &h.theirs).map(Arc::new);
        Hunk { mine: h.mine.clone(), theirs: h.theirs.as_str().into(), disk, words }
    };
    m.hunks.iter().map(hunk).collect()
}

impl App {
    /// PROP-6: the filter ended. After a clean exit what it printed, in place of the lines it was
    /// given, is `theirs` of a merge with the buffer as it is now, and every change in it becomes a
    /// proposal; the text stays as it is, edits made meanwhile included. Otherwise it is discarded.
    pub(super) fn filter_done(&mut self, status: Option<i32>, out: &[u8], err: &[u8], by: Instant) {
        let Some(run) = self.filter.take() else { return };
        let why = String::from_utf8_lossy(err);
        let why = why.lines().map(str::trim).find(|l| !l.is_empty());
        let out = std::str::from_utf8(out).ok().filter(|s| !s.contains('\0'));
        if run.killed {
            return self.say("filter cancelled");
        } else if status != Some(0) {
            let how = status.map_or("killed".into(), |n| format!("exit {n}"));
            return self.say(why.map_or(format!("filter failed ({how})"), String::from));
        }
        let Some(out) = out else { return self.say("filter output is not text: discarded") };
        let [base, mine] = [run.text.to_string(), self.buf.text.to_string()].map(|t| t + "\n");
        let ls = lines(&base);
        let nl = if out.is_empty() || out.ends_with('\n') { "" } else { "\n" };
        let theirs =
            [&ls[..run.lines.start].concat(), out, nl, &ls[run.lines.end..].concat()].concat();
        let m = merge3(&base, &mine, &theirs, true, Some(by));
        let new = hunks(&m, &mut budgeted(by), false);
        let n = new.len();
        if n == 0 {
            return self.say("no changes from the filter");
        }
        self.buf.propose(new);
        self.say(format!("{} from the filter · ]c to review", plural(n, "proposal", "proposals")));
    }

    /// PROP-1: the watcher read the file (`None`: it is gone). Our own saves are told by their hash.
    pub(super) fn disk(&mut self, bytes: Option<Vec<u8>>, known: Option<u64>, by: Instant) {
        let h = bytes.as_deref().map(|b| known.unwrap_or_else(|| hash(b)));
        let ours = self.saving.as_ref().is_some_and(|p| Some(p.hash) == h);
        if self.path.is_none() || h == self.disk || ours {
            return;
        }
        match (bytes, h) {
            (Some(b), Some(h)) => drop(self.take_disk(&b, h, by)),
            _ => self.vanished(),
        }
    }

    /// The file was deleted: the text stays and counts as unsaved, and `:w` makes the file again.
    pub(super) fn vanished(&mut self) {
        (self.disk, self.gone) = (None, true);
        self.buf.saved = u64::MAX;
    }

    /// PROP-2: the file now holds `bytes`, which hash to `h`. A clean buffer reloads; a dirty one
    /// merges. Returns the number of conflicts, or `None` (after saying why) when the content
    /// cannot be merged.
    pub(super) fn take_disk(&mut self, bytes: &[u8], h: u64, by: Instant) -> Option<usize> {
        if Some(h) == self.disk {
            // Taken in already (a save's answer after the watcher's read): no second undo step.
            return Some(self.buf.hunks.iter().filter(|h| h.disk).count());
        }
        let disk = Buffer::decode(bytes);
        if self.buf.readonly.is_some() || disk.readonly.is_some() {
            self.say("changed on disk, not as text: not merged (:e! reloads, :w! overwrites)");
            return None;
        }
        let base = self.buf.base.to_string() + "\n";
        let mine = if self.buf.dirty() { self.buf.text.to_string() + "\n" } else { base.clone() };
        (self.disk, self.gone) = (Some(h), false);
        Some(self.merge_in(&base, &mine, disk, by))
    }

    /// Merges the file `disk` into the buffer as one undo step (PROP-2): `mine` is what the user has
    /// made of `base`. Changes of the file are taken in and marked; conflicts become hunks that keep
    /// mine in the buffer. Every text here ends with a line break, so that a line added at the end
    /// is an added line and not a change of the last one. Returns the number of new hunks.
    pub(super) fn merge_in(&mut self, base: &str, mine: &str, disk: Buffer, by: Instant) -> usize {
        self.leave_visual(); // its anchor is a place in the old text
        let theirs = disk.text.to_string() + "\n";
        let mut m = merge3(base, mine, &theirs, false, Some(by));
        m.text = if m.text.is_empty() { "\n".into() } else { m.text }; // a buffer has a line
        let ls = lines(&m.text);
        let (mut marks, mut regs, mut delta) = (Vec::<Mark>::new(), vec![], 0isize);
        // `regs` are the places where a change of the file replaced lines of mine: where mine had
        // them, how many it had, how many replace them, and the change in line count so far.
        let mut diff = budgeted(by);
        for mk in &m.marks {
            let (n, k) = (mk.lines.len(), mk.old.split_inclusive('\n').count());
            let start = (mk.lines.start as isize - delta) as usize;
            delta += n as isize - k as isize;
            regs.push((start, k, n, delta));
            let kind = if mk.old.is_empty() { '+' } else { '~' };
            let new = ls[mk.lines.clone()].concat();
            let diff = if kind == '~' && n > 0 { diff(&mk.old, &new).map(|w| w.1) } else { None };
            if n == 0 {
                let line = mk.lines.start.min(ls.len().saturating_sub(1));
                marks.push(Mark { line, kind: '-', words: Some([].into()) });
            }
            let mut at = 0;
            for (i, l) in ls[mk.lines.clone()].iter().enumerate() {
                let len = l.trim_end_matches('\n').chars().count();
                let words =
                    diff.as_ref().map(|d| d.iter().filter_map(|r| clip(r, at, len)).collect());
                marks.push(Mark { line: mk.lines.start + i, kind, words });
                at += l.chars().count();
            }
        }
        // Where a line of mine went (`end`: where the range that ends with it ends), and whether a
        // change of the file replaced it.
        let first = |l: usize| regs.partition_point(|r| r.0 + r.1 <= l);
        let inside = |l: usize| regs.get(first(l)).filter(|r| r.0 <= l);
        let go = |l: usize, end: bool| {
            let d = first(l).checked_sub(1).map_or(0, |i| regs[i].3);
            let (start, n) = inside(l).map_or((l, 1), |r| (r.0, r.2));
            (start as isize + d) as usize + if end { n } else { 0 }
        };
        let b = &self.buf;
        let (line, col) = (go(b.line_of(b.cursor), false), b.col());
        // `'<` and `'>` are lines too: they follow their text like the cursor does.
        let sel = (go(self.marks.0, false), go(self.marks.1, false));
        let top = go(self.top, false); // the view follows the text like the cursor
        let old_marks = b.marks.iter().filter(|k| inside(k.line).is_none());
        let mut all: Vec<Mark> =
            old_marks.map(|k| Mark { line: go(k.line, false), ..k.clone() }).collect();
        all.extend(marks);
        all.sort_by_key(|k| (k.line, k.kind == '-'));
        all.dedup_by_key(|k| k.line);
        let new = hunks(&m, &mut diff, true);
        let covered = |o: &Hunk| {
            let i = new.partition_point(|n| n.mine.start <= o.mine.start);
            o.disk && new[i.saturating_sub(2)..i].iter().any(|n| covers(&n.mine, &o.mine))
        };
        let old_hunks = b.hunks.iter().map(|h| {
            let start = go(h.mine.start, false);
            let end = if h.mine.is_empty() { start } else { go(h.mine.end - 1, true) };
            // A change that lands inside the hunk makes its word diff stale, as an edit does.
            let hit = regs.get(first(h.mine.start)).is_some_and(|r| r.0 < h.mine.end);
            let words = h.words.clone().filter(|_| !hit);
            Hunk { mine: start..end, theirs: h.theirs.clone(), disk: h.disk, words }
        });
        let mut kept: Vec<Hunk> = old_hunks.filter(|h| !covered(h)).collect();
        kept.extend(new);
        kept.sort_by_key(|h| h.mine.start);
        let clean = m.text == theirs;
        self.buf.merged(m.text.strip_suffix('\n').unwrap_or(&m.text), disk, clean);
        let b = &mut self.buf;
        (b.marks, b.hunks) = (Arc::new(all), Arc::new(kept));
        let (last, line) = (b.lines() - 1, line.min(b.lines() - 1));
        (b.cursor, b.want) = (b.start(line) + col.min(b.line_len(line)), None);
        b.checkpoint();
        (self.marks, self.top) = ((sel.0.min(last), sel.1.min(last)), top.min(last));
        self.clamp();
        m.hunks.len()
    }
}
