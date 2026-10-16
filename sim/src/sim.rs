//! TREE-1, TREE-2, TREE-4, TREE-5, BUF-1, BUF-2, BUF-3, FIND-3, MARK-1, MARK-2, SYN-1, SYN-2, SYN-3, SYN-5, LOOK-1, LOOK-8
//! sim's state (pure): the open files (each one a si App), the panel, and what each event does.
//! `Sim::update` takes an event and returns what the shell is to do; it reads no clock, no file
//! and no terminal, so the tests drive it without threads. The panel's keys, search and focus
//! are in `panel.rs`, the search of file contents in `live.rs`.
mod live;
mod panel;
mod ptr;

use crate::color::{Syntax, Tones};
use crate::grep;
use crate::io::GIT_SKIPPED;
use crate::layout::{self, Deck, Ed, Fade, Geo, Kind, Mark, Skin};
use crate::marks::Marks;
use crate::paint::{Looks, Screen};
use crate::rank::History;
use crate::syn::Spans;
use crate::tree::{Entry, Row as TreeRow, Tree};
use crossterm::event::MouseEvent;
use live::Live;
use panel::Search;
use ropey::Rope;
use si::core::app::{App, Effect, Event, Key, Read};
use si::core::buffer::Buffer;
use si::core::cli;
use si::core::color::{Palette, palette};
use si::core::vi::Mode;
use si::core::view::{self, Frame, Row, Shape};
use si::io::journal::Snapshot;
use std::collections::HashSet;
use std::mem::{replace, take};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use unicode_width::UnicodeWidthStr;

/// A file open in sim. Ids are never reused, so a late answer for a closed file is dropped.
pub type Id = usize;

/// The first and the last line of the file that a frame shows.
type Win = (usize, usize);

/// SYN-3: files over this many bytes are not highlighted.
pub const MAX_HL: usize = 10_000_000;

#[derive(Debug)]
pub enum SimEvent {
    /// An event of one open file: a change on disk, the result of a save, a reload.
    App(Id, Event),
    /// What the terminal sent that si understands: a key, a paste, a new size, its background.
    Term(Event),
    /// The mouse (PTR-1): a click, a drag, a release, the wheel.
    Mouse(MouseEvent),
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
    /// The answer to `SimEffect::Open` when the path is a directory: the id asked for, and the path.
    Dir(Id, String),
    /// FIND-3: the files under the root, sorted, relative to it.
    Files(Vec<String>),
    /// MARK-1: git's status, and where the root lies below the top of the repository.
    Status(Result<(Vec<u8>, String), String>),
    /// SYN-1: the payload of a reply of the terminal to OSC 4.
    Osc(String),
    /// SYN-3: the spans of a window of a file.
    Spans(Hl),
    /// GREP-5: a batch of what a content search found.
    Grep(grep::Batch),
    /// GREP-4: the answer to `SimEffect::Preview`.
    Preview(Pv),
}

/// GREP-4: a file read for the preview: the name asked for (relative to the root) and what the
/// read gave.
#[derive(Debug)]
pub struct Pv {
    pub path: String,
    pub file: Read,
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
    /// Search the contents of files and answer with `SimEvent::Grep`, in batches; a search that
    /// is not the newest is to stop (GREP-5).
    Grep(grep::Request),
    /// Read this file (relative to the root) for the preview and answer with `SimEvent::Preview` (GREP-4).
    Preview(String),
    /// LOOK-1: the look that was chosen, to remember.
    Look(Skin),
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
    /// Where the file was closed last time (CLI-6): line and column.
    pub place: Option<(usize, usize)>,
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
    /// GREP-1: the content search that runs, if any.
    live: Option<Live>,
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
    /// FIND-3: the files under the root, sorted (shared with the thread of a content search).
    files: Arc<Vec<String>>,
    /// GREP-5: the generation of the last content search asked for.
    live_gen: u64,
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
    /// VIS-4: si's palette, made of the terminal's background (it paints the four styles).
    /// SYN-1, SYN-2, SYN-5: the syntax colors, whether they are on, and a `z` just typed.
    pal: Option<Palette>,
    syn: Syntax,
    hl_off: bool,
    z: bool,
    /// LOOK-1: the look, and the colors it adds to the palette (none without a palette or truecolor, LOOK-8).
    pub skin: Skin,
    tones: Option<Tones>,
    /// `SI_ON_NORMAL` (VI-11), which every file's App runs.
    hook: Option<String>,
    /// PTR-1: the press in the text that a drag goes on from: the file, and whether it has begun.
    drag: Option<(Id, bool)>,
}

