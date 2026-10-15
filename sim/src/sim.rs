//! TREE-1, TREE-2, TREE-4, TREE-5, BUF-1, BUF-2, BUF-3, FIND-3, MARK-1, MARK-2, SYN-1, SYN-2, SYN-3, SYN-5
//! sim's state (pure): the open files (each one a si App), the panel, and what each event does.
//! `Sim::update` takes an event and returns what the shell is to do; it reads no clock, no file
//! and no terminal, so the tests drive it without threads. The panel's keys, search and focus
//! are in `panel.rs`.
mod panel;

use crate::color::Palette;
use crate::io::GIT_SKIPPED;
use crate::layout::{self, Mark};
use crate::marks::Marks;
use crate::paint::{Looks, Screen};
use crate::rank::History;
use crate::syn::Spans;
use crate::tree::{Entry, Row as TreeRow, Tree};
use panel::Search;
use ropey::Rope;
use si::core::app::{App, Effect, Event, Key, Read};
use si::core::cli;
use si::core::vi::Mode;
use si::core::view::{self, Frame, Row, Shape};
use si::io::journal::Snapshot;
use std::collections::HashSet;
use std::mem::{replace, take};
use std::path::{Path, PathBuf};
use std::time::Instant;
use unicode_width::UnicodeWidthStr;

/// A file open in sim. Ids are never reused, so a late answer for a closed file is dropped.
pub type Id = usize;

/// SYN-3: files over this many bytes are not highlighted.
pub const MAX_HL: usize = 10_000_000;

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
    /// FIND-3: the files under the root, sorted, relative to it.
    Files(Vec<String>),
    /// MARK-1: git's status, and where the root lies below the top of the repository.
    Status(Result<(Vec<u8>, String), String>),
    /// SYN-1: the payload of a reply of the terminal to OSC 4, 10 or 11.
    Osc(String),
    /// SYN-3: the spans of a window of a file.
    Spans(Hl),
}

#[derive(Debug)]
pub enum SimEffect {
    Render(Screen),
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
    /// List the files under the root and answer with `SimEvent::Files` (FIND-3). The shell runs
    /// it so that it never runs twice at once: a request while it runs is one more run after it.
    Files,
    /// Ask git for the status of the root and answer with `SimEvent::Status` (MARK-1); likewise.
    Status,
    /// Save the history of opened files (MARK-2).
    History(History),
    /// Highlight a window of a file and answer with `SimEvent::Spans` (SYN-3).
    Highlight(Req),
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

/// SYN-3: lines `from..from + n` of the file `id` named `name`, as of version `ver` of `text`.
#[derive(Debug)]
pub struct Req {
    pub id: Id,
    pub name: String,
    pub ver: u64,
    pub text: Rope,
    pub from: usize,
    pub n: usize,
}

/// SYN-3: the spans of lines `from..` of a file at version `ver`, which had `lines` lines.
#[derive(Debug)]
pub struct Hl {
    pub id: Id,
    pub ver: u64,
    pub lines: usize,
    pub from: usize,
    pub spans: Vec<Spans>,
}

pub struct Buf {
    pub id: Id,
    pub key: String,
    pub app: App,
    /// The App's last frame, valid for the size it has now.
    frame: Option<Frame>,
    /// The latest spans, and what was last asked for (version, first and last line on screen).
    hl: Option<Hl>,
    asked: Option<(u64, usize, usize)>,
    /// Where the command line scrolls to on the full-width status line (VI-12): the App's own
    /// `vi.off` is that of its narrower frame, which every one of its updates renders.
    wide: usize,
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
    /// A path (below the root) to show: its directories are expanded, step by step, as their
    /// listings arrive, and the cursor goes there.
    reveal: Option<PathBuf>,
    /// FIND-1: the search that runs, if any.
    search: Option<Search>,
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
    /// FIND-3: the files under the root, sorted.
    files: Vec<String>,
    /// MARK-1: git's status (and the prefix), the paths seen changing on disk, and the marks.
    status: (Vec<u8>, String),
    disk: HashSet<PathBuf>,
    marks: Marks,
    no_repo: bool,
    /// MARK-1: git was not run because the repository's config could run commands (told once).
    skipped: bool,
    /// MARK-2: the history, and the time in seconds (at the start, then the shell's ticks).
    history: History,
    now: u64,
    t0: u64,
    /// MARK-3: the rows of the narrowed tree, when it is narrowed.
    focus: Option<Tree>,
    /// SYN-1, SYN-2, SYN-5: the terminal's palette, whether colors are on, and a `z` just typed.
    pal: Palette,
    hl_off: bool,
    z: bool,
}

/// What `sim` starts with besides what it was asked to open: the shell's side of the world.
#[derive(Default)]
pub struct Env {
    pub cwd: PathBuf,
    pub size: (u16, u16),
    /// Seconds since the epoch.
    pub now: u64,
    pub history: History,
    pub pal: Palette,
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
    pub fn new(s: Start, env: Env) -> (Sim, Vec<SimEffect>) {
        let panel = Panel { open: s.file.is_none(), focus: s.file.is_none(), ..Panel::default() };
        let (root, keys, size) = (s.root, s.keys, (env.size.0.max(1), env.size.1.max(2)));
        let (history, pal, cwd) = (env.history, env.pal, env.cwd);
        let mut sim = Sim { panel, cwd, size, keys, root, history, pal, ..Sim::default() };
        (sim.now, sim.t0) = (env.now, env.now);
        let mut out = vec![SimEffect::Read(sim.root.clone())];
        out.push(SimEffect::Files);
        sim.git(&mut out);
        if let Some(f) = s.file {
            sim.note(&sim.cwd.join(&f), &mut out);
            sim.request(f, &mut out);
        }
        let screen = sim.frame(&mut out);
        out.push(SimEffect::Render(screen));
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
            SimEvent::Tick(n) => {
                self.now = self.t0 + n / 1000;
                return self.snapshots(None, || Event::Tick(n));
            }
            SimEvent::Signal(n) => {
                let mut out = self.snapshots(None, || Event::Signal(n));
                out.push(SimEffect::Quit(128 + n));
                return out;
            }
            SimEvent::Listing(dir, r) => self.listing(&dir, r, &mut out),
            SimEvent::Opened(o) => self.opened(o, &mut out),
            SimEvent::Dir(path) => self.reveal_dir(&path, &mut out),
            SimEvent::Files(files) => {
                self.files = files;
                self.search_run(&mut out);
            }
            SimEvent::Status(r) => match r {
                Ok(status) => {
                    self.status = status;
                    self.remark();
                }
                Err(e) if e.starts_with("not a git repository") => self.no_repo = true,
                Err(e) if e == GIT_SKIPPED => {
                    // Told once (MARK-1), not again at every opening of the panel.
                    self.no_repo = true;
                    if !replace(&mut self.skipped, true) {
                        self.msg = Some(e);
                    }
                }
                Err(e) => self.msg = Some(e),
            },
            SimEvent::Osc(s) => self.pal.reply(&s),
            SimEvent::Spans(h) => {
                if let Some(b) = self.bufs.iter_mut().find(|b| b.id == h.id) {
                    b.hl = Some(h);
                }
            }
        }
        let screen = self.frame(&mut out);
        out.push(SimEffect::Render(screen));
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

