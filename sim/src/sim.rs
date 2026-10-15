//! TREE-1, TREE-2, TREE-4, TREE-5, BUF-1, BUF-2, BUF-3
//! sim's state (pure): the open files (each one a si App), the panel, and what each event does.
//! `Sim::update` takes an event and returns what the shell is to do; it reads no clock, no file
//! and no terminal, so the tests drive it without threads.
use crate::layout;
use crate::tree::{Entry, Row as TreeRow, Tree};
use si::core::app::{App, Effect, Event, Key, Read};
use si::core::cli;
use si::core::vi::Mode;
use si::core::view::{self, Frame, Row, Shape};
use si::io::journal::Snapshot;
use std::mem::take;
use std::path::{Path, PathBuf};
use std::time::Instant;
use unicode_width::UnicodeWidthStr;

/// A file open in sim. Ids are never reused, so a late answer for a closed file is dropped.
pub type Id = usize;

#[derive(Debug)]
pub enum SimEvent {
    /// An event of one open file: a change on disk, the result of a save, a reload.
    App(Id, Event),
    /// What the terminal sent that si understands: a key, a paste, a new size.
    Term(Event),
    /// Shift-Tab, which si's `Key` does not have.
    BackTab,
    Tick(u64),
    Signal(i32),
    /// A thread has something to tell the user.
    Notice(String),
    /// A directory has been read (TREE-5).
    Listing(PathBuf, Result<Vec<Entry>, String>),
    /// The answer to `SimEffect::Open`: the file, read.
    Opened(Opened),
    /// The answer to `SimEffect::Open` when the path is a directory.
    Dir(String),
}

#[derive(Debug)]
pub enum SimEffect {
    Render(Frame),
    /// What a file's App asks for: a write, a watch, a snapshot, the clipboard, a reload.
    App(Id, Effect),
    /// Read this directory and answer with `SimEvent::Listing`.
    Read(PathBuf),
    /// Open this file (a name relative to the working directory) as file `Id`: watch it, read it,
    /// look for the snapshot a dead session left, and answer with `SimEvent::Opened`; or with
    /// `SimEvent::Dir` if it is a directory.
    Open(Id, String),
    /// The file is no longer open: stop watching it.
    Close(Id),
    /// A recovered snapshot (journal key, pid of the dead session) is ours to delete once the
    /// file's own snapshot is on disk, or when the file is saved or closed (SAFE-6).
    Hold(Id, String, u32),
    Quit(i32),
}

/// A snapshot that a dead session left (SAFE-6), the file that holds it, its age in ms and the
/// instant its merge gives up at: what `Event::Recover` takes.
#[derive(Debug)]
pub struct Found(pub Snapshot, pub String, pub u64, pub Instant);

#[derive(Debug)]
pub struct Opened {
    pub id: Id,
    pub path: String,
    /// The journal key (SAFE-6): two names of one file have the same.
    pub key: String,
    pub file: Read,
    pub found: Option<Found>,
}

pub struct Buf {
    pub id: Id,
    pub key: String,
    pub app: App,
    /// The App's last frame, valid for the size it has now.
    frame: Option<Frame>,
}

#[derive(Default)]
pub struct Panel {
    pub open: bool,
    pub focus: bool,
    /// The cursor row and the first row on screen.
    pub cur: usize,
    pub top: usize,
    /// A `g` waits for a second one.
    g: bool,
    /// A directory (below the root) to expand, step by step, as its listings arrive.
    reveal: Option<PathBuf>,
}

#[derive(Default)]
pub struct Sim {
    pub bufs: Vec<Buf>,
    pub cur: usize,
    pub tree: Tree,
    pub panel: Panel,
    pub root: PathBuf,
    cwd: PathBuf,
    size: (u16, u16),
    next: Id,
    /// The keys of `+cmd` and `file:line`, typed into the first file once it is open.
    keys: String,
    msg: Option<String>,
}

/// What `sim` was asked to open (TREE-1).
pub struct Start {
    pub root: PathBuf,
    pub file: Option<String>,
    pub keys: String,
}

/// TREE-1: `sim`, `sim {dir}`, `sim {file}`, with si's rules for the file. `None` is a usage error.
pub fn start(
    args: &[String],
    cwd: &Path,
    exists: impl Fn(&str) -> bool,
    is_dir: impl Fn(&Path) -> bool,
) -> Option<Start> {
    let cli = cli::parse(args, exists)?;
    let dir = cli.file.as_deref().map(|f| cwd.join(f)).filter(|d| is_dir(d));
    Some(match dir {
        Some(root) => Start { root, file: None, keys: String::new() },
        None => Start { root: cwd.into(), file: cli.file, keys: cli.keys },
    })
}

