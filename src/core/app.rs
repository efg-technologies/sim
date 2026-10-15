//! CLI-1, CLI-3, SAFE-1, SAFE-4, VI-9, VI-12, VI-13
//! `App::update(&mut self, Event) -> Vec<Effect>`: the state, and what each Event does to it.
use super::buffer::{ABSENT, Buffer, hash};
use super::vi::{Mode, Parser};
use super::view::{self, Frame};
use ropey::Rope;
use std::mem::take;

#[rustfmt::skip]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Key {
    Char(char), Ctrl(char), Esc, Enter, Backspace, Tab, Delete,
    Up, Down, Left, Right, Home, End, PageUp, PageDown, // the cursor keys, in this order
}

/// What reading a file gave: `Ok(None)` if it does not exist, `Err` if it could not be read.
pub type Read = Result<Option<Vec<u8>>, String>;

#[derive(Debug)]
pub enum Event {
    Key(Key),
    Paste(String),
    Resize(u16, u16),
    /// The hash of what was written and a warning, or why it could not be written.
    Saved(Result<(u64, Option<String>), String>),
    /// The save was refused because the disk holds this instead.
    SaveConflict(Vec<u8>),
    /// The result of reading the file an `Effect::Open` asked for.
    Opened(String, Read),
    /// Wall-clock milliseconds.
    Tick(u64),
    Signal(i32),
}

#[rustfmt::skip]
#[derive(Debug)]
pub enum Effect {
    Render(Frame),
    Write { path: String, bytes: Vec<u8>, expect_hash: Option<u64> },
    /// Read this file and answer with `Event::Opened`.
    Open(String),
    /// Text for the system clipboard (OSC 52).
    Clipboard(String),
    Quit(i32),
}

/// A write in flight: what it saved and what to do when it is done.
struct Pending {
    version: u64,
    text: Rope,
    path: String,
    quit: bool,
}

#[derive(Default)]
pub struct App {
    pub buf: Buffer,
    pub path: Option<String>,
    pub vi: Parser,
    pub msg: Option<String>,
    /// First buffer line on the screen.
    pub top: usize,
    pub size: (u16, u16),
    /// Whether the next render puts the cursor line in the middle (VIS-12).
    pub center: bool,
    /// The other end of the Visual selection.
    pub(super) anchor: usize,
    /// The lines of `'<` and `'>`.
    pub(super) marks: (usize, usize),
    /// The one register (VI-8): text, and whether it is whole lines.
    pub(super) reg: (String, bool),
    /// The last `f F t T`: the key and the char.
    pub(super) find: Option<(char, char)>,
    /// The last search: pattern, whether it is case-sensitive regardless, whether it goes forward.
    pub(super) search: Option<(String, bool, bool)>,
    /// A buffer of at least this many bytes is searched by several threads (PERF-4): `None` is
    /// `PAR_MIN`. Tests set it to search a small buffer in parallel, or a big one in one thread.
    pub par_min: Option<usize>,
    pub(super) unit: Option<String>,
    pub(super) fx: Vec<Effect>,
    /// The keys of the change being typed, and of the last one (for `.`).
    rec: Vec<Key>,
    pub(super) last: Vec<Key>,
    changed: bool,
    /// Set when an operator found nothing to act on.
    pub(super) failed: bool,
    /// Hash of the file as last read or written; `None` for a file that does not exist.
    disk: Option<u64>,
    saving: Option<Pending>,
}

impl App {
    pub fn new(path: Option<String>, file: Read, size: (u16, u16)) -> App {
        let mut app = App { size: (size.0.max(1), size.1.max(2)), ..Default::default() };
        app.load(path, file);
        app
    }

    /// Replaces the buffer by a file just read: path, disk hash, undo, cursor and marks restart.
    fn load(&mut self, path: Option<String>, file: Read) {
        let bytes = file.as_ref().ok().and_then(Option::as_deref);
        self.buf = Buffer::decode(bytes.unwrap_or_default());
        self.disk = bytes.map(hash);
        if let Err(e) = &file {
            self.buf.readonly = Some(format!("read-only: cannot read: {e}"));
        }
        (self.path, self.vi, self.top, self.marks, self.unit) =
            (path, Parser::default(), 0, (0, 0), None);
    }

