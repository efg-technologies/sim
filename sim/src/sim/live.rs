//! GREP-1, GREP-3, GREP-4, GREP-6
//! The search of file contents in sim's state (pure): its keys, the results that arrive in
//! batches and where they go, the cursor, the preview in the editor pane, and the rows and the
//! status line it gives the screen. The tree under it is not touched: Esc puts back the panel.
use super::panel::Out;
use super::{Pv, Sim, SimEffect, text_of};
use crate::grep::{self, Batch, Group, Line, PRow, Query, Request};
use crate::layout;
use crate::paint::{Look, Looks};
use crate::syn::Spans;
use si::core::app::{App, Event, Key};
use si::core::view::{self, Frame, Row, Style};
use std::collections::HashMap;
use std::mem::take;
use std::path::PathBuf;

/// A content search. `before` is the panel's `open` and `focus` before it, for Esc.
#[derive(Default)]
pub(super) struct Live {
    query: String,
    pub(super) typing: bool,
    generation: u64,
    groups: Vec<Group>,
    /// The groups are the last query's: the first batch of the new one replaces them.
    fresh: bool,
    done: bool,
    capped: bool,
    /// The line under the cursor, counted over all the matching lines, and whether a key has moved
    /// it (else it stays on the first line as groups arrive above it); a `g` waits for a second.
    cur: usize,
    moved: bool,
    g: bool,
    top: usize,
    likely: HashMap<String, f64>,
    before: (bool, bool),
    preview: Option<Preview>,
    /// The file whose preview was asked for and has not come.
    asked: Option<String>,
}

/// The file under the cursor, as an App that is rendered and never given a key; and the frame it
/// was last rendered as, for the line, the width, the height and the query it was rendered for.
struct Preview {
    path: String,
    app: App,
    shown: Option<Shown>,
}

/// What a preview was rendered for (line, width, height, query), and the result.
type Shown = ((usize, u16, u16, String), Frame, Vec<Looks>);

impl Live {
    fn lines(&self) -> usize {
        self.groups.iter().map(|g| g.lines.len()).sum()
    }

    /// The group and the line in it of matching line `i`.
    fn at(&self, mut i: usize) -> Option<(usize, usize)> {
        for (g, group) in self.groups.iter().enumerate() {
            if i < group.lines.len() {
                return Some((g, i));
            }
            i -= group.lines.len();
        }
        None
    }

    fn pick(&self) -> Option<(&Group, &Line)> {
        let (g, i) = self.at(self.cur)?;
        Some((&self.groups[g], &self.groups[g].lines[i]))
    }

    fn index_of(&self, path: &str, no: usize) -> Option<usize> {
        let before: usize =
            self.groups.iter().take_while(|g| g.path != path).map(|g| g.lines.len()).sum();
        let group = self.groups.iter().find(|g| g.path == path)?;
        Some(before + group.lines.iter().position(|l| l.no == no)?)
    }

    /// The row of the panel under the cursor, the first row that must be in view with it (its
    /// file's row if the cursor is on the first line), and the number of rows.
    fn rows_at(&self) -> (usize, usize, usize) {
        let total = self.groups.iter().map(|g| 1 + g.lines.len()).sum();
        let Some((g, i)) = self.at(self.cur) else { return (0, 0, total) };
        let above: usize = self.groups[..g].iter().map(|x| 1 + x.lines.len()).sum();
        let row = above + 1 + i;
        (row, row - usize::from(i == 0), total)
    }

    /// The rows of the panel from row `from`, `n` of them, `max` columns wide.
    fn rows(&self, from: usize, n: usize, max: usize) -> Vec<PRow> {
        let (mut out, mut at, mut flat) = (vec![], 0, 0);
        for g in &self.groups {
            for k in (0..=g.lines.len()).filter(|k| at + k >= from) {
                if out.len() >= n {
                    return out;
                }
                out.push(if k == 0 {
                    PRow::file(&g.path, g.lines.len(), max)
                } else {
                    PRow { sel: flat + k - 1 == self.cur, ..grep::row(&g.lines[k - 1], max) }
                });
            }
            (at, flat) = (at + 1 + g.lines.len(), flat + g.lines.len());
        }
        out
    }