/// The commands for all the files at once, which si's App does not know (BUF-3), and `:e name`,
/// which opens another file instead of replacing this one (BUF-1): name, `!`, argument.
fn all(line: &str) -> Option<(&str, bool, &str)> {
    let s = line.strip_prefix(':')?.trim();
    let (name, rest) = s.split_at(s.find(|c: char| !c.is_ascii_alphabetic()).unwrap_or(s.len()));
    let (bang, arg) = rest.strip_prefix('!').map_or((false, rest), |a| (true, a));
    let ok = matches!(
        (name, arg.trim().is_empty()),
        ("qa" | "qall" | "wa" | "wall", true) | ("e" | "edit", false)
    );
    ok.then_some((name, bang, arg.trim()))
}

impl Sim {
    pub fn new(s: Start, cwd: PathBuf, size: (u16, u16)) -> (Sim, Vec<SimEffect>) {
        let panel = Panel { open: s.file.is_none(), focus: s.file.is_none(), ..Panel::default() };
        let (root, keys, size) = (s.root, s.keys, (size.0.max(1), size.1.max(2)));
        let mut sim = Sim { panel, cwd, size, keys, root, ..Sim::default() };
        let mut out = vec![SimEffect::Read(sim.root.clone())];
        if let Some(f) = s.file {
            sim.request(f, &mut out);
        }
        out.push(SimEffect::Render(sim.frame()));
        (sim, out)
    }

    /// The columns of the panel (0 when it is closed or the screen is too narrow for it) and of
    /// the editor: the panel is a third of the screen, at most 32, and a column of space follows.
    pub fn widths(&self) -> (u16, u16) {
        let w = self.size.0;
        let pw = if self.panel.open { (w / 3).min(32) } else { 0 };
        if pw < 4 { (0, w) } else { (pw, w - pw - 1) }
    }

    pub fn update(&mut self, ev: SimEvent) -> Vec<SimEffect> {
        let mut out = vec![];
        self.fit();
        match ev {
            SimEvent::Term(Event::Key(k)) => self.key(k, &mut out),
            SimEvent::Term(Event::Paste(s)) if !self.panel.focus => {
                self.to(self.id(), Event::Paste(s), &mut out)
            }
            SimEvent::Term(Event::Resize(w, h)) => self.size = (w.max(1), h.max(2)),
            SimEvent::Term(_) => {}
            SimEvent::BackTab => self.step(self.bufs.len().saturating_sub(1)),
            SimEvent::App(id, e) => self.to(id, e, &mut out),
            SimEvent::Notice(m) => self.msg = Some(m),
            SimEvent::Tick(n) => return self.snapshots(None, || Event::Tick(n)),
            SimEvent::Signal(n) => {
                let mut out = self.snapshots(None, || Event::Signal(n));
                out.push(SimEffect::Quit(128 + n));
                return out;
            }
            SimEvent::Listing(dir, r) => self.listing(&dir, r, &mut out),
            SimEvent::Opened(o) => self.opened(o, &mut out),
            SimEvent::Dir(path) => self.reveal_dir(&path, &mut out),
        }
        out.push(SimEffect::Render(self.frame()));
        out
    }

    /// Tells every App the size it has, so that what it renders fits next to the panel.
    fn fit(&mut self) {
        let (_, ew) = self.widths();
        self.bufs.iter_mut().for_each(|b| b.app.size = (ew, self.size.1));
    }

    /// The id of the current file (none: 0, which no `to` finds).
    fn id(&self) -> Id {
        self.bufs.get(self.cur).map_or(0, |b| b.id)
    }

    /// Normal mode with nothing pending, in a file: the state in which Tab is ours (BUF-2).
    fn idle(&self) -> bool {
        self.bufs.get(self.cur).is_some_and(|b| b.app.vi.idle())
    }

    /// Shows the file `by` places on, wrapping round (Tab is 1, Shift-Tab one less than all).
    fn step(&mut self, by: usize) {
        if self.idle() {
            self.cur = (self.cur + by) % self.bufs.len();
        }
    }