    /// MARK-1: asks for git's status (not where it said there is no repository).
    fn git(&mut self, out: &mut Vec<SimEffect>) {
        out.extend((!self.no_repo).then_some(SimEffect::Status));
    }

    /// MARK-1: the marks from git's status and the paths seen changing on disk.
    fn remark(&mut self) {
        self.marks = Marks::new(&self.status.0, &self.status.1, &self.disk);
        self.refocus();
    }

    /// MARK-2: notes that the user opened a file, and has the history saved.
    fn note(&mut self, path: &Path, out: &mut Vec<SimEffect>) {
        self.history.record(&path.to_string_lossy(), self.now);
        out.push(SimEffect::History(self.history.clone()));
    }

    /// Gives an event to one file's App, and runs what it asks for. A change on disk or a save
    /// is a change for MARK-1 (it works without git too), and git is asked again.
    fn to(&mut self, id: Id, ev: Event, out: &mut Vec<SimEffect>) {
        let changed = matches!(ev, Event::Disk { .. } | Event::Saved(Ok(_)));
        let Some(b) = self.bufs.iter_mut().find(|b| b.id == id) else { return };
        let (fx, path) = (b.app.update(ev), b.app.path.clone());
        self.run(id, fx, out);
        if let (true, Some(path)) = (changed, path) {
            if let Ok(rel) = self.cwd.join(path).strip_prefix(&self.root) {
                self.disk.insert(rel.into());
                self.remark();
            }
            self.git(out);
        }
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
        self.refocus();
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
            Key::Ctrl('p') => return self.find(out),
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
        // SYN-2: `zh` turns the colors off and on. si's parser takes the `h` after a `z` and
        // does nothing, so the App still gets the key.
        let zh = self.z && k == Key::Char('h');
        self.z = self.idle() && k == Key::Char('z');
        self.to(id, Event::Key(k), out);
        if zh {
            self.hl_off = !self.hl_off;
            self.msg = Some(format!("syntax colors {}", if self.hl_off { "off" } else { "on" }));
        }
    }