    /// How a file ranks (GREP-3): its likelihood, by the history.
    fn liked(&self, path: &str) -> f64 {
        self.likely.get(path).copied().unwrap_or(0.0)
    }
}

impl Sim {
    /// GREP-1: Ctrl-f. The panel opens with the focus and an empty query is typed; a search that
    /// is on goes back to typing its query.
    pub(super) fn live_start(&mut self, out: Out) {
        if let Some(l) = self.panel.live.as_mut() {
            return l.typing = true;
        }
        self.end_search();
        let before = (self.panel.open, self.panel.focus);
        if self.panel.open {
            out.push(SimEffect::Files);
        } else {
            self.show(out);
        }
        self.panel.focus = true;
        self.panel.live = Some(Live { typing: true, before, done: true, ..Live::default() });
    }

    /// Ends the search and tells the thread to stop. With `restore` the panel is as it was
    /// (Esc); else it stays as it is (Enter, Ctrl-n, Ctrl-p).
    pub(super) fn live_end(&mut self, restore: bool, out: Out) {
        let Some(l) = self.panel.live.take() else { return };
        self.live_gen += 1;
        out.push(SimEffect::Grep(Request::stop(self.live_gen)));
        if restore {
            (self.panel.open, self.panel.focus) = l.before;
        }
    }

    /// GREP-5: asks for the search of the query again (a key, or a new list of files). The old
    /// results stay until the first batch of this one comes.
    pub(super) fn live_run(&mut self, out: Out) {
        let Some(l) = self.panel.live.as_mut() else { return };
        let top = self.history.top(&self.root.to_string_lossy(), usize::MAX, self.now);
        let first = top.iter().filter(|(p, _)| self.files.binary_search(p).is_ok());
        let first: Vec<String> = first.map(|(p, _)| p.clone()).collect();
        self.live_gen += 1;
        (l.generation, l.likely) = (self.live_gen, top.into_iter().collect());
        if l.query.is_empty() {
            (l.groups, l.cur, l.moved) = (vec![], 0, false);
        }
        (l.fresh, l.done, l.capped) = (!l.query.is_empty(), l.query.is_empty(), false);
        let (root, query, files) = (self.root.clone(), l.query.clone(), self.files.clone());
        out.push(SimEffect::Grep(Request { generation: l.generation, root, query, first, files }));
        self.live_sync(out);
    }

    /// GREP-3, GREP-5: a batch of whole files. Those of an older search are dropped. Each file goes
    /// where its rank puts it, and the cursor stays on its line once a key has moved it.
    pub(super) fn live_batch(&mut self, b: Batch, out: Out) {
        let Some(l) = self.panel.live.as_mut().filter(|l| l.generation == b.generation) else {
            return;
        };
        let was = l.pick().map(|(g, ln)| (g.path.clone(), ln.no)).filter(|_| l.moved && !l.fresh);
        if take(&mut l.fresh) {
            (l.groups, l.cur, l.moved) = (vec![], 0, false);
        }
        for g in b.groups {
            let before = |x: &Group| {
                grep::rank((&x.path, l.liked(&x.path)), (&g.path, l.liked(&g.path))).is_lt()
            };
            let at = l.groups.partition_point(before);
            l.groups.insert(at, g);
        }
        (l.done, l.capped) = (b.done, b.capped);
        l.cur = was.and_then(|(p, no)| l.index_of(&p, no)).unwrap_or(l.cur);
        self.live_sync(out);
    }

    /// GREP-4: asks for the preview of the file under the cursor, unless it is the one shown or
    /// already asked for.
    fn live_sync(&mut self, out: Out) {
        let Some(l) = self.panel.live.as_mut() else { return };
        let Some(path) = l.pick().map(|(g, _)| g.path.clone()) else { return };
        if l.preview.as_ref().is_some_and(|p| p.path == path) || l.asked.as_ref() == Some(&path) {
            return;
        }
        l.asked = Some(path.clone());
        out.push(SimEffect::Preview(path));
    }

