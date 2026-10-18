//! DISK-1, DISK-2, DISK-6, FILT-2, SAFE-5, SAFE-6, SAFE-7
//! What the buffer does when the file changes under it: reload or merge, mark the changes, and keep
//! mine where it conflicts. The same merge puts what a filter printed in place of its lines.
use super::app::{App, plural};
use super::buffer::{Buffer, Mark};
use super::merge::{clip, lines, merge3, words};
use std::sync::Arc;
use std::time::Instant;

/// Bytes of text that word diffs look at in one merge. Past it whole lines count as changed, so
/// that a rewrite of a huge file costs the line diff and nothing more (DISK-5).
const WORDS_BUDGET: usize = 256 << 10;

/// The changed char ranges of the new text, for words that are looked at in a budget of
/// `WORDS_BUDGET` bytes of text.
fn budgeted(by: Instant) -> impl FnMut(&str, &str) -> Option<Vec<std::ops::Range<usize>>> {
    let mut budget = WORDS_BUDGET;
    move |a, b| {
        let n = a.len() + b.len();
        (n <= budget).then(|| {
            budget -= n;
            words(a, b, Some(by)).1
        })
    }
}

impl App {
    /// FILT-2: the filter ended. After a clean exit what it printed takes the place of the lines it
    /// was given, as one undo step; the edits made meanwhile are kept, and where they overlap the
    /// lines of the range nothing is applied. Otherwise the output is discarded.
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
        let m = merge3(&base, &mine, &theirs, Some(by));
        if !m.conflicts.is_empty() {
            return self.say("filter result discarded: the lines changed meanwhile (run it again)");
        } else if m.text == mine {
            return self.say("no changes from the filter");
        }
        // Only the lines that differ are replaced, so the marks elsewhere stay where they are.
        let [a, b] = [&mine, &m.text].map(|s| s.strip_suffix('\n').unwrap_or(s));
        let (an, bn) = (a.chars().count(), b.chars().count());
        let head = a.chars().zip(b.chars()).take_while(|(x, y)| x == y).count();
        let tail = a.chars().rev().zip(b.chars().rev()).take(an.min(bn) - head);
        let tail = tail.take_while(|(x, y)| x == y).count();
        let with: String = b.chars().skip(head).take(bn - head - tail).collect();
        let (cut, wn) = (head..an - tail, with.chars().count());
        self.buf.checkpoint();
        self.buf.replace(cut.clone(), &with);
        // The cursor and the Visual anchor keep their text; inside the replaced lines they go to
        // where those start.
        let at = |i: usize| match i {
            i if i < cut.start => i,
            i if i < cut.end => cut.start,
            i => i - cut.end + cut.start + wn,
        };
        (self.buf.cursor, self.anchor) = (at(self.buf.cursor), at(self.anchor));
        self.clamp();
        let n = with.matches('\n').count() + 1;
        self.say(format!("{} from the filter · u to undo", plural(n, "line", "lines")));
    }

    /// DISK-1: the watcher read the file (`None`: it is gone). Our own saves are told by their hash.
    pub(super) fn disk(&mut self, bytes: Option<Vec<u8>>, known: Option<u64>, by: Instant) {
        let h = bytes.as_deref().map(|b| known.unwrap_or_else(|| super::buffer::hash(b)));
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

    /// DISK-2: the file now holds `bytes`, which hash to `h`. A clean buffer reloads; a dirty one
    /// merges. False (after saying why) when the content cannot be merged.
    pub(super) fn take_disk(&mut self, bytes: &[u8], h: u64, by: Instant) -> bool {
        if Some(h) == self.disk {
            return true; // taken in already (a save's answer after the watcher's read): no second undo step
        }
        let disk = Buffer::decode(bytes);
        if self.buf.readonly.is_some() || disk.readonly.is_some() {
            if self.buf.readonly.is_none() {
                self.theirs = Some(bytes.to_vec()); // not text, and the buffer is: SAFE-7 keeps it
            }
            self.say("changed on disk, not as text: not merged (:e! reloads, :w! overwrites)");
            return false;
        }
        let base = self.buf.base.to_string() + "\n";
        let mine = if self.buf.dirty() { self.buf.text.to_string() + "\n" } else { base.clone() };
        (self.disk, self.gone) = (Some(h), false);
        self.merge_in(&base, &mine, disk, by);
        true
    }

    /// Merges the file `disk` into the buffer as one undo step (DISK-2): `mine` is what the user has
    /// made of `base`. Changes of the file are taken in and marked; where both changed the same
    /// lines mine stays, marked, and the buffer is diverged (SAFE-7). Every text here ends with a
    /// line break, so that a line added at the end is an added line and not a change of the last one.
    pub(super) fn merge_in(&mut self, base: &str, mine: &str, disk: Buffer, by: Instant) {
        self.leave_visual(); // its anchor is a place in the old text
        let theirs = disk.text.to_string() + "\n";
        let mut m = merge3(base, mine, &theirs, Some(by));
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
            let diff = if kind == '~' && n > 0 { diff(&mk.old, &new) } else { None };
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
        // A conflict is marked on the lines of mine it overlaps, whole (the disk's words are not
        // here), or by a deletion point where mine has no line.
        for r in &m.conflicts {
            let last = ls.len().saturating_sub(1);
            let (kind, words) = if r.is_empty() { ('-', Some([].into())) } else { ('~', None) };
            marks.extend((r.start..r.end.max(r.start + 1)).map(|line| Mark {
                line: line.min(last),
                kind,
                words: words.clone(),
            }));
        }
        self.wrote = marks.iter().filter(|m| m.kind != '-').map(|m| m.line).collect();
        // Where a line of mine went (`end`: where the range that ends with it ends), and whether a
        // change of the file replaced it.
        let first = |l: usize| regs.partition_point(|r| r.0 + r.1 <= l);
        let inside = |l: usize| regs.get(first(l)).filter(|r| r.0 <= l);
        let go = |l: usize| {
            let d = first(l).checked_sub(1).map_or(0, |i| regs[i].3);
            let (start, _) = inside(l).map_or((l, 1), |r| (r.0, r.2));
            (start as isize + d) as usize
        };
        let b = &self.buf;
        let (line, col) = (go(b.line_of(b.cursor)), b.col());
        // `'<` and `'>` are lines too: they follow their text like the cursor does.
        let sel = (go(self.marks.0), go(self.marks.1));
        let top = go(self.top); // the view follows the text like the cursor
        // The marks of VI-17 follow their text too; one on a line the file's change took away is gone,
        // and one on a line it replaced goes to where the replacement begins.
        let vmark = b
            .vmark
            .map(|m| m.and_then(|(l, c)| inside(l).is_none_or(|r| r.2 > 0).then_some((go(l), c))));
        let old_marks = b.marks.iter().filter(|k| inside(k.line).is_none());
        let mut all: Vec<Mark> =
            old_marks.map(|k| Mark { line: go(k.line), ..k.clone() }).collect();
        all.extend(marks);
        all.sort_by_key(|k| (k.line, k.kind == '-'));
        all.dedup_by_key(|k| k.line);
        let clean = m.text == theirs;
        // The disk's last version is kept while it conflicts with mine, and not once the buffer
        // holds it (SAFE-7); a version that arrives while mine still conflicts replaces it.
        let apart = !clean && (!m.conflicts.is_empty() || self.theirs.is_some());
        self.theirs = apart.then(|| disk.encode());
        self.buf.merged(m.text.strip_suffix('\n').unwrap_or(&m.text), disk, clean);
        let b = &mut self.buf;
        b.marks = Arc::new(all);
        let (last, line) = (b.lines() - 1, line.min(b.lines() - 1));
        b.vmark = vmark.map(|m| m.map(|(l, c)| (l.min(last), c)));
        (b.cursor, b.want) = (b.start(line) + col.min(b.line_len(line)), None);
        b.checkpoint();
        (self.marks, self.top) = ((sel.0.min(last), sel.1.min(last)), top.min(last));
        self.clamp();
    }

    /// DISK-6: `]c` and `[c` go to the next and the previous mark. The cursor has seen the marks of
    /// its own line, so the next one is a line further.
    pub(super) fn jump(&mut self, fwd: bool) {
        let (b, here) = (&self.buf, self.buf.line_of(self.buf.cursor));
        let i = b.marks.partition_point(|m| m.line < here + usize::from(fwd));
        let near = if fwd { b.marks.get(i) } else { i.checked_sub(1).and_then(|i| b.marks.get(i)) };
        let Some(line) = near.map(|m| m.line) else {
            return self.say(if fwd { "no next change" } else { "no previous change" });
        };
        let b = &mut self.buf;
        (b.cursor, b.want, self.center) = (b.nonblank(line), None, true);
    }
}
