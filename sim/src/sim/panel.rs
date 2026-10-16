//! TREE-2, TREE-4, TREE-5, FIND-1, FIND-2, FIND-4, MARK-3
//! The panel's side of sim's state (pure): its keys, the directories it reveals as their listings
//! arrive, the search that lights up matches, and the narrowed tree.
use super::{Sim, SimEffect};
use crate::find::{Found, Query};
use crate::layout;
use crate::tree::{self, Entry, Row as TreeRow, Tree};
use si::core::app::Key;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::mem::take;
use std::path::{Path, PathBuf};

/// MARK-3: a file opened as often as one opened once a day ago (likelihood 1/sqrt(86,400)) counts
/// as recent: the files of today's work.
const RECENT: f64 = 0.0034;

/// FIND-1: a search. `open` and `at` are what the tree looked like before it (the directories
/// that were open, the entry under the cursor), for Esc to restore.
#[derive(Default)]
pub(super) struct Search {
    pub query: String,
    /// The query is being typed; else the matches stay lit for `n` and `N`.
    pub typing: bool,
    pub found: Found,
    /// The best match: where the cursor goes and what Enter opens.
    target: Option<String>,
    open: HashSet<PathBuf>,
    at: PathBuf,
}

pub type Out<'a> = &'a mut Vec<SimEffect>;

/// The match that comes last (`back`) or first of these in the order of the rows.
fn pick<'a>(it: impl Iterator<Item = &'a str>, back: bool) -> Option<&'a str> {
    let cmp = |a: &&str, b: &&str| tree::cmp_paths(a, false, b, false);
    if back { it.max_by(cmp) } else { it.min_by(cmp) }
}

impl Sim {
    /// The tree the panel shows: the narrowed one (MARK-3), or the whole.
    pub(super) fn view(&self) -> &Tree {
        self.focus.as_ref().unwrap_or(&self.tree)
    }

    fn view_mut(&mut self) -> &mut Tree {
        self.focus.as_mut().unwrap_or(&mut self.tree)
    }

    /// The row of the panel at `row`, if there is one.
    pub(super) fn row(&self, row: usize) -> Option<TreeRow> {
        self.view().rows(row, 1).pop()
    }

    /// Puts the cursor on `path`, or on the nearest row above it that is shown.
    fn place(&mut self, path: &Path) {
        let found = path
            .ancestors()
            .filter(|a| !a.as_os_str().is_empty())
            .find_map(|p| self.view().row_of(p));
        self.panel.cur = found.unwrap_or(0);
    }

    /// Opens the panel with the focus, and reads again what may have changed meanwhile: the
    /// directories that are open (TREE-2), the list of files (FIND-3) and git's status (MARK-1).
    pub(super) fn show(&mut self, out: Out) {
        (self.panel.open, self.panel.focus, self.no_repo) = (true, true, false);
        let rows = self.tree.rows(0, self.tree.visible());
        let open = rows.into_iter().filter(|r| r.is_dir && r.open).map(|r| self.root.join(r.path));
        out.extend(std::iter::once(self.root.clone()).chain(open).map(SimEffect::Read));
        out.push(SimEffect::Files);
        self.git(out);
    }

    /// TREE-2: Ctrl-n opens the panel and focuses it, or closes it when it has the focus.
    pub(super) fn toggle(&mut self, out: Out) {
        self.live_end(false, out);
        if self.panel.open && self.panel.focus {
            self.end_search();
            (self.panel.open, self.panel.focus) = (false, false);
        } else {
            self.show(out);
        }
    }