    /// Gives an event to one file's App, and runs what it asks for.
    fn to(&mut self, id: Id, ev: Event, out: &mut Vec<SimEffect>) {
        let Some(b) = self.bufs.iter_mut().find(|b| b.id == id) else { return };
        let fx = b.app.update(ev);
        self.run(id, fx, out);
    }

    /// Takes the frames and the quits out of an App's effects; the shell gets the rest.
    fn run(&mut self, id: Id, fx: Vec<Effect>, out: &mut Vec<SimEffect>) {
        let (mut quit, mut frame) = (None, None);
        for e in fx {
            match e {
                Effect::Render(f) => frame = Some(f),
                Effect::Quit(code) => quit = Some(code),
                e => out.push(SimEffect::App(id, e)),
            }
        }
        if let Some(b) = self.bufs.iter_mut().find(|b| b.id == id) {
            b.frame = frame.or(b.frame.take());
        }
        match quit {
            Some(0) => self.close(id, out),
            // `:cq`: the whole of sim ends, and the other files keep what is unsaved in them.
            Some(code) => {
                out.extend(self.snapshots(Some(id), || Event::Signal(0)));
                out.push(SimEffect::Quit(code));
            }
            None => {}
        }
    }

    /// BUF-3: closes a file: the one after it shows, the one before if it was the last; sim ends
    /// with the last one. The App has asked the journal to forget its snapshot already.
    fn close(&mut self, id: Id, out: &mut Vec<SimEffect>) {
        let Some(i) = self.bufs.iter().position(|b| b.id == id) else { return };
        self.bufs.remove(i);
        out.push(SimEffect::Close(id));
        self.cur = (self.cur - usize::from(i < self.cur)).min(self.bufs.len().saturating_sub(1));
        if self.bufs.is_empty() {
            out.push(SimEffect::Quit(0));
        }
    }

    /// Gives every App but `except` an event that makes it ask for a snapshot of its unsaved text
    /// (a tick, a signal: SAFE-6), and keeps only those requests.
    fn snapshots(&mut self, except: Option<Id>, ev: impl Fn() -> Event) -> Vec<SimEffect> {
        let mut out = vec![];
        for b in self.bufs.iter_mut().filter(|b| Some(b.id) != except) {
            let fx =
                b.app.update(ev()).into_iter().filter(|e| matches!(e, Effect::Snapshot { .. }));
            out.extend(fx.map(|e| SimEffect::App(b.id, e)));
        }
        out
    }

    fn key(&mut self, k: Key, out: &mut Vec<SimEffect>) {
        self.msg = None;
        match k {
            Key::Ctrl('n') => return self.toggle(out),
            Key::Tab if self.idle() => return self.step(1),
            _ if self.panel.open && self.panel.focus => return self.panel_key(k, out),
            _ => {}
        }
        let Some(b) = self.bufs.get(self.cur) else {
            return out.extend((k == Key::Char('q')).then_some(SimEffect::Quit(0)));
        };
        let submit = k == Key::Enter && b.app.vi.mode == Mode::Cmd;
        let (id, line) = (b.id, if submit { b.app.vi.line.clone() } else { String::new() });
        if let Some(cmd) = all(&line) {
            self.to(id, Event::Key(Key::Esc), out); // the command line is ours: close it
            return self.run_all(cmd, out);
        }
        self.to(id, Event::Key(k), out);
    }

    fn run_all(&mut self, (name, bang, arg): (&str, bool, &str), out: &mut Vec<SimEffect>) {
        let (ids, names): (Vec<Id>, Vec<String>) = (self.bufs.iter())
            .filter(|b| b.app.buf.dirty())
            .map(|b| (b.id, b.app.path.clone().unwrap_or_default()))
            .unzip();
        match name {
            "qa" | "qall" if bang || ids.is_empty() => {
                out.extend(self.bufs.iter().map(|b| SimEffect::App(b.id, Effect::Unsnapshot)));
                out.push(SimEffect::Quit(0));
            }
            "qa" | "qall" => {
                self.msg = Some(format!(
                    "no write since last change in {} (:qa! to discard)",
                    names.join(", ")
                ))
            }
            "wa" | "wall" if ids.is_empty() => self.msg = Some("no modified files".into()),
            "wa" | "wall" => {
                ids.into_iter().for_each(|id| self.to(id, Event::Key(Key::Ctrl('s')), out))
            }
            _ => self.open(self.cwd.join(arg), out),
        }
    }