    /// GREP-4: the file has been read. An answer for a file that is no longer under the cursor is dropped.
    pub(super) fn live_preview(&mut self, pv: Pv) {
        let (w, h) = (self.widths().1, self.size.1);
        let Some(l) = self.panel.live.as_mut() else { return };
        l.asked = l.asked.take().filter(|a| *a != pv.path);
        if l.pick().is_none_or(|(g, _)| g.path != pv.path) {
            return;
        }
        let app = App::new(Some(pv.path.clone()), pv.file, (w, h));
        l.preview = Some(Preview { path: pv.path, app, shown: None });
    }

    /// GREP-1, GREP-4: a key while the search is on.
    pub(super) fn live_key(&mut self, k: Key, out: Out) {
        let page = usize::from(self.size.1) - 1;
        let Some(l) = self.panel.live.as_mut() else { return };
        if l.typing && !matches!(k, Key::Down | Key::Up) {
            match k {
                Key::Char(c) => l.query.push(c),
                Key::Backspace if l.query.is_empty() => return self.live_end(true, out),
                Key::Backspace => drop(l.query.pop()),
                Key::Esc => return self.live_end(true, out),
                Key::Enter => return self.live_open(out),
                _ => return,
            }
            return self.live_run(out);
        }
        l.typing = false;
        let (cur, last, g) = (l.cur, l.lines().saturating_sub(1), take(&mut l.g));
        let to = match k {
            Key::Char('j') | Key::Down => (cur + 1).min(last),
            Key::Char('k') | Key::Up => cur.saturating_sub(1),
            Key::Char('n') => (cur + 1) % (last + 1),
            Key::Char('N') => (cur + last) % (last + 1),
            Key::Ctrl('d') => cur + page / 2,
            Key::Ctrl('u') => cur.saturating_sub(page / 2),
            Key::PageDown => cur + page,
            Key::PageUp => cur.saturating_sub(page),
            Key::Char('g') if g => 0,
            Key::Char('g') => {
                l.g = true;
                cur
            }
            Key::Char('G') => last,
            Key::Char('/') | Key::Ctrl('f') => return l.typing = true,
            Key::Enter => return self.live_open(out),
            Key::Esc => return self.live_end(true, out),
            _ => cur,
        };
        (l.cur, l.moved) = (to.min(last), true);
        self.live_sync(out);
    }

    /// GREP-4: Enter. The panel stays as the tree (with the file shown in it), and the file is
    /// opened as the tree's Enter does, at the line and the column of the match.
    fn live_open(&mut self, out: Out) {
        let Some(l) = self.panel.live.as_ref().filter(|l| !l.fresh) else { return };
        let Some((g, ln)) = l.pick() else { return };
        let (path, line, col) = (g.path.clone(), ln.no, ln.col);
        self.live_end(false, out);
        self.panel.reveal = Some(PathBuf::from(&path));
        self.reveal(out);
        let at = self.root.join(&path);
        let open =
            self.bufs.iter().find(|b| b.app.path.as_ref().is_some_and(|p| self.cwd.join(p) == at));
        let (id, keys) = (
            open.map(|b| b.id),
            format!(":{line}\n{}", if col > 0 { format!("{col}l") } else { String::new() }),
        );
        self.open(at, out);
        match id {
            Some(id) if self.idle() => {
                for c in keys.chars() {
                    self.to(id, Event::Key(if c == '\n' { Key::Enter } else { Key::Char(c) }), out);
                }
            }
            Some(_) => {}
            None => self.keys = keys,
        }
    }

