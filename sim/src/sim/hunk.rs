//! DIFF-11
//! The hunks against HEAD in the text (pure): `<Space>gr` gives the hunk under the cursor back to HEAD as
//! one edit of the buffer that `u` undoes, and `]h` `[h` walk the hunks. Nothing is written and git is asked
//! nothing: HEAD's lines are those of the diff the signs were made of (UI-5), and a stale one is refused.
use super::ptr::put;
use super::{Sim, is_head};
use crate::diff::Hunk;
use crate::marks::Git;

impl Sim {
    /// `<Space>gr`: HEAD's lines of the hunk the cursor is in, in the place of the buffer's, where the buffer is
    /// the saved text the diff was asked for; else why not, and nothing changes.
    pub(super) fn reset_hunk(&mut self) {
        let untracked =
            self.open_path().is_some_and(|p| self.marks.git(&p) == Some(Git::Untracked));
        let Some(b) = self.bufs.get(self.cur) else { return };
        let (buf, d) = (&b.app.buf, b.diff.as_ref());
        let cl = buf.line_of(buf.cursor);
        let on = |h: &&Hunk| (h.c.max(1) - 1..h.c.max(1) - 1 + h.d.max(1)).contains(&cl);
        let h = d.and_then(|d| d.hunks.iter().find(on)).cloned();
        let why = match (d, &h) {
            _ if self.plain => "plain mode runs no program: no hunks",
            _ if buf.readonly.is_some() => buf.readonly.as_deref().unwrap_or("read-only"),
            _ if is_head(b) || untracked || self.no_repo => {
                "hunk not reset: git does not track this file"
            }
            (None, _) => "hunk not reset: the signs are not known yet",
            (Some(d), _) if buf.dirty() || d.ver != buf.version() => {
                "hunk not reset: the signs are older than the text (:w, then again)"
            }
            (_, None) => "no hunk here",
            (_, Some(h)) if h.old.iter().any(|l| l.contains('\u{fffd}')) => {
                "hunk not reset: HEAD's lines are not valid UTF-8"
            }
            _ => "",
        }
        .to_string();
        let (Some(h), true, Some(b)) = (h, why.is_empty(), self.bufs.get_mut(self.cur)) else {
            return self.msg = Some(why);
        };
        // the lines `a..a + d` become HEAD's; the last line of the text has no line break of its own
        let buf = &mut b.app.buf;
        let (a, n) = (if h.d == 0 { h.c } else { h.c - 1 }, buf.lines());
        let tail = a + h.d >= n || buf.text.len_chars() == 0;
        let from = if tail && a > 0 { buf.end(a - 1) } else { buf.start(a) };
        let to = if tail { buf.text.len_chars() } else { buf.start(a + h.d) };
        let with: String = match (tail, a > 0) {
            (true, true) => h.old.iter().map(|l| format!("\n{l}")).collect(),
            (true, false) => h.old.join("\n"),
            _ => h.old.iter().map(|l| format!("{l}\n")).collect(),
        };
        buf.checkpoint();
        buf.replace(from..to, &with);
        buf.cursor = buf.nonblank(a.min(buf.lines() - 1));
        b.frame = None;
        let n = h.b.max(h.d);
        self.msg =
            Some(format!("hunk reset: {n} line{} (u undoes)", if n == 1 { "" } else { "s" }));
    }

    /// `]h` and `[h`: the first line of the next (`back`: the previous) hunk against HEAD, wrapping round, where
    /// its sign is drawn (shifted as the signs are, UI-5), in the middle of the screen.
    pub(super) fn walk_hunks(&mut self, back: bool) {
        let Some(b) = self.bufs.get_mut(self.cur) else { return };
        let (buf, diff) = (&b.app.buf, b.diff.as_ref());
        let (cl, now) = (buf.line_of(buf.cursor), buf.lines());
        let lines = diff.map_or(0, |d| d.lines);
        let shift = |l: usize| if l > cl { (l + now).saturating_sub(lines) } else { l };
        let starts: Vec<usize> =
            diff.map_or(vec![], |d| d.hunks.iter().map(|h| shift(h.c.max(1) - 1)).collect());
        let next = if back {
            starts.iter().rev().find(|&&l| l < cl).or(starts.last())
        } else {
            starts.iter().find(|&&l| l > cl).or(starts.first())
        };
        let Some(&l) = next else { return self.msg = Some("no hunks against HEAD".into()) };
        let at = buf.nonblank(l.min(now - 1));
        put(&mut b.app, at);
        (b.app.center, b.frame) = (true, None);
    }
}