    /// BUF-1: shows the file if it is open, else asks for it to be read.
    fn open(&mut self, path: PathBuf, out: &mut Vec<SimEffect>) {
        let same = |b: &Buf| b.app.path.as_ref().is_some_and(|p| self.cwd.join(p) == path);
        if let Some(i) = self.bufs.iter().position(same) {
            (self.cur, self.panel.focus) = (i, false);
        } else {
            self.request(
                path.strip_prefix(&self.cwd).unwrap_or(&path).to_string_lossy().into_owned(),
                out,
            );
        }
    }

    fn request(&mut self, name: String, out: &mut Vec<SimEffect>) {
        out.push(SimEffect::Open(self.next, name));
        self.next += 1;
    }

    fn opened(&mut self, o: Opened, out: &mut Vec<SimEffect>) {
        self.panel.focus = false;
        if let Some(i) = self.bufs.iter().position(|b| b.key == o.key) {
            self.cur = i; // another name of a file that is open: one buffer, one snapshot
            return out.push(SimEffect::Close(o.id));
        }
        let mut app = App::new(Some(o.path), o.file, (self.widths().1, self.size.1));
        let mut fx = vec![];
        if let Some(Found(s, file, age, by)) = o.found {
            let v = app.buf.version();
            fx = app.update(Event::Recover(s.base, s.text, age, file, by));
            if app.buf.version() != v {
                out.push(SimEffect::Hold(o.id, s.path, s.pid));
            }
        }
        fx.extend(app.start(&take(&mut self.keys)));
        self.bufs.push(Buf { id: o.id, key: o.key, app, frame: None });
        self.cur = self.bufs.len() - 1;
        self.run(o.id, fx, out);
    }

    // ---- the panel ----

    /// TREE-2: Ctrl-n opens the panel and focuses it, or closes it when it has the focus. An
    /// opening reads the directories again, so that what changed meanwhile shows.
    fn toggle(&mut self, out: &mut Vec<SimEffect>) {
        let close = self.panel.open && self.panel.focus;
        (self.panel.open, self.panel.focus) = (!close, !close);
        if !close {
            let rows = self.tree.rows(0, self.tree.visible());
            let open =
                rows.into_iter().filter(|r| r.is_dir && r.open).map(|r| self.root.join(r.path));
            out.extend(std::iter::once(self.root.clone()).chain(open).map(SimEffect::Read));
        }
    }

    /// The row of the panel at `row`, if there is one.
    fn row(&self, row: usize) -> Option<TreeRow> {
        self.tree.rows(row, 1).pop()
    }