    /// The names of the files `pick` says yes to, in order, with their ids.
    fn named(&self, pick: impl Fn(&Buf) -> bool) -> (Vec<Id>, String) {
        let files = self.bufs.iter().filter(|b| pick(b));
        let (ids, names): (_, Vec<_>) =
            files.map(|b| (b.id, b.app.path.clone().unwrap_or_default())).unzip();
        (ids, names.join(", "))
    }

    /// BUF-3: `:qa` and `:wa` hold SAFE-7 for each file. `:qa` stays while a file has a hunk
    /// (si's `:q` says so before it says anything about a write), then while one is modified;
    /// `:wa` leaves out the files that have a conflict with the disk, naming them, and saves the rest.
    fn run_all(&mut self, (name, bang, arg): (&str, bool, &str), out: &mut Vec<SimEffect>) {
        let (open, hunks) = self.named(|b| !b.app.buf.hunks.is_empty());
        let (modified, names) = self.named(|b| b.app.buf.dirty());
        match name {
            "qa" | "qall" if bang || (open.is_empty() && modified.is_empty()) => {
                out.extend(self.bufs.iter().map(|b| SimEffect::App(b.id, Effect::Unsnapshot)));
                out.push(SimEffect::Quit(0));
            }
            "qa" | "qall" if !open.is_empty() => {
                self.msg =
                    Some(format!("unresolved proposals in {hunks} (]c to review, :qa! discards)"))
            }
            "qa" | "qall" => {
                self.msg = Some(format!("no write since last change in {names} (:qa! to discard)"))
            }
            "wa" | "wall" if modified.is_empty() => self.msg = Some("no modified files".into()),
            "wa" | "wall" => {
                let (stuck, stuck_names) = self.named(|b| b.app.buf.hunks.iter().any(|h| h.disk));
                for id in modified.into_iter().filter(|id| !stuck.contains(id)) {
                    self.to(id, Event::Key(Key::Ctrl('s')), out);
                }
                if !stuck.is_empty() {
                    self.msg = Some(format!(
                        "not saved: conflicts with the disk in {stuck_names} (]c, :w! keeps yours)"
                    ));
                }
            }
            _ => self.open(self.cwd.join(arg), out),
        }
    }

    /// BUF-1: shows the file if it is open, else asks for it to be read. MARK-2: either way the
    /// user chose it, so it goes into the history.
    fn open(&mut self, path: PathBuf, out: &mut Vec<SimEffect>) {
        self.note(&path, out);
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
        self.bufs.push(Buf {
            id: o.id,
            key: o.key,
            app,
            frame: None,
            hl: None,
            asked: None,
            wide: 0,
        });
        self.cur = self.bufs.len() - 1;
        self.refocus();
        self.run(o.id, fx, out);
    }

    // ---- the frame ----

    /// SYN-1, SYN-3: the colors of the file on screen. The spans come from the highlighter's
    /// thread: the last ones are used until new ones come (an edit leaves the colors of the other
    /// lines as they were for the moment), and when they do not cover what is on screen they are
    /// asked for again, a screenful above and below.
    fn colors(&self, b: &Buf, f: &Frame) -> (Vec<Looks>, Option<Req>) {
        if b.app.vi.mode == Mode::Review {
            return (vec![Looks::new(); f.rows.len()], None); // its rows weave mine and theirs
        }
        let buf = &b.app.buf;
        let text = |l: usize| {
            (l < buf.lines()).then(|| buf.text.line(l).chars().take(buf.line_len(l)).collect())
        };
        // Lines inserted or removed at the cursor move the lines below it: their old spans are
        // shifted to match, for the moment until the new ones arrive.
        let (hl, cur) = (b.hl.as_ref().filter(|_| !self.hl_off), buf.line_of(buf.cursor));
        let spans = |l: usize| {
            let h = hl?;
            let old = if l > cur {
                l.checked_add_signed(h.lines as isize - buf.lines() as isize)?
            } else {
                l
            };
            h.spans.get(old.checked_sub(h.from)?)
        };
        let (looks, win) = layout::colors(f, buf.lines().to_string().len().max(3), &text, &spans);
        let (ver, h) = (buf.version(), usize::from(self.size.1));
        let (Some((a, z)), Some(name)) = (win, b.app.path.clone()) else { return (looks, None) };
        let covered = |h: &Hl| h.ver == ver && h.from <= a && z < h.from + h.spans.len();
        let (from, on) = (a.saturating_sub(h), !self.hl_off && buf.text.len_bytes() <= MAX_HL);
        let n = z + h - from + 1;
        let ask = on && !b.hl.as_ref().is_some_and(covered) && b.asked != Some((ver, from, n));
        (looks, ask.then(|| Req { id: b.id, name, ver, text: buf.text.clone(), from, n }))
    }

