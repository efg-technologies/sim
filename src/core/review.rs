//! PROP-8, SAFE-7, VIS-12
//! REVIEW mode: `]c` and `[c` visit the marks and hunks, `y n Y N` decide the hunks, and the
//! refusals that keep an unresolved one from being lost.
use super::app::{App, plural};
use super::vi::Mode;

pub(super) const CONFLICTS: &str =
    "unresolved conflicts with the disk: ]c to review, :w! keeps yours";
pub(super) const PROPOSALS: &str = "unresolved proposals: ]c to review, :q! discards";

impl App {
    /// The line the cursor goes to for hunk `i`: its first line, or the line after its place.
    fn anchor(&self, i: usize) -> usize {
        self.buf.hunks[i].mine.start.min(self.buf.lines() - 1)
    }

    /// The hunk REVIEW is on: the one `rev` names while the cursor is at its line, else the first
    /// one at or after the cursor (a merge may have moved the hunks meanwhile).
    pub fn current(&self) -> usize {
        let (b, here) = (&self.buf, self.buf.line_of(self.buf.cursor));
        if self.rev < b.hunks.len() && self.anchor(self.rev) == here {
            return self.rev;
        }
        let first = b.hunks.partition_point(|h| h.mine.start.min(b.lines() - 1) < here);
        first.min(b.hunks.len().saturating_sub(1))
    }

    /// `]c` / `[c`: to the next (previous) mark or hunk. A hunk starts REVIEW, a mark ends it. In
    /// REVIEW the next hunk is the next one in the list; in Normal mode a hunk on the cursor's own
    /// line is the next one (nothing has decided it), a mark is not (the cursor has seen it).
    pub(super) fn jump(&mut self, fwd: bool) {
        let (b, here) = (&self.buf, self.buf.line_of(self.buf.cursor));
        let (hn, mn, review) = (b.hunks.len(), b.marks.len(), self.vi.mode == Mode::Review);
        let (last, cur) = (b.lines() - 1, if review { self.current() } else { 0 });
        // How many hunks and marks lie behind the cursor in this direction.
        #[rustfmt::skip]
        let hb = if review { cur + usize::from(fwd) } else { b.hunks.partition_point(|h| h.mine.start.min(last) < here) };
        let mb = b.marks.partition_point(|m| m.line < here + usize::from(fwd));
        let step = |i: usize| if fwd { Some(i) } else { i.checked_sub(1) };
        // (line, is a mark, index) of the nearest hunk and the nearest mark.
        let hunk = step(hb).filter(|&i| i < hn).map(|i| (self.anchor(i), false, i));
        let mark = step(mb).filter(|&i| i < mn).map(|i| (b.marks[i].line, true, i));
        let both = [hunk, mark].into_iter().flatten();
        #[rustfmt::skip]
        let near = if fwd { both.min_by_key(|c| (c.0, c.1)) } else { both.max_by_key(|c| (c.0, !c.1)) };
        let Some((line, mark, i)) = near else {
            return self.say(if fwd { "no next change" } else { "no previous change" });
        };
        let b = &mut self.buf;
        (b.cursor, b.want, self.center) = (b.nonblank(line), None, true);
        if !mark {
            (self.rev, self.vi.mode) = (i, Mode::Review);
        } else if review {
            self.vi.mode = Mode::Normal;
        }
    }

    /// `y` `n` (the current hunk) and `Y` `N` (all of them): one undo unit each, then on to the
    /// next hunk. REVIEW ends when none is left.
    pub(super) fn decide(&mut self, accept: bool, all: bool) {
        let (i, was, line) =
            (self.current(), self.buf.hunks.len(), self.buf.line_of(self.buf.cursor));
        self.buf.resolve((!all).then_some(i), accept);
        let left = self.buf.hunks.len();
        let next = if i < left { i } else { 0 };
        let line = if left == 0 { line.min(self.buf.lines() - 1) } else { self.anchor(next) };
        self.buf.cursor = self.buf.nonblank(line);
        if left == 0 {
            self.vi.mode = Mode::Normal;
            let what = plural(was - left, "proposal", "proposals");
            self.say(format!("{what} {}", if accept { "accepted" } else { "rejected" }));
        } else {
            (self.rev, self.center) = (next, true);
        }
        self.clamp();
    }
}
