//! DIFF-7, DIFF-8, DIFF-9, DIFF-10
//! The changes view as a watchtower, in Sim's state (pure). While the view has the focus the text pane shows
//! the diff of the file under its cursor against HEAD: git's patch made into a core App that is rendered and
//! never given a key, so that a hidden char is its form (SAFE-8, SAFE-10). `]c` and `[c` walk its hunks and go
//! on into the next file's, `J` and `K` scroll it, Enter opens the file at the hunk shown. It is asked again
//! with every answer of git's status, so it follows the disk (WATCH-1); FOLLOW (`f`) takes the view's cursor to
//! the file another process wrote last.
use super::panel::Out;
use super::{PatchReq, Sim, SimEffect};
use crate::layout::{self, Kind};
use crate::marks::Git;
use crate::patch::{Is, Patch};
use sim_core::core::app::App;
use sim_core::core::view::{self, Frame, Style};
use std::path::{Path, PathBuf};

/// The preview shown: its file, its patch (the text is the App's), the App (never given a key), the line of
/// the text it starts at, the hunk shown, and what it was last drawn as.
pub(super) struct Preview {
    pub(super) path: PathBuf,
    pub(super) patch: Patch,
    pub(super) app: App,
    top: usize,
    hunk: usize,
    pub(super) drawn: Option<Drawn>,
}

/// The preview drawn for a pane's size and a first line: the frame, what each of its rows is (LOOK-2's
/// tints) and the lines of the text on screen (LOOK-4's ruler).
pub(super) struct Drawn {
    key: (u16, u16, usize),
    pub(super) frame: Frame,
    pub(super) ch: Vec<Kind>,
    pub(super) win: Option<(usize, usize)>,
}

#[derive(Default)]
pub(super) struct Tower {
    pub(super) shown: Option<Preview>,
    /// The request that waits for its answer: its number, its file, and whether its last hunk is shown (`[c`).
    asked: Option<(u64, PathBuf, bool)>,
    n: u64,
    /// DIFF-10: FOLLOW is on, and the file another process wrote last, until the view has its row.
    pub(super) follow: bool,
    pub(super) wrote: Option<PathBuf>,
    /// The view has just opened: its cursor stays on the first row until git's first answer has made it.
    pub(super) fresh: bool,
}

impl Sim {
    /// DIFF-7: whether the text pane shows the preview: the changes view has the focus.
    pub(super) fn towering(&self) -> bool {
        self.changes.is_some() && self.panel.open && self.panel.focus
    }

    /// The file under the view's cursor (none on a folder).
    fn under(&self) -> Option<PathBuf> {
        self.row(self.panel.cur).filter(|r| !r.is_dir).map(|r| r.path)
    }

    fn ask_patch(&mut self, path: PathBuf, last: bool, out: Out) {
        let (new, old) = (
            self.marks.git(&path) == Some(Git::Untracked),
            self.marks.renamed_from(&path).map(Path::to_path_buf),
        );
        let t = &mut self.tower;
        t.n += 1;
        t.asked = Some((t.n, path.clone(), last));
        out.push(SimEffect::Patch(PatchReq { n: t.n, path, old, new }));
    }

    /// DIFF-9: git's status has answered (something changed): the preview is asked for again, and keeps its
    /// hunk; where the view has no focus it is let go, to be asked for when it has.
    pub(super) fn patch_again(&mut self, out: Out) {
        let Some(path) = self.tower.shown.as_ref().map(|p| p.path.clone()) else { return };
        if self.towering() { self.ask_patch(path, false, out) } else { self.tower.shown = None }
    }

    /// DIFF-7: the answer; one to a request that is not the newest is dropped. The same file keeps its hunk.
    pub(super) fn patched(&mut self, n: u64, mut patch: Patch) {
        let Some((_, path, last)) = self.tower.asked.take_if(|a| a.0 == n) else { return };
        let text = std::mem::take(&mut patch.text).into_bytes();
        let mut app = App::new(Some(path.to_string_lossy().into_owned()), Ok(Some(text)), (1, 2));
        app.wide = true; // (UI-9)
        let kept = self.tower.shown.take().filter(|p| p.path == path).map(|p| p.hunk);
        let end = patch.hunks.len().saturating_sub(1);
        let hunk = kept.unwrap_or(if last { end } else { 0 }).min(end);
        let top = patch.hunks.get(hunk).map_or(0, |h| h.0);
        self.tower.shown = Some(Preview { path, patch, app, top, hunk, drawn: None });
    }

    /// DIFF-7: before a frame: the preview of the file under the cursor is asked for if it is neither the one
    /// shown nor the one asked for (until it comes the last one stays, as the picker's does), and the one shown
    /// is drawn for the pane and its first line.
    pub(super) fn tower_prepare(&mut self, out: Out) {
        if !self.towering() {
            return;
        }
        let t = &self.tower;
        if let Some(p) = self.under()
            && t.shown.as_ref().is_none_or(|s| s.path != p)
            && t.asked.as_ref().is_none_or(|a| a.1 != p)
        {
            self.ask_patch(p, false, out);
        }
        let g = self.geo();
        let Some(p) = self.tower.shown.as_mut() else { return };
        let key = (g.ew, g.rows + 1, p.top);
        if p.drawn.as_ref().is_none_or(|d| d.key != key) {
            p.drawn = Some(draw(p, key));
        }
    }