    /// TREE-4: a key while the panel has the focus.
    pub(super) fn panel_key(&mut self, k: Key, out: Out) {
        if self.panel.search.as_ref().is_some_and(|s| s.typing) {
            return self.search_key(k, out);
        }
        self.panel.reveal = None; // a key of the user's own cancels a reveal still waiting for a listing
        let browsing = self.panel.search.is_some();
        let last = self.view().visible().saturating_sub(1);
        let page = usize::from(self.geo().rows);
        let (cur, g) = (self.panel.cur, take(&mut self.panel.g));
        let to = match k {
            Key::Char('j') | Key::Down => cur + 1,
            Key::Char('k') | Key::Up => cur.saturating_sub(1),
            Key::Ctrl('d') => cur + page / 2,
            Key::Ctrl('u') => cur.saturating_sub(page / 2),
            Key::PageDown => cur + page,
            Key::PageUp => cur.saturating_sub(page),
            Key::Char('g') if g => 0,
            Key::Char('g') => {
                self.panel.g = true;
                cur
            }
            Key::Char('G') => last,
            Key::Char('l') | Key::Right | Key::Enter => return self.enter(out),
            Key::Char('h') | Key::Left => return self.back(),
            Key::Char('/') => return self.find(out),
            Key::Char('n' | 'N') if browsing => return self.step_match(k == Key::Char('N'), out),
            Key::Char('.') => return self.focus_toggle(out),
            Key::Char('-') => {
                let top = self.row(cur).and_then(|r| r.path.iter().next().map(PathBuf::from));
                self.view_mut().collapse_all();
                top.and_then(|p| self.view().row_of(&p)).unwrap_or(0)
            }
            Key::Char('q') => {
                self.end_search();
                return (self.panel.open, self.panel.focus) = (false, false);
            }
            Key::Esc if browsing => return self.end_search(),
            Key::Esc => return self.panel.focus = false,
            _ => cur,
        };
        self.panel.cur = to.min(last);
    }

    /// `l`, Right and Enter: expand a directory, open a file (which ends a search, so that the
    /// tree shows the file in its place and nothing else of the search).
    pub(super) fn enter(&mut self, out: Out) {
        let Some(r) = self.row(self.panel.cur) else { return };
        let abs = self.root.join(&r.path);
        if !r.is_dir {
            self.end_search_at(r.path, out);
            self.open(abs, out);
        } else if !r.open && self.view_mut().expand(&r.path) {
            out.push(SimEffect::Read(abs));
        }
    }

    /// `h` and Left: collapse an open directory, else go to the directory above.
    pub(super) fn back(&mut self) {
        let Some(r) = self.row(self.panel.cur) else { return };
        if r.is_dir && r.open {
            self.view_mut().collapse(&r.path);
        } else if let Some(up) = self.view().parent_row(self.panel.cur) {
            self.panel.cur = up;
        }
    }

    /// TREE-5: a directory has been read. The cursor and the first row stay on their entries
    /// whatever came in above them.
    pub(super) fn listing(&mut self, dir: &Path, r: Result<Vec<Entry>, String>, out: Out) {
        let Ok(rel) = dir.strip_prefix(&self.root) else { return };
        let entries = r.unwrap_or_else(|e| {
            let name = if rel.as_os_str().is_empty() { &self.root } else { rel };
            self.msg =
                Some(format!("cannot read {}: {e}", layout::short(&name.to_string_lossy(), 32)));
            vec![]
        });
        let kept =
            self.focus.is_none().then(|| (self.row(self.panel.cur), self.row(self.panel.top)));
        self.tree.insert_listing(rel, entries);
        if let Some((cur, top)) = kept {
            let find =
                |r: Option<TreeRow>, old| r.and_then(|r| self.tree.row_of(&r.path)).unwrap_or(old);
            (self.panel.cur, self.panel.top) =
                (find(cur, self.panel.cur), find(top, self.panel.top));
        }
        self.reveal(out);
    }

