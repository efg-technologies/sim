//! GREP-1, GREP-3, GREP-4, GREP-6, PICK-2, PICK-3, PICK-4, PICK-5
//! The picker in sim's state (pure): Ctrl-f's search of file contents,
//! their keys, the results (the content search's arrive in batches), the selection, the preview of
//! the selected file, and what the window shows. The panel and the files are not touched: Esc
//! closes the picker and everything is as it was.
use super::panel::Out;
use super::{Hl, Id, MAX_HL, Pv, Req, Sim, SimEffect, text_of};
use crate::color::Tones;
use crate::grep::{self, Batch, Group, Line, PRow, Query, Request};
use crate::layout::{self, Line as Cells};
use crate::paint::{Look, Looks};
use crate::pick::{self, Lit, Pane, SPIN, View, Win};
use crate::syn::Spans;
use si::core::app::{App, Event, Key};
use si::core::view::{self, Frame};
use std::collections::HashMap;
use std::mem::take;
use std::path::PathBuf;

/// Which picker it is: Ctrl-f's search of contents.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Pick {
    #[default]
    Grep,
}

/// A picker.
#[derive(Default)]
pub(super) struct Live {
    kind: Pick,
    query: String,
    generation: u64,
    groups: Vec<Group>,
    /// The groups are the last query's: the first batch of the new one replaces them.
    fresh: bool,
    done: bool,
    capped: bool,
    /// The result under the selection, counted over all the results, and whether a key has moved it
    /// (else it stays on the first result as more arrive).
    cur: usize,
    moved: bool,
    top: usize,
    spin: usize,
    likely: HashMap<String, f64>,
    preview: Option<Preview>,
    /// The file whose preview was asked for and has not come.
    asked: Option<String>,
}

/// The file under the selection, as an App that is rendered and never given a key (GREP-4), its
/// id for the highlighter, the spans it answered with and the window they were asked for, the
/// reason there is no text (a file too big), and what it was last rendered as.
struct Preview {
    path: String,
    id: Id,
    app: App,
    note: Option<String>,
    hl: Option<Hl>,
    asked: Option<(usize, usize)>,
    shown: Option<Shown>,
}

/// What a preview was rendered for (line, size, query, the window of spans, whether colors are
/// off), and the result.
type Rendered = (usize, u16, u16, String, Option<(usize, usize)>, bool);

struct Shown {
    key: Rendered,
    title: String,
    frame: Frame,
    looks: Vec<Looks>,
    lit: Option<Lit>,
}

impl Live {
    fn lines(&self) -> usize {
        self.groups.iter().map(|g| g.lines.len()).sum()
    }

    /// The group and the line in it of result `i`.
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

    /// The rows of the list from result `from`, `n` of them, `max` columns wide (PICK-3).
    fn rows(&self, from: usize, n: usize, max: usize) -> Vec<PRow> {
        let all = self.groups.iter().flat_map(|g| g.lines.iter().map(move |l| (g, l)));
        let row = |(i, (g, l)): (usize, (&Group, &Line))| PRow {
            sel: i == self.cur,
            ..grep::row(&g.path, l, max)
        };
        all.enumerate().skip(from).take(n).map(row).collect()
    }

    /// How a file ranks (GREP-3): its likelihood, by the history.
    fn liked(&self, path: &str) -> f64 {
        self.likely.get(path).copied().unwrap_or(0.0)
    }

    /// PICK-5: whether a search is still running (or the results shown are the last query's).
    fn running(&self) -> bool {
        !self.done || self.fresh
    }
}

impl Sim {
    /// PICK-1: where the picker's window is, if it is open and the screen has room for it.
    pub fn pick_win(&self) -> Option<Win> {
        self.panel.live.as_ref()?;
        Win::new(self.size)
    }

    /// Whether a picker is open (every key is its own then).
    pub fn picker_open(&self) -> bool {
        self.panel.live.is_some()
    }