    /// DIFF-8: `]c` and `[c`: the next or the previous hunk of the preview; past its last (before its first)
    /// the first hunk of the next file of the view (the last of the previous one).
    pub(super) fn hunk(&mut self, back: bool, out: Out) {
        let here = self.under();
        if let Some(p) = self.tower.shown.as_mut().filter(|p| Some(&p.path) == here.as_ref()) {
            let to = if back { p.hunk.checked_sub(1) } else { Some(p.hunk + 1) };
            if let Some(h) = to.filter(|&h| h < p.patch.hunks.len()) {
                return (p.hunk, p.top) = (h, p.patch.hunks[h].0);
            }
        }
        let rows: Vec<usize> = if back {
            (0..self.panel.cur).rev().collect()
        } else {
            (self.panel.cur + 1..self.view().visible()).collect()
        };
        let file = rows.into_iter().find_map(|i| Some((i, self.row(i).filter(|r| !r.is_dir)?)));
        if let Some((i, r)) = file {
            self.panel.cur = i;
            self.ask_patch(r.path, back, out);
        }
    }

    /// DIFF-8: `J` and `K` (and the wheel over the pane) scroll the preview by `by` lines; the hunk shown is the
    /// one its first line is in.
    pub(super) fn roll_patch(&mut self, by: isize) {
        let rows = usize::from(self.geo().rows);
        let Some(p) = self.tower.shown.as_mut() else { return };
        let max = p.app.buf.lines().saturating_sub(rows);
        p.top = p.top.min(max).saturating_add_signed(by).min(max);
        p.hunk = p.patch.hunks.iter().rposition(|h| h.0 <= p.top).unwrap_or(0);
    }

    /// DIFF-8: the keys Enter types into the file it opens from the view: the first line the hunk shown
    /// changes, where the file's preview is the one shown.
    pub(super) fn at_hunk(&self, path: &Path) -> String {
        let p = self.tower.shown.as_ref().filter(|p| self.changes.is_some() && p.path == path);
        p.and_then(|p| p.patch.hunks.get(p.hunk)).map_or(String::new(), |h| format!(":{}\n", h.1))
    }

    /// DIFF-10: FOLLOW takes the view's cursor to the file another process wrote last, once the view has its row.
    pub(super) fn follow(&mut self) {
        let row = self.tower.wrote.as_ref().filter(|_| self.tower.follow && self.changes.is_some());
        if let Some(i) = row.and_then(|p| self.view().row_of(p)) {
            (self.panel.cur, self.tower.wrote) = (i, None);
        }
    }
}

/// DIFF-7: the preview's frame from line `top` of its text on, for a pane of `w` x `h` (its status line
/// included): the numbers are the lines of the file now (none for a line that went and for a header), all in
/// the secondary gray, and so is a header's text; and what each row is, for LOOK-2's tints (a line that came is
/// a gain, one that went a loss, and the wrapped rows of a line are what it is).
fn draw(p: &mut Preview, key: (u16, u16, usize)) -> Drawn {
    let ((w, h, top), buf) = (key, &mut p.app.buf);
    let rows = usize::from(h).saturating_sub(1).max(1);
    let top = top.min(buf.lines().saturating_sub(rows));
    // (the core keeps `min(8, (rows - 1) / 2)` rows above the cursor, VIS-12: the cursor is put that far down)
    let at = (top + 8.min(rows.saturating_sub(1) / 2)).min(buf.lines().saturating_sub(1));
    buf.cursor = buf.start(at);
    (p.app.size, p.app.top) = ((w, h), top);
    let mut frame = view::render(&mut p.app);
    let digits = p.app.buf.lines().to_string().len().max(3);
    let (mut ch, mut kind, mut win) = (vec![], Kind::Plain, None::<(usize, usize)>);
    let body = frame.rows.len().saturating_sub(1);
    for row in &mut frame.rows[..body] {
        match layout::number(row, digits) {
            Some(n) => {
                let (is, now) = p.patch.lines.get(n - 1).copied().unwrap_or((Is::Same, None));
                win = Some((win.map_or(n - 1, |w| w.0), n - 1));
                let now = now.map(|k| k.to_string()).filter(|s| s.len() <= digits);
                row.text
                    .replace_range(1..1 + digits, &format!("{:>digits$}", now.unwrap_or_default()));
                if !row.spans.iter().any(|s| s.0.start == 1) {
                    row.spans.insert(0, (1..1 + digits as u16, Style::Secondary));
                }
                if is == Is::Head {
                    let end = layout::fit(&row.text, usize::MAX).1 as u16;
                    row.spans.push((digits as u16 + 2..end, Style::Secondary)); // (after a hidden char's)
                }
                kind = match is {
                    Is::Came => Kind::Gain,
                    Is::Went => Kind::Loss,
                    _ => Kind::Plain,
                };
            }
            None if row.text.is_empty() => kind = Kind::Plain,
            None => {}
        }
        ch.push(kind);
    }
    Drawn { key, frame, ch, win }
}