/// What `sim` starts with besides what it was asked to open: the shell's side of the world.
#[derive(Default)]
pub struct Env {
    pub cwd: PathBuf,
    pub size: (u16, u16),
    /// Seconds since the epoch.
    pub now: u64,
    pub history: History,
    pub syn: Syntax,
    pub hook: Option<String>,
    /// LOOK-1: the look to start with (calm if none is given).
    pub skin: Skin,
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

/// The history's key for a path: the same file under `./a` and `a` is one key.
fn key_of(path: &Path) -> String {
    path.components().collect::<PathBuf>().to_string_lossy().into_owned()
}

/// The width of the line numbers of a buffer (si's gutter).
fn digits(buf: &Buffer) -> usize {
    buf.lines().to_string().len().max(3)
}

/// Line `l` of the buffer without its line break; none past the end.
fn text_of(buf: &Buffer, l: usize) -> Option<String> {
    (l < buf.lines()).then(|| buf.text.line(l).chars().take(buf.line_len(l)).collect())
}

/// What follows the line address at the start of `s` (si's: `.` `$` `'<` `'>` or digits).
fn after_address(s: &str) -> Option<&str> {
    match s.chars().next()? {
        '.' | '$' => Some(&s[1..]),
        '\'' => s[1..].strip_prefix(['<', '>']),
        '0'..='9' => Some(s.trim_start_matches(|c: char| c.is_ascii_digit())),
        _ => None,
    }
}

/// What follows the range at the start of `s`, as si reads one: `%`, or an address and perhaps a
/// `,` and another.
fn after_range(s: &str) -> &str {
    let Some(rest) = s.strip_prefix('%').map_or_else(|| after_address(s), Some) else { return s };
    rest.strip_prefix(',').and_then(after_address).unwrap_or(rest)
}

/// The commands for all the files at once, which si's App does not know (BUF-3), and `:e name`,
/// which opens another file instead of replacing this one (BUF-1): name, `!`, argument. si takes
/// a range before `:e` and ignores it, and would replace this file by the other: `:%e name` is
/// `:e name` here too.
fn all(line: &str) -> Option<(&str, bool, &str)> {
    fn parse(s: &str) -> Option<(&str, bool, &str)> {
        let (name, rest) =
            s.split_at(s.find(|c: char| !c.is_ascii_alphabetic()).unwrap_or(s.len()));
        let (bang, arg) = rest.strip_prefix('!').map_or((false, rest), |a| (true, a));
        let ok = matches!(
            (name, arg.trim().is_empty()),
            ("qa" | "qall" | "wa" | "wall", true) | ("e" | "edit", false)
        );
        ok.then_some((name, bang, arg.trim()))
    }
    let s = line.strip_prefix(':')?.trim();
    parse(s).or_else(|| parse(after_range(s).trim_start()).filter(|c| c.0.starts_with('e')))
}

impl Sim {
    pub fn new(s: Start, env: Env) -> (Sim, Vec<SimEffect>) {
        let panel = Panel { open: s.file.is_none(), focus: s.file.is_none(), ..Panel::default() };
        let (root, keys, size) = (s.root, s.keys, (env.size.0.max(1), env.size.1.max(2)));
        let (history, syn, cwd) = (env.history, env.syn, env.cwd);
        let skin = env.skin;
        let mut sim = Sim {
            panel,
            cwd,
            size,
            keys,
            root,
            history,
            syn,
            skin,
            hook: env.hook,
            ..Sim::default()
        };
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

    /// Where the parts of the screen are (LOOK-3). The panel is a third of the screen, at most 32
    /// columns (0 when it is closed or the screen is too narrow for it), and a column of space
    /// follows; in depth the borders of the boxes are around it and the editor.
    pub fn geo(&self) -> Geo {
        Geo::new(self.size, self.skin, self.panel.open)
    }

    /// The columns of the panel and of the editor.
    pub fn widths(&self) -> (u16, u16) {
        let g = self.geo();
        (g.pw, g.ew)
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
            SimEvent::Term(Event::Bg(rgb)) => self.background(rgb, &mut out),
            SimEvent::Term(_) => {}
            SimEvent::Mouse(m) if self.panel.live.is_none() => self.mouse(m, &mut out),
            SimEvent::Mouse(_) => {}
            SimEvent::BackTab => self.step(self.bufs.len().saturating_sub(1)),
            SimEvent::App(id, e) => self.to(id, e, &mut out),
            SimEvent::Notice(m) => self.msg = Some(m),
            SimEvent::Tick(n) => {
                self.now = self.t0 + n / 1000;
                let (fx, drawn) = self.snapshots(None, || Event::Tick(n));
                if !drawn {
                    return fx;
                }
                out = fx;
            }
            SimEvent::Signal(n) => {
                let mut out = self.snapshots(None, || Event::Signal(n)).0;
                out.push(SimEffect::Quit(128 + n));
                return out;
            }
            SimEvent::Listing(dir, r) => self.listing(&dir, r, &mut out),
            SimEvent::Opened(o) => self.opened(o, &mut out),
            SimEvent::Dir(id, path) => {
                out.push(SimEffect::Close(id)); // no file is open: its watcher is not needed
                // `open` noted it before it was known to be a directory: it is no file that was opened (MARK-2).
                self.history.forget(&key_of(&self.cwd.join(&path)));
                out.push(SimEffect::History(self.history.clone()));
                self.reveal_dir(&path, &mut out)
            }
            SimEvent::Files(files) => {
                let same = *self.files == files;
                self.files = Arc::new(files);
                self.search_run(&mut out);
                if !same {
                    self.live_run(&mut out);
                }
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
            SimEvent::Osc(s) => self.syn.reply(&s),
            SimEvent::Spans(h) => {
                if let Some(b) = self.bufs.iter_mut().find(|b| b.id == h.id) {
                    b.hl = Some(h);
                }
            }
            SimEvent::Grep(b) => self.live_batch(b, &mut out),
            SimEvent::Preview(pv) => self.live_preview(pv),
        }
        let screen = self.frame(&mut out);
        out.push(SimEffect::Render(screen));
        out
    }

    /// VIS-4: the terminal's background came, however late: si's palette is made of it, every App
    /// is told (as si's loop tells its one), and the syntax colors follow it (SYN-1).
    fn background(&mut self, rgb: (u8, u8, u8), out: &mut Vec<SimEffect>) {
        (self.pal, _) = (Some(palette(rgb)), self.syn.bg(rgb));
        self.tones = self.pal.filter(|_| self.syn.truecolor).map(|p| Tones::new(&p));
        let ids: Vec<Id> = self.bufs.iter().map(|b| b.id).collect();
        ids.into_iter().for_each(|id| self.to(id, Event::Bg(rgb), out));
    }

    /// Whether the terminal may still be answering what was asked of it (VIS-4): the quit then
    /// waits for it to finish, so that no answer is typed into the shell.
    pub fn asking(&self) -> bool {
        self.pal.is_none() || self.syn.pending()
    }

    /// Tells every App the size it has, so that what it renders fits next to the panel: the rows
    /// of the body and its status line, two fewer in REVIEW while a card is shown (LOOK-5).
    fn fit(&mut self) {
        let g = self.geo();
        let card = g.boxed && self.tones.is_some();
        for b in &mut self.bufs {
            b.app.size = (g.ew, g.rows + 1 - 2 * u16::from(card && b.app.vi.mode == Mode::Review));
        }
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
        self.history.record(&key_of(path), self.now);
        out.push(SimEffect::History(self.history.clone()));
    }

    /// Gives an event to one file's App, and runs what it asks for. A change on disk or a save
    /// is a change for MARK-1 (it works without git too), and git is asked again.
    fn to(&mut self, id: Id, ev: Event, out: &mut Vec<SimEffect>) {
        let changed = matches!(ev, Event::Disk { .. } | Event::Saved(Ok(_), _));
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
                out.extend(self.snapshots(Some(id), || Event::Signal(0)).0);
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
    /// (a tick, a signal: SAFE-6), and keeps only those requests (and, for a signal, the place of
    /// its cursor, CLI-6). The frame an App makes meanwhile is kept for the screen: a tick brings
    /// the seconds of a running filter (PROP-5).
    fn snapshots(&mut self, except: Option<Id>, ev: impl Fn() -> Event) -> (Vec<SimEffect>, bool) {
        let (mut out, mut drawn) = (vec![], false);
        for b in self.bufs.iter_mut().filter(|b| Some(b.id) != except) {
            for e in b.app.update(ev()) {
                match e {
                    Effect::Render(f) => (b.frame, drawn) = (Some(f), true),
                    Effect::Snapshot { .. } | Effect::Remember { .. } => {
                        out.push(SimEffect::App(b.id, e))
                    }
                    _ => {}
                }
            }
        }
        (out, drawn)
    }

    fn key(&mut self, k: Key, out: &mut Vec<SimEffect>) {
        self.msg = None;
        match k {
            Key::Ctrl('n') => return self.toggle(out),
            Key::Ctrl('p') => return self.find(out),
            Key::Ctrl('f') => return self.live_start(out),
            _ if self.panel.live.is_some() => return self.live_key(k, out),
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
        let (zh, zl) = (self.z && k == Key::Char('h'), self.z && k == Key::Char('l'));
        self.z = self.idle() && k == Key::Char('z');
        self.to(id, Event::Key(k), out);
        if zh {
            self.hl_off = !self.hl_off;
            self.msg = Some(format!("syntax colors {}", if self.hl_off { "off" } else { "on" }));
        }
        // LOOK-1: `zl` goes on to the next look, and the choice is remembered.
        if zl {
            self.skin = self.skin.next();
            self.msg = Some(format!("look: {}", self.skin.name()));
            out.push(SimEffect::Look(self.skin));
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
                for b in &self.bufs {
                    let fx = [Some(Effect::Unsnapshot), b.app.place()]; // CLI-6: the place is remembered
                    out.extend(fx.into_iter().flatten().map(|e| SimEffect::App(b.id, e)));
                }
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
            self.keys.clear(); // (a jump to a line was meant for the file just read)
            return out.push(SimEffect::Close(o.id));
        }
        let g = self.geo();
        let mut app = App::new(Some(o.path.clone()), o.file, (g.ew, g.rows + 1));
        (app.hook, app.cwd) = (self.hook.clone(), Some(self.cwd.to_string_lossy().into_owned()));
        let mut fx = vec![];
        if let Some(Found(s, file, age, by)) = o.found {
            let v = app.buf.version();
            fx = app.update(Event::Recover(s.base, s.text, age, file, by));
            if app.buf.version() != v {
                out.push(SimEffect::Hold(o.id, s.path, s.pid));
            }
        }
        // A `+cmd` or `file:line` says where to go; otherwise it is where the file was closed (CLI-6).
        if let Some((l, c)) = o.place.filter(|_| self.keys.is_empty()) {
            fx.extend(app.update(Event::Place(o.path, l, c)));
        }
        fx.extend(self.pal.map(|p| app.update(Event::Bg(p.bg))).into_iter().flatten());
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
    fn colors(&self, b: &Buf, f: &Frame, ch: &[Kind]) -> (Vec<Looks>, Option<Req>, Option<Win>) {
        let review = b.app.vi.mode == Mode::Review;
        if review && !self.deep() {
            return (vec![Looks::new(); f.rows.len()], None, None); // its rows weave mine and theirs
        }
        let buf = &b.app.buf;
        let text = |l: usize| text_of(buf, l);
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
        let (mut looks, win) = layout::colors(f, digits(buf), &text, &spans);
        // LOOK-6: in REVIEW a row that has a change weaves mine and theirs and has no colors; the others do.
        if review {
            looks.iter_mut().zip(ch).filter(|x| *x.1 != Kind::Plain).for_each(|x| x.0.clear());
        }
        let (ver, h) = (buf.version(), usize::from(self.size.1));
        let (Some((a, z)), Some(name)) = (win, b.app.path.clone()) else {
            return (looks, None, win);
        };
        let z = z.max(a); // a screen too narrow for its gutter shows cut numbers: no window that ends before it starts
        let covered = |h: &Hl| h.ver == ver && h.from <= a && z < h.from + h.spans.len();
        let (from, on) = (a.saturating_sub(h), !self.hl_off && buf.text.len_bytes() <= MAX_HL);
        let n = z + h - from + 1;
        let ask = on && !b.hl.as_ref().is_some_and(covered) && b.asked != Some((ver, from, n));
        (looks, ask.then(|| Req { id: b.id, name, ver, text: buf.text.clone(), from, n }), win)
    }

    /// Whether depth draws its card, fog and boxes in colors: depth on a screen big enough for its
    /// boxes, with a palette and truecolor (LOOK-3, LOOK-8).
    fn deep(&self) -> bool {
        self.geo().boxed && self.tones.is_some()
    }

    /// LOOK-2: which rows of the current file's frame have a change, when flat or depth tints them.
    fn changes(&self) -> Vec<Kind> {
        let b = self.bufs.get(self.cur).filter(|_| self.skin != Skin::Calm && self.tones.is_some());
        b.and_then(|b| {
            let (buf, f) = (&b.app.buf, b.frame.as_ref()?);
            // A hunk's line: in the range its proposal would replace, or where an empty one is put.
            let hunk = |l: usize| {
                let i = buf.hunks.partition_point(|h| h.mine.start <= l);
                let near = [i.checked_sub(1), Some(i)].into_iter().flatten();
                near.filter_map(|j| buf.hunks.get(j)).any(|h| {
                    h.mine.contains(&l)
                        || h.mine.is_empty() && h.mine.start.min(buf.lines() - 1) == l
                })
            };
            Some(layout::changed(f, digits(buf), &hunk))
        })
        .unwrap_or_default()
    }

    /// LOOK-5: the rows of the card in the current file's frame, while REVIEW has one (`ch` is
    /// `changes()`).
    fn card(&self, ch: &[Kind]) -> Option<Range<usize>> {
        let (b, deep) = (self.bufs.get(self.cur)?, self.deep());
        let (a, f) = (&b.app, b.frame.as_ref()?);
        let hunk = a.buf.hunks.get(a.current()).filter(|_| deep && a.vi.mode == Mode::Review)?;
        layout::card(f, ch, digits(&a.buf), hunk.mine.end)
    }

    /// The whole screen: the panel, the current file and the status line (BUF-2: `2/5` is
    /// appended when more than one file is open).
    fn frame(&mut self, out: &mut Vec<SimEffect>) -> Screen {
        self.fit();
        self.scroll();
        self.live_scroll();
        let ((w, h), g) = (self.size, self.geo());
        let focus = self.panel.open && self.panel.focus;
        let live = self.panel.live.is_some();
        let rows = if g.pw > 0 && !live {
            self.view().rows(self.panel.top, usize::from(g.rows))
        } else {
            vec![]
        };
        let root_str = self.root.to_string_lossy();
        let likely = if g.pw > 0 { self.history.top(&root_str, 5, self.now) } else { vec![] };
        let marked: Vec<(TreeRow, Mark)> =
            rows.into_iter().map(|r| self.mark(r, &likely)).collect();
        let at = self.row(self.panel.cur);
        let root = self.root.display().to_string();
        // (a directory's name ends with `/`, which is part of the state words: a path is cut as a list of components)
        let (tree, slash) = at.as_ref().map_or((root.clone(), ""), |r| {
            (r.path.display().to_string(), if r.is_dir { "/" } else { "" })
        });
        // BUF-2: the position `2/5` follows the status, which is cut to what is left of the width;
        // on the command line, which has the whole width, there is none.
        let cmd = !focus && self.bufs.get(self.cur).is_some_and(|b| b.app.vi.mode == Mode::Cmd);
        let pos = if self.bufs.len() > 1 && !cmd {
            format!("  {}/{}", self.cur + 1, self.bufs.len())
        } else {
            String::new()
        };
        let room = usize::from(w).saturating_sub(pos.width());
        let (mut status, mut cursor, mut shape) =
            (layout::short(&root, room), (g.ex, g.y0), Shape::Block);
        if let Some(b) = self.bufs.get_mut(self.cur) {
            if b.frame.as_ref().is_some_and(|f| (f.width, f.height) != b.app.size) {
                b.frame = None;
            }
            let f = b.frame.get_or_insert_with(|| view::render(&mut b.app));
            (shape, cursor) = (f.shape, (f.cursor.0 + g.ex, f.cursor.1 + g.y0));
            status = f.rows.last().map_or(String::new(), |r| r.text.clone());
            // The App cuts its status line to its own width; the screen has more (all of it for
            // the command line, what the position leaves for the status), or less (no panel, a position).
            if !cmd {
                b.wide = 0;
            }
            let width = if cmd { w } else { room.max(1) as u16 };
            if width != g.ew && (cmd || status.width() + 2 >= usize::from(g.ew.min(width))) {
                let (top, skip, center, size) = (b.app.top, b.app.skip, b.app.center, b.app.size);
                (b.app.size.0, b.app.vi.off) = (width, b.wide);
                let wide = view::render(&mut b.app);
                b.wide = b.app.vi.off;
                (b.app.top, b.app.skip, b.app.center, b.app.size) = (top, skip, center, size);
                status = wide.rows.last().map_or(String::new(), |r| r.text.clone());
                cursor = if cmd { (wide.cursor.0, h - 1) } else { cursor };
            }
        }
        if focus && self.bufs.get(self.cur).is_none_or(|b| b.app.msg.is_none()) {
            status = layout::stated(&tree, &format!("{slash}  TREE"), room);
        }
        if let (true, Some(s)) = (focus, &self.panel.search) {
            let n = s.found.hits.len();
            let found = match (s.query.is_empty(), n) {
                (true, _) => String::new(),
                (_, 0) => "  no match".into(),
                _ => format!("  {n} match{}", if n == 1 { "" } else { "es" }),
            };
            let (found, used) = layout::fit(&found, room);
            let head = layout::tail(&format!("/{}", s.query), room - used);
            let cols = layout::fit(&head, usize::MAX).1;
            status = head + &found;
            if s.typing {
                (cursor, shape) = ((cols.min(usize::from(w) - 1) as u16, h - 1), Shape::Bar);
            }
        }
        if let Some((s, cols)) = self.live_status(room) {
            status = s;
            if self.panel.live.as_ref().is_some_and(|l| l.typing) {
                (cursor, shape) = ((cols.min(usize::from(w) - 1) as u16, h - 1), Shape::Bar);
            } else {
                (cursor, shape) = (
                    (g.px + 1.min(g.pw.saturating_sub(1)), g.y0 + self.live_cursor()),
                    Shape::Block,
                );
            }
        }
        status = layout::fit(&self.msg.clone().unwrap_or(status), room).0 + &pos;
        let sel = focus.then(|| self.panel.cur - self.panel.top).filter(|_| !live);
        let typing = self.panel.search.as_ref().is_some_and(|s| s.typing);
        if let (Some(i), false) = (sel, typing) {
            let col = at.map_or(0, |r| (2 * r.depth + 2) as u16);
            let x = g.px + col.min(g.pw.saturating_sub(1));
            (cursor, shape) = ((x, g.y0 + i as u16), Shape::Block);
        }
        let ch = self.changes();
        let looked =
            self.bufs.get(self.cur).and_then(|b| Some(self.colors(b, b.frame.as_ref()?, &ch)));
        let (elooks, ask, win) = looked.unwrap_or_default();
        if let (Some(req), Some(b)) = (ask, self.bufs.get_mut(self.cur)) {
            b.asked = Some((req.ver, req.from, req.n));
            out.push(SimEffect::Highlight(req));
        }
        // LOOK-5: the text's cursor goes down with the half row above the card.
        let card = self.card(&ch);
        if card.is_some() && !focus && !typing {
            cursor.1 += 1;
        }
        let tones = self.tones.filter(|_| self.skin != Skin::Calm);
        let fade = match (g.boxed, focus, sel) {
            (false, ..) => Fade::None,
            (_, false, _) => Fade::Recede,
            _ if live => Fade::Find(usize::from(self.live_cursor())),
            (_, _, Some(i)) if self.panel.search.is_some() => Fade::Find(i),
            _ => Fade::None,
        };
        let app = self.bufs.get(self.cur).map(|b| &b.app);
        let chrome = g.boxed.then(|| layout::chrome(app, win, usize::from(g.rows), focus));
        let deck = Deck {
            geo: g,
            tones: tones.as_ref(),
            fade,
            chrome,
            gray: self.syn.sel().is_some(),
            live: self.live_rows(g),
        };
        let pane = self.live_pane();
        let ed = self.bufs.get(self.cur).and_then(|b| Some((b, b.frame.as_ref()?)));
        let editor = match (&pane, ed) {
            (Some((f, l)), _) => {
                Some(Ed { frame: f, looks: l, digits: 3, ch: vec![], card: None, review: false })
            }
            (_, Some((b, f))) => {
                let review = b.app.vi.mode == Mode::Review;
                Some(Ed { frame: f, looks: &elooks, digits: digits(&b.app.buf), ch, card, review })
            }
            _ => None,
        };
        let (mut rows, mut looks, mut paints) = layout::body(&deck, &marked, sel, editor.as_ref());
        rows.push(Row { text: layout::fit(&status, usize::from(w)).0, ..Row::default() });
        looks.push(vec![]);
        paints.push(vec![]);
        let frame = Frame { width: w, height: h, rows, cursor, shape, pal: self.pal };
        Screen { frame, looks, paints, syn: self.syn }
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