    /// `:e dir` (BUF-1): the panel opens with that directory expanded and under the cursor.
    pub(super) fn reveal_dir(&mut self, path: &str, out: Out) {
        let abs = self.cwd.join(path);
        let Ok(rel) = abs.strip_prefix(&self.root) else {
            let (root, abs) = (self.root.to_string_lossy(), abs.to_string_lossy());
            return self.msg = Some(format!(
                "not under {}: {}",
                layout::short(&root, 28),
                layout::short(&abs, 34)
            ));
        };
        (self.panel.reveal, self.focus) = (Some(rel.to_path_buf()), None);
        self.end_search();
        (self.panel.open, self.panel.focus) = (true, true);
        self.reveal(out);
    }

    /// Expands the directories on the way to the path to reveal, as far as their listings are
    /// there; the rest follows as they arrive. The cursor goes there at the end.
    pub(super) fn reveal(&mut self, out: Out) {
        let Some(target) = self.panel.reveal.clone() else { return };
        let mut path = PathBuf::new();
        for c in &target {
            path.push(c);
            if self.tree.row_of(&path).is_none() {
                return self.panel.reveal = None; // not there
            }
            if self.tree.expand(&path) {
                return out.push(SimEffect::Read(self.root.join(&path)));
            }
        }
        self.panel.reveal = None;
        self.panel.cur = self.tree.row_of(&target).unwrap_or(self.panel.cur);
    }

    /// Keeps the cursor row on screen, and the first row where the tree has rows for it.
    pub(super) fn scroll(&mut self) {
        let (rows, n) = (usize::from(self.geo().rows), self.view().visible());
        let p = &mut self.panel;
        p.cur = p.cur.min(n.saturating_sub(1));
        p.top = p.top.min(n.saturating_sub(rows)).clamp((p.cur + 1).saturating_sub(rows), p.cur);
    }

    // ---- FIND-1, FIND-4: the search ----

    /// `/` in the panel and Ctrl-p anywhere (FIND-4): the panel opens, and a search starts.
    pub(super) fn find(&mut self, out: Out) {
        self.live_end(false, out);
        self.end_search();
        self.focus = None;
        if !self.panel.open {
            self.show(out);
        }
        self.panel.focus = true;
        let at = self.row(self.panel.cur).map(|r| r.path).unwrap_or_default();
        let open = self.tree.open_dirs();
        self.panel.search = Some(Search { typing: true, open, at, ..Search::default() });
    }

    /// Esc: the search ends, the expansion and the cursor are what they were before it.
    pub(super) fn end_search(&mut self) {
        let Some(s) = self.panel.search.take() else { return };
        self.panel.reveal = None;
        self.tree.restore(&s.open);
        self.place(&s.at);
    }

    /// Enter ends the search on a file that is opened: the expansion is put back to what it was
    /// before the search, then only the directories of that file are opened (as `:e` reveals
    /// one), so the tree shows where the file is and nothing the cursor passed on its way.
    fn end_search_at(&mut self, path: PathBuf, out: Out) {
        let Some(s) = self.panel.search.take() else { return };
        self.tree.restore(&s.open);
        self.panel.reveal = Some(path);
        self.reveal(out);
    }

    /// FIND-1, FIND-2: finds the matches of the query among the files and takes the cursor to
    /// the best, expanding the directories on its way.
    pub(super) fn search_run(&mut self, out: Out) {
        let Some(s) = self.panel.search.as_mut() else { return };
        let found = Query::new(&s.query).search(&self.files);
        // The likelihoods are the history's (at most 1,000 paths), by their place in the list.
        let history = self.history.top(&self.root.to_string_lossy(), usize::MAX, self.now);
        let place = |(p, l): (String, f64)| Some((self.files.binary_search(&p).ok()?, l));
        let liked: Vec<(usize, f64)> = history.into_iter().filter_map(place).collect();
        let best = found.best(&self.files, &liked);
        let target = best.map(|hit| self.files[hit.index].clone());
        (s.found, s.target) = (found, target.clone());
        if let Some(t) = target {
            self.panel.reveal = Some(t.into());
            self.reveal(out);
        }
    }