    /// The first frame, then the effects of typing `keys` (`\n` is Enter).
    pub fn start(&mut self, keys: &str) -> Vec<Effect> {
        let mut fx = vec![Effect::Render(view::render(self))];
        for c in keys.chars() {
            fx.extend(self.update(Event::Key(if c == '\n' { Key::Enter } else { Key::Char(c) })));
        }
        fx
    }

    pub fn update(&mut self, ev: Event) -> Vec<Effect> {
        match ev {
            Event::Tick(_) => return vec![],
            Event::Signal(n) => return vec![Effect::Quit(128 + n)],
            Event::Key(k) => {
                self.msg = None;
                self.key(k);
            }
            Event::Paste(s) => {
                self.msg = None;
                let s = s.replace("\r\n", "\n").replace('\r', "\n");
                if self.vi.mode == Mode::Cmd {
                    self.vi.insert_str(&s.replace('\n', " "));
                } else {
                    self.leave_visual();
                    match self.buf.readonly.clone() {
                        Some(why) => self.say(why),
                        None => self.paste_text(&s),
                    }
                }
            }
            Event::Resize(w, h) => self.size = (w.max(1), h.max(2)),
            Event::Saved(r) => self.saved(r),
            Event::SaveConflict(_) => self.conflict(),
            Event::Opened(path, file) => self.load(Some(path), file),
        }
        let frame = view::render(self);
        self.fx.push(Effect::Render(frame));
        take(&mut self.fx)
    }

    pub(super) fn say(&mut self, m: impl Into<String>) {
        self.msg = Some(m.into());
    }

    /// A key in any mode. Ctrl-s saves in all of them; the keys of a change are recorded for `.`.
    pub(super) fn key(&mut self, k: Key) {
        if k == Key::Ctrl('s') {
            return self.ex("w");
        }
        let (mode, version) = (self.vi.mode, self.buf.version());
        let typing = matches!(mode, Mode::Normal | Mode::Insert);
        if self.vi.idle() {
            self.rec.clear();
        }
        if typing {
            self.rec.push(k);
        }
        match mode {
            Mode::Insert => self.insert(k),
            Mode::Cmd => {
                if let Some(line) = self.vi.cmdline(k) {
                    self.submit(&line);
                }
            }
            _ => {
                if let Some(c) = self.vi.key(k) {
                    self.exec(c);
                }
            }
        }
        self.changed |= typing && self.buf.version() != version;
        if self.vi.idle() && take(&mut self.changed) && !self.rec.is_empty() {
            self.last = take(&mut self.rec);
        }
    }

    /// `:w`. Without `force` the write is refused if the disk changed since it was read, and
    /// `:w {other}` is refused if `other` exists.
    pub(super) fn write(&mut self, arg: &str, force: bool, quit: bool) {
        if let Some(why) = self.buf.readonly.clone() {
            return self.say(why);
        }
        if self.saving.is_some() {
            return self.say("a save is already running");
        }
        let Some(path) = (if arg.is_empty() { self.path.clone() } else { Some(arg.to_string()) })
        else {
            return self.say("no file name");
        };
        let same = self.path.as_deref() == Some(path.as_str());
        let expect_hash = (!force).then(|| if same { self.disk.unwrap_or(ABSENT) } else { ABSENT });
        let text = self.buf.text.clone();
        self.saving = Some(Pending { version: self.buf.version(), text, path: path.clone(), quit });
        self.fx.push(Effect::Write { path, bytes: self.buf.encode(), expect_hash });
    }

    fn saved(&mut self, r: Result<(u64, Option<String>), String>) {
        let Some(p) = self.saving.take() else { return };
        let (h, warn) = match r {
            Ok(done) => done,
            Err(e) => return self.say(e),
        };
        self.path.get_or_insert_with(|| p.path.clone());
        if self.path.as_deref() == Some(p.path.as_str()) {
            self.buf.mark_saved(p.version, p.text);
            self.disk = Some(h);
        }
        let note = warn.map_or(String::new(), |w| format!(" (warning: {w})"));
        match (p.quit, self.buf.dirty()) {
            (true, false) => self.fx.push(Effect::Quit(0)),
            (true, true) => self.say("saved, but edited since: not quitting"),
            _ => self.say(format!("saved {}{note}", p.path)),
        }
    }

    fn conflict(&mut self) {
        let Some(p) = self.saving.take() else { return };
        let what = if self.path.as_deref() == Some(p.path.as_str()) {
            "file changed on disk".into()
        } else {
            format!("{} exists", p.path)
        };
        self.say(format!("not saved: {what} (:w! to overwrite)"));
    }
}