    /// GREP-1: Ctrl-f. The picker opens with an empty query and the list of files is read again.
    pub(super) fn live_start(&mut self, kind: Pick, out: Out) {
        self.live_end(out);
        out.push(SimEffect::Files);
        self.panel.live = Some(Live { kind, done: true, ..Live::default() });
        self.live_run(out);
    }

    /// Closes the picker: the thread of a search is told to stop and the highlighter forgets the preview.
    pub(super) fn live_end(&mut self, out: Out) {
        let Some(l) = self.panel.live.take() else { return };
        if l.kind == Pick::Grep {
            self.live_gen += 1;
            out.push(SimEffect::Grep(Request::stop(self.live_gen)));
        }
        out.extend(l.preview.map(|p| SimEffect::Close(p.id)));
    }

    /// GREP-5: asks for the search of the query again (a key, or a new list of files).
    pub(super) fn live_run(&mut self, out: Out) {
        match self.panel.live.as_ref().map(|l| l.kind) {
            Some(Pick::Grep) => self.grep_run(out),
            None => {}
        }
        self.live_sync(out);
    }

    /// GREP-5: the old results stay until the first batch of this search comes.
    fn grep_run(&mut self, out: Out) {
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
        out.extend(l.running().then_some(SimEffect::Spin(l.generation)));
    }

    /// GREP-3, GREP-5: a batch of whole files. Those of an older search are dropped. Each file goes
    /// where its rank puts it, and the selection stays on its line once a key has moved it.
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

    /// PICK-5: the next frame of the spinner, while the search of that generation runs.
    pub(super) fn live_spin(&mut self, generation: u64, out: Out) {
        if let Some(l) =
            self.panel.live.as_mut().filter(|l| l.generation == generation && l.running())
        {
            l.spin += 1;
            out.push(SimEffect::Spin(generation));
        }
    }

    /// GREP-4: asks for the preview of the file under the selection, unless it is the one shown or
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

    /// GREP-4: the file has been read. An answer for a file that is no longer selected is dropped.
    pub(super) fn live_preview(&mut self, pv: Pv, out: Out) {
        let (w, h) = self.pick_win().map_or((1, 2), |w| (w.pane.w.max(1), w.pane.h + 1));
        let id = self.next;
        let Some(l) = self.panel.live.as_mut() else { return };
        l.asked = l.asked.take().filter(|a| *a != pv.path);
        if l.pick().is_none_or(|(g, _)| g.path != pv.path) {
            return;
        }
        self.next += 1;
        let (note, file) = match pv.file {
            Err(e) => (Some(e), Ok(None)),
            ok => (None, ok),
        };
        let mut app = App::new(Some(pv.path.clone()), file, (w, h));
        app.wide = true; // (UI-9)
        let p = Preview { path: pv.path, id, app, note, hl: None, asked: None, shown: None };
        out.extend(l.preview.replace(p).map(|o| SimEffect::Close(o.id)));
    }

    /// PICK-4: the highlighter's answer for the preview's id.
    pub(super) fn live_spans(&mut self, h: Hl) {
        let p = self.panel.live.as_mut().and_then(|l| l.preview.as_mut());
        if let Some(p) = p.filter(|p| p.id == h.id) {
            p.hl = Some(h);
        }
    }

    /// GREP-1, PICK-2: a key while the picker is open. It is always the picker's.
    pub(super) fn live_key(&mut self, k: Key, out: Out) {
        let page = self.pick_win().map_or(1, |w| usize::from(w.list.h).max(1));
        let Some(l) = self.panel.live.as_mut() else { return };
        let (cur, last) = (l.cur, l.lines().saturating_sub(1));
        let to = match k {
            Key::Char(c) => return self.live_edit(|q| q.push(c), out),
            Key::Backspace if l.query.is_empty() => return self.live_end(out),
            Key::Backspace => return self.live_edit(|q| _ = q.pop(), out),
            Key::Ctrl('u') => return self.live_edit(String::clear, out),
            Key::Ctrl('w') => return self.live_edit(word, out),
            Key::Esc => return self.live_end(out),
            Key::Enter => return self.live_open(out),
            Key::Down | Key::Ctrl('n' | 'j') => cur + 1,
            Key::Up | Key::Ctrl('p' | 'k') => cur.saturating_sub(1),
            Key::PageDown => cur + page,
            Key::PageUp => cur.saturating_sub(page),
            _ => return,
        };
        (l.cur, l.moved) = (to.min(last), true);
        self.live_sync(out);
    }