    /// TREE-4: a key while the panel has the focus.
    fn panel_key(&mut self, k: Key, out: &mut Vec<SimEffect>) {
        let last = self.tree.visible().saturating_sub(1);
        let page = usize::from(self.size.1) - 1;
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
            Key::Char('-') => {
                let top = self.row(cur).and_then(|r| r.path.iter().next().map(PathBuf::from));
                self.tree.collapse_all();
                top.and_then(|p| self.tree.row_of(&p)).unwrap_or(0)
            }
            Key::Char('q') => return (self.panel.open, self.panel.focus) = (false, false),
            Key::Esc => return self.panel.focus = false,
            _ => cur,
        };
        self.panel.cur = to.min(last);
    }

    /// `l`, Right and Enter: expand a directory, open a file.
    fn enter(&mut self, out: &mut Vec<SimEffect>) {
        let Some(r) = self.row(self.panel.cur) else { return };
        let abs = self.root.join(&r.path);
        if !r.is_dir {
            self.open(abs, out);
        } else if !r.open && self.tree.expand(&r.path) {
            out.push(SimEffect::Read(abs));
        }
    }

    /// `h` and Left: collapse an open directory, else go to the directory above.
    fn back(&mut self) {
        let Some(r) = self.row(self.panel.cur) else { return };
        if r.is_dir && r.open {
            self.tree.collapse(&r.path);
        } else if let Some(up) = self.tree.parent_row(self.panel.cur) {
            self.panel.cur = up;
        }
    }

    /// TREE-5: a directory has been read. The cursor and the first row stay on their entries
    /// whatever came in above them.
    fn listing(&mut self, dir: &Path, r: Result<Vec<Entry>, String>, out: &mut Vec<SimEffect>) {
        let Ok(rel) = dir.strip_prefix(&self.root) else { return };
        let entries = r.unwrap_or_else(|e| {
            self.msg = Some(e);
            vec![]
        });
        let (cur, top) = (self.row(self.panel.cur), self.row(self.panel.top));
        self.tree.insert_listing(rel, entries);
        let find =
            |r: Option<TreeRow>, old| r.and_then(|r| self.tree.row_of(&r.path)).unwrap_or(old);
        (self.panel.cur, self.panel.top) = (find(cur, self.panel.cur), find(top, self.panel.top));
        self.reveal(out);
    }

    /// `:e dir` (BUF-1): the panel opens with that directory expanded and under the cursor.
    fn reveal_dir(&mut self, path: &str, out: &mut Vec<SimEffect>) {
        let abs = self.cwd.join(path);
        let Ok(rel) = abs.strip_prefix(&self.root) else {
            return self.msg =
                Some(format!("{} is not under {}", abs.display(), self.root.display()));
        };
        self.panel.reveal = Some(rel.to_path_buf());
        (self.panel.open, self.panel.focus) = (true, true);
        self.reveal(out);
    }

    /// Expands the directories on the way to the one to reveal, as far as their listings are
    /// there; the rest follows as they arrive.
    fn reveal(&mut self, out: &mut Vec<SimEffect>) {
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
    fn scroll(&mut self) {
        let (rows, n) = (usize::from(self.size.1) - 1, self.tree.visible());
        let p = &mut self.panel;
        p.cur = p.cur.min(n.saturating_sub(1));
        p.top = p.top.min(n.saturating_sub(rows)).clamp((p.cur + 1).saturating_sub(rows), p.cur);
    }

    // ---- the frame ----

    /// The whole screen: the panel, the current file and the status line (BUF-2: `2/5` is
    /// appended when more than one file is open).
    fn frame(&mut self) -> Frame {
        self.fit();
        self.scroll();
        let ((w, h), (pw, ew)) = (self.size, self.widths());
        let (off, focus) = (if pw > 0 { pw + 1 } else { 0 }, self.panel.open && self.panel.focus);
        let rows = if pw > 0 { self.tree.rows(self.panel.top, usize::from(h) - 1) } else { vec![] };
        let at = self.row(self.panel.cur);
        let dir = |r: &TreeRow| if r.is_dir { "/" } else { "" };
        let root = self.root.display().to_string();
        let tree = at.as_ref().map_or(root.clone(), |r| format!("{}{}", r.path.display(), dir(r)));
        let (mut status, mut cursor, mut shape) = (root, (off, 0), Shape::Block);
        let mut cmd = false;
        if let Some(b) = self.bufs.get_mut(self.cur) {
            if b.frame.as_ref().is_some_and(|f| (f.width, f.height) != (ew, h)) {
                b.frame = None;
            }
            let f = b.frame.get_or_insert_with(|| view::render(&mut b.app));
            (cmd, shape, cursor) =
                (b.app.vi.mode == Mode::Cmd && !focus, f.shape, (f.cursor.0 + off, f.cursor.1));
            status = f.rows.last().map_or(String::new(), |r| r.text.clone());
            // The App cuts its status line to its own width; the screen has more.
            if pw > 0 && (cmd || status.width() + 2 >= usize::from(ew)) {
                let (top, center, size) = (b.app.top, b.app.center, b.app.size);
                b.app.size.0 = w;
                let wide = view::render(&mut b.app);
                (b.app.top, b.app.center, b.app.size) = (top, center, size);
                status = wide.rows.last().map_or(String::new(), |r| r.text.clone());
                cursor = if cmd { wide.cursor } else { cursor };
            }
            if focus && b.app.msg.is_none() {
                status = format!("{tree}  TREE");
            }
        } else if focus {
            status = format!("{tree}  TREE");
        }
        status = self.msg.clone().unwrap_or(status);
        if self.bufs.len() > 1 && !cmd {
            status += &format!("  {}/{}", self.cur + 1, self.bufs.len());
        }
        let sel = focus.then(|| self.panel.cur - self.panel.top);
        if let Some(i) = sel {
            let col = at.map_or(0, |r| (2 * r.depth + 2) as u16);
            (cursor, shape) = ((col.min(pw.saturating_sub(1)), i as u16), Shape::Block);
        }
        let editor = self.bufs.get(self.cur).and_then(|b| b.frame.as_ref());
        let mut rows = layout::body(w, h, pw, &rows, sel, editor);
        rows.push(Row { text: layout::fit(&status, usize::from(w)).0, ..Row::default() });
        Frame { width: w, height: h, rows, cursor, shape }
    }
}