    /// The whole screen: the panel, the current file and the status line (BUF-2: `2/5` is
    /// appended when more than one file is open).
    fn frame(&mut self, out: &mut Vec<SimEffect>) -> Screen {
        self.fit();
        self.scroll();
        let ((w, h), (pw, ew)) = (self.size, self.widths());
        let (off, focus) = (if pw > 0 { pw + 1 } else { 0 }, self.panel.open && self.panel.focus);
        let rows =
            if pw > 0 { self.view().rows(self.panel.top, usize::from(h) - 1) } else { vec![] };
        let root_str = self.root.to_string_lossy();
        let likely = if pw > 0 { self.history.top(&root_str, 5, self.now) } else { vec![] };
        let marked: Vec<(TreeRow, Mark)> =
            rows.into_iter().map(|r| self.mark(r, &likely)).collect();
        let at = self.row(self.panel.cur);
        let root = self.root.display().to_string();
        let tree = at.as_ref().map_or(root.clone(), |r| {
            format!("{}{}", r.path.display(), if r.is_dir { "/" } else { "" })
        });
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
            if !cmd {
                b.wide = 0;
            }
            if pw > 0 && (cmd || status.width() + 2 >= usize::from(ew)) {
                let (top, skip, center, size) = (b.app.top, b.app.skip, b.app.center, b.app.size);
                (b.app.size.0, b.app.vi.off) = (w, b.wide);
                let wide = view::render(&mut b.app);
                b.wide = b.app.vi.off;
                (b.app.top, b.app.skip, b.app.center, b.app.size) = (top, skip, center, size);
                status = wide.rows.last().map_or(String::new(), |r| r.text.clone());
                cursor = if cmd { wide.cursor } else { cursor };
            }
        }
        if focus && self.bufs.get(self.cur).is_none_or(|b| b.app.msg.is_none()) {
            status = format!("{tree}  TREE");
        }
        if let (true, Some(s)) = (focus, &self.panel.search) {
            let n = s.found.hits.len();
            status = match (s.query.is_empty(), n) {
                (true, _) => format!("/{}", s.query),
                (_, 0) => format!("/{}  no match", s.query),
                _ => format!("/{}  {n} match{}", s.query, if n == 1 { "" } else { "es" }),
            };
            if s.typing {
                (cursor, shape) = ((1 + s.query.width() as u16, h - 1), Shape::Bar);
            }
        }
        status = self.msg.clone().unwrap_or(status);
        if self.bufs.len() > 1 && !cmd {
            status += &format!("  {}/{}", self.cur + 1, self.bufs.len());
        }
        let sel = focus.then(|| self.panel.cur - self.panel.top);
        let typing = self.panel.search.as_ref().is_some_and(|s| s.typing);
        if let (Some(i), false) = (sel, typing) {
            let col = at.map_or(0, |r| (2 * r.depth + 2) as u16);
            (cursor, shape) = ((col.min(pw.saturating_sub(1)), i as u16), Shape::Block);
        }
        let ed = self.bufs.get(self.cur).and_then(|b| Some((b, b.frame.as_ref()?)));
        let (elooks, ask) = ed.map_or((vec![], None), |(b, f)| self.colors(b, f));
        if let (Some(req), Some(b)) = (ask, self.bufs.get_mut(self.cur)) {
            b.asked = Some((req.ver, req.from, req.n));
            out.push(SimEffect::Highlight(req));
        }
        let editor =
            self.bufs.get(self.cur).and_then(|b| b.frame.as_ref()).map(|f| (f, &elooks[..]));
        let (mut rows, mut looks) =
            layout::body(w, h, pw, &marked, sel, editor, self.pal.sel().is_some());
        rows.push(Row { text: layout::fit(&status, usize::from(w)).0, ..Row::default() });
        looks.push(vec![]);
        Screen { frame: Frame { width: w, height: h, rows, cursor, shape }, looks, pal: self.pal }
    }

    /// A row of the panel with what it shows besides its name (see `layout::Mark`).
    fn mark(&self, r: TreeRow, likely: &[(String, f64)]) -> (TreeRow, Mark) {
        let (p, name) = (r.path.as_path(), r.path.to_string_lossy());
        let hit = self.panel.search.as_ref().filter(|s| !s.query.is_empty()).map(|s| {
            r.is_dir && s.found.dirs.contains(name.as_ref())
                || !r.is_dir
                    && (self.files.binary_search_by(|f| f.as_str().cmp(&name)).ok())
                        .is_some_and(|i| s.found.hits.binary_search_by_key(&i, |h| h.index).is_ok())
        });
        let mark = Mark {
            change: !r.is_dir && self.marks.changed(p),
            below: r.is_dir && self.marks.changed_below(p),
            likely: !r.is_dir && likely.iter().any(|l| Path::new(&l.0) == p),
            hit,
        };
        (r, mark)
    }
}