    /// Changes the query, and searches it.
    fn live_edit(&mut self, edit: impl FnOnce(&mut String), out: Out) {
        let Some(l) = self.panel.live.as_mut() else { return };
        edit(&mut l.query);
        l.moved = false;
        self.live_run(out);
    }

    /// GREP-4: Enter. The picker closes, the file is shown in the tree as the tree's Enter
    /// shows it, and opened as `file:line:col` does, at the line and the column of the match.
    fn live_open(&mut self, out: Out) {
        let Some(l) = self.panel.live.as_ref().filter(|l| !l.fresh) else { return };
        let Some((g, ln)) = l.pick() else { return };
        let (path, line, col) = (g.path.clone(), ln.no, ln.col);
        self.live_end(out);
        self.panel.reveal = Some(PathBuf::from(&path));
        self.reveal(out);
        let at = self.root.join(&path);
        let open =
            self.bufs.iter().find(|b| b.app.path.as_ref().is_some_and(|p| self.cwd.join(p) == at));
        let id = open.map(|b| b.id);
        let keys = match (line, col) {
            (0, _) => String::new(),
            (_, 0) => format!(":{line}\n"),
            _ => format!(":{line}\n{col}l"),
        };
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

    /// PICK-3, PICK-4: keeps the selected row in view and the preview made: rendered for the line, the
    /// pane's size and the query when any of them changed, with the spans asked for when the ones
    /// there do not cover the pane (PICK-4).
    pub(super) fn live_prepare(&mut self, out: Out) {
        let Some(win) = self.pick_win() else { return };
        let (off, rows) = (self.hl_off, usize::from(win.list.h).max(1));
        let Some(l) = self.panel.live.as_mut() else { return };
        let total = l.lines();
        l.cur = l.cur.min(total.saturating_sub(1));
        l.top =
            l.top.min(total.saturating_sub(rows)).clamp((l.cur + 1).saturating_sub(rows), l.cur);
        let Some((path, no)) = l.pick().map(|(g, ln)| (g.path.clone(), ln.no)) else { return };
        if win.pane.h == 0 {
            return; // (a window with no room for a preview: si renders no pane of no rows)
        }
        let query = l.query.clone();
        let Some(p) = l.preview.as_mut().filter(|p| p.path == path) else { return };
        let (line, ph) = (no.saturating_sub(1), usize::from(win.pane.h));
        let (lines, ver) = (p.app.buf.lines(), p.app.buf.version());
        let (lo, hi) = (line.saturating_sub(ph), (line + ph).min(lines.saturating_sub(1)));
        let covered = |h: &Hl| h.from <= lo && hi < h.from + h.spans.len();
        let ask = (line.saturating_sub(2 * ph), 5 * ph + 1);
        let on = !off && p.app.buf.text.len_bytes() <= MAX_HL && p.note.is_none();
        if on && !p.hl.as_ref().is_some_and(covered) && p.asked != Some(ask) {
            p.asked = Some(ask);
            let (name, text) = (path.clone(), p.app.buf.text.clone());
            out.push(SimEffect::Highlight(Req {
                id: p.id,
                name,
                ver,
                text,
                from: ask.0,
                n: ask.1,
            }));
        }
        let (w, h) = (win.pane.w.max(1), win.pane.h + 1);
        let span = p.hl.as_ref().map(|h| (h.from, h.spans.len()));
        let key = (line, w, h, query.clone(), span, off);
        if p.shown.as_ref().is_some_and(|s| s.key == key) {
            return;
        }
        let buf = &mut p.app.buf;
        buf.cursor = buf.start(line.min(buf.lines().saturating_sub(1)));
        (p.app.size, p.app.center) = ((w, h), true);
        let frame = view::render(&mut p.app);
        let buf = &p.app.buf;
        let digits = buf.lines().to_string().len().max(3);
        let text = |l: usize| text_of(buf, l);
        let hl = p.hl.as_ref().filter(|_| !off);
        let spans = |l: usize| hl.and_then(|h| h.spans.get(l.checked_sub(h.from)?));
        let looks = layout::colors(&frame, digits, &text, &spans, None).looks;
        let lit = Query::new(&query)
            .filter(|_| no > 0)
            .and_then(|q| lit_of(&frame, digits, &text, line, &q));
        let title = layout::short(
            &format!("{path}:{no}"),
            usize::from(win.pane.w).saturating_sub(4).max(1),
        );
        p.shown = Some(Shown { key, title, frame, looks, lit });
    }

    /// PICK-1: the window's rows and the columns the query takes, for the tones `t` (none: calm).
    pub(super) fn live_draw(&self, win: &Win, t: Option<&Tones>) -> Option<(Vec<Cells>, usize)> {
        let l = self.panel.live.as_ref()?;
        let rows =
            l.rows(l.top, usize::from(win.list.h), usize::from(win.list.w).saturating_sub(1));
        let (n, plus) = (l.lines(), if l.capped { "+" } else { "" });
        let shown = if n == 0 { 0 } else { l.cur + 1 };
        let pane = l.preview.as_ref().and_then(|p| {
            let s = p.shown.as_ref()?;
            let (frame, looks, lit) = (&s.frame, &s.looks[..], s.lit.as_ref());
            Some(Pane { title: s.title.clone(), frame, looks, lit, note: p.note.as_deref() })
        });
        let view = View {
            title: "Live grep",
            query: &l.query,
            rows: &rows,
            none: l.done && !l.fresh && l.lines() == 0 && !l.query.is_empty(),
            count: format!("{} / {}{plus}", pick::thousands(shown), pick::thousands(n)),
            spin: l.running().then(|| SPIN[l.spin % SPIN.len()]),
            pane,
        };
        Some(pick::compose(win, &view, t))
    }
}

/// Ctrl-w: takes the last word (and the blanks after it) off a query.
fn word(q: &mut String) {
    let end = q.trim_end().len();
    q.truncate(end);
    let from = q.char_indices().rev().find(|(_, c)| c.is_whitespace());
    q.truncate(from.map_or(0, |(i, c)| i + c.len_utf8()));
}

/// PICK-4: the rows of the frame that are line `line`'s, and the columns of the chars `q` matches
/// in them, found by the walk that gives the looks (so wide chars, tabs and wraps are counted as
/// the frame has them).
fn lit_of(
    frame: &Frame,
    digits: usize,
    text: &dyn Fn(usize) -> Option<String>,
    line: usize,
    q: &Query,
) -> Option<Lit> {
    let body = frame.rows.len().saturating_sub(1);
    let first = (0..body).find(|&i| layout::number(&frame.rows[i], digits) == Some(line + 1))?;
    let stops = |i: &usize| {
        layout::number(&frame.rows[*i], digits).is_some() || frame.rows[*i].text.is_empty()
    };
    let end = (first + 1..body).find(stops).unwrap_or(body);
    let bold: Spans = q.ranges(&text(line)?).into_iter().map(|r| (r, Look::Bold)).collect();
    let marked = layout::colors(frame, digits, text, &|l| (l == line).then_some(&bold), None).looks;
    let marks = (first..end).flat_map(|i| {
        let row: &Looks = &marked[i];
        row.iter().filter(|(_, l)| *l == Look::Bold).map(move |(r, _)| (i, r.clone()))
    });
    Some(Lit { rows: first..end, marks: marks.collect() })
}