    /// A key while the query is being typed.
    fn search_key(&mut self, k: Key, out: Out) {
        let Some(s) = self.panel.search.as_mut() else { return };
        match k {
            Key::Char(c) => s.query.push(c),
            Key::Backspace if s.query.is_empty() => return self.end_search(),
            Key::Backspace => drop(s.query.pop()),
            Key::Esc => return self.end_search(),
            Key::Enter => {
                let Some(t) = s.target.clone() else { return };
                self.end_search_at(PathBuf::from(&t), out);
                return self.open(self.root.join(t), out);
            }
            Key::Down | Key::Up => {
                s.typing = false;
                return self.step_match(k == Key::Up, out);
            }
            _ => return,
        }
        self.search_run(out);
    }

    /// `n` and `N`: the cursor goes to the next or the previous match in the order the rows are
    /// on screen, and round to the first or the last. The work is a pass over the matches.
    fn step_match(&mut self, back: bool, out: Out) {
        let Some(s) = &self.panel.search else { return };
        let (cp, cd) = self
            .row(self.panel.cur)
            .map_or((String::new(), false), |r| (r.path.to_string_lossy().into_owned(), r.is_dir));
        let paths = || s.found.hits.iter().map(|h| self.files[h.index].as_str());
        let want = if back { Ordering::Less } else { Ordering::Greater };
        let beyond = paths().filter(|p| tree::cmp_paths(p, false, &cp, cd) == want);
        let Some(t) = pick(beyond, back).or_else(|| pick(paths(), back)).map(PathBuf::from) else {
            return;
        };
        self.panel.reveal = Some(t);
        self.reveal(out);
    }

    // ---- MARK-3: the narrowed tree ----

    /// `.`: narrows the tree to the files that changed, the open ones and the recent ones, with
    /// their directories; or shows everything again, with the entry under the cursor revealed.
    fn focus_toggle(&mut self, out: Out) {
        self.end_search();
        let at = self.row(self.panel.cur).map(|r| r.path).unwrap_or_default();
        if self.focus.take().is_some() {
            (self.panel.cur, self.panel.reveal) = (0, Some(at));
            return self.reveal(out);
        }
        self.focus = Some(Tree::default());
        self.refocus();
        self.place(&at);
    }

    /// Makes the narrowed tree again (if the tree is narrowed) after something it depends on
    /// changed: the marks, the open files, the history. It is a Tree like the other, made of
    /// listings that hold only these files and the directories above them, all open.
    pub(super) fn refocus(&mut self) {
        if self.focus.is_none() {
            return;
        }
        let open = self.bufs.iter().filter_map(|b| b.app.path.as_ref()).map(|p| self.cwd.join(p));
        let open = open.filter_map(|p| p.strip_prefix(&self.root).ok().map(Path::to_path_buf));
        let recent = self.history.top(&self.root.to_string_lossy(), 1000, self.now);
        let recent = recent.into_iter().filter(|r| r.1 >= RECENT).map(|r| PathBuf::from(r.0));
        let (mut kids, mut seen) = (HashMap::<PathBuf, Vec<Entry>>::new(), HashSet::new());
        for file in self.marks.files().cloned().chain(open).chain(recent) {
            let above = file.ancestors().enumerate().take_while(|(_, p)| !p.as_os_str().is_empty());
            for (i, p) in above.filter(|(_, p)| seen.insert(p.to_path_buf())) {
                let name = p.file_name().unwrap_or_default().to_string_lossy().into_owned();
                let dir = p.parent().unwrap_or(Path::new("")).to_path_buf();
                kids.entry(dir).or_default().push(Entry { name, is_dir: i > 0 });
            }
        }
        let mut narrowed = Tree::default();
        let mut dirs: Vec<PathBuf> = kids.keys().cloned().collect();
        dirs.sort_by_key(|d| d.components().count()); // the ones above first
        for dir in dirs {
            narrowed.insert_listing(&dir, kids.remove(&dir).unwrap_or_default());
            narrowed.expand(&dir);
        }
        self.focus = Some(narrowed);
    }
}