    /// GREP-6: the status line and the column of the terminal's cursor at the end of the query:
    /// `/{query}  {count}`, the query cut from the left to keep the count, and when the results
    /// are browsed the path and line under the cursor, made to fit what is left.
    pub(super) fn live_status(&self, room: usize) -> Option<(String, usize)> {
        let l = self.panel.live.as_ref()?;
        let (n, files) = (l.lines(), l.groups.len());
        let found = if l.query.is_empty() {
            String::new()
        } else if l.fresh || (!l.done && n == 0) {
            "  searching…".into()
        } else if n == 0 {
            "  no matches".into()
        } else {
            let more = if l.capped { "+" } else { "" };
            let (m, f) =
                (if n == 1 && !l.capped { "" } else { "es" }, if files == 1 { "" } else { "s" });
            format!("  {n}{more} match{m} in {files} file{f}{}", if l.done { "" } else { "…" })
        };
        let (found, used) = layout::fit(&found, room);
        let head = layout::tail(&format!("/{}", l.query), room - used);
        let cols = layout::fit(&head, usize::MAX).1;
        let mut status = head + &found;
        let left = room.saturating_sub(cols + used);
        if let (false, false, Some((g, ln)), true) = (l.typing, l.fresh, l.pick(), left > 8) {
            status += &format!("  {}", layout::short(&format!("{}:{}", g.path, ln.no), left - 2));
        }
        Some((status, cols))
    }

    /// Keeps the cursor row on screen, and its file's row with it when there is room.
    pub(super) fn live_scroll(&mut self) {
        let rows = usize::from(self.size.1) - 1;
        let Some(l) = self.panel.live.as_mut() else { return };
        let (row, anchor, total) = l.rows_at();
        let lo = (row + 1).saturating_sub(rows);
        l.top = l.top.min(total.saturating_sub(rows)).clamp(lo, anchor.max(lo));
    }

    /// GREP-3: the results in the panel's columns of the body rows `layout::body` made with no panel.
    pub(super) fn live_overlay(&self, rows: &mut [Row], looks: &mut [Looks], pw: u16) {
        let Some(l) = self.panel.live.as_ref().filter(|_| pw > 0) else { return };
        for (i, p) in l.rows(l.top, rows.len(), usize::from(pw)).into_iter().enumerate() {
            let used = layout::fit(&p.text, usize::MAX).1;
            let editor: String = rows[i].text.chars().skip(usize::from(pw) + 1).collect();
            rows[i].text =
                format!("{}{} {editor}", p.text, " ".repeat(usize::from(pw).saturating_sub(used)));
            rows[i].spans.extend((!p.dim.is_empty()).then_some((p.dim, Style::Secondary)));
            rows[i].underline.extend(p.sel.then_some(0..used as u16));
            looks[i].extend(p.bold.into_iter().map(|r| (r, Look::Bold)));
        }
    }

    /// The row of the terminal's cursor while the results are browsed: the one under the cursor.
    pub(super) fn live_cursor(&self) -> u16 {
        self.panel.live.as_ref().map_or(0, |l| l.rows_at().0.saturating_sub(l.top) as u16)
    }

    /// GREP-4: the editor pane while the cursor is on a result: the file with the line in the
    /// middle and the matches on it bold. The frame is made again only when the line, the width or
    /// the height changed; until the file under the cursor has been read the last one stays.
    pub(super) fn live_pane(&mut self) -> Option<(Frame, Vec<Looks>)> {
        let (w, h) = (self.widths().1, self.size.1);
        let l = self.panel.live.as_mut()?;
        let (path, no) = l.pick().map(|(g, ln)| (g.path.clone(), ln.no))?;
        let (query, p) = (Query::new(&l.query), l.preview.as_mut()?);
        let key = (no, w, h, l.query.clone());
        if p.path == path && p.shown.as_ref().is_none_or(|s| s.0 != key) {
            let (line, buf) = (no - 1, &mut p.app.buf);
            buf.cursor = buf.start(line.min(buf.lines().saturating_sub(1)));
            (p.app.size, p.app.center) = ((w, h), true);
            let frame = view::render(&mut p.app);
            let buf = &p.app.buf;
            let marks =
                query.zip(text_of(buf, line)).map(|(q, t)| q.ranges(&t)).unwrap_or_default();
            let bold: Spans = marks.into_iter().map(|r| (r, Look::Bold)).collect();
            let digits = buf.lines().to_string().len().max(3);
            let looks = layout::colors(&frame, digits, &|l| text_of(buf, l), &|l| {
                (l == line).then_some(&bold)
            })
            .0;
            p.shown = Some((key, frame, looks));
        }
        p.shown.as_ref().map(|(_, f, l)| (f.clone(), l.clone()))
    }
}
