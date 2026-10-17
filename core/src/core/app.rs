//! CLI-1, CLI-3, CLI-6, VIS-4, SAFE-1, SAFE-4, SAFE-5, SAFE-6, SAFE-7, DISK-1, DISK-6, FILT-1, VI-2, VI-7, VI-9, VI-10, VI-11, VI-12, VI-13, VI-14, PLAIN-1
//! `App::update(&mut self, Event) -> Vec<Effect>`: the state, and what each Event does to it.
use super::buffer::{ABSENT, Buffer, Format, hash};
use super::color::{Palette, Rgb, palette};
use super::vi::{Mode, Parser};
use super::view::{self, Frame};
use ropey::Rope;
use std::collections::VecDeque;
use std::mem::take;
use std::ops::Range;
use std::path::Path;
use std::time::Instant;

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
    /// The hash of what was written and a warning, or why it could not be written; the number of
    /// the answer (see `Disk`).
    Saved(Result<(u64, Option<String>), String>, u64),
    /// The save was refused because the disk holds this instead (`None`: no file). The diff of
    /// the merge gives up at the instant, as for `Disk` (DISK-5). The number is the answer's.
    SaveConflict(Option<Vec<u8>>, Instant, u64),
    /// The file changed on disk (DISK-1): its new content, `None` if it is gone. The shell stamps
    /// each one with the time its diff must be done by (DISK-5), since the core reads no clock, and
    /// with the hash of the content when the thread that read the file has made it: 10 MB take 8 ms
    /// to hash, which no key should wait for (P3). `None`: App makes it. `seq` numbers every read of
    /// the disk (this one, a load, a save's answer) from the shell's one counter, taken when the
    /// read starts: App drops a read that is older than what it took in already (DISK-1).
    #[rustfmt::skip]
    Disk { bytes: Option<Vec<u8>>, hash: Option<u64>, deadline: Instant, seq: u64 },
    /// A thread has something to tell the user, e.g. that snapshots cannot be written (SAFE-6).
    Notice(String),
    /// The answer to `Effect::Keep` (SAFE-7): where the copy is, or why it could not be made.
    Kept(Result<String, String>),
    /// The result of reading the file an `Effect::Open` asked for.
    Opened(String, Read, u64),
    /// Unsaved text left by a session that ended abnormally (SAFE-6): the base it was edited
    /// from, the text, its age in ms, the file that holds it, and the instant its diff gives up
    /// at. It is applied (as one undo step) if the disk still holds the base, merged otherwise.
    Recover(Vec<u8>, Vec<u8>, u64, String, Instant),
    /// Milliseconds on the shell's tick clock. The shell sends one at least every second
    /// however busy the keys keep it, and its ticks are 1000 apart.
    Tick(u64),
    Signal(i32),
    /// The filter ended (FILT-2): its exit code (`None`: killed by a signal), stdout and stderr. The
    /// diff of its result gives up at the instant (DISK-5).
    FilterDone(Option<i32>, Vec<u8>, Vec<u8>, Instant),
    /// Where the file just opened was left (CLI-6): path, line and column, 0-based.
    Place(String, usize, usize),
    /// The terminal's background color, from its answer to the OSC 11 query (VIS-4).
    Bg(Rgb),
}

#[rustfmt::skip]
#[derive(Debug)]
pub enum Effect {
    Render(Frame),
    Write { path: String, bytes: Vec<u8>, expect_hash: Option<u64> },
    /// Read this file and answer with `Event::Opened`.
    Open(String),
    /// Watch this file from now on, and no other (DISK-1): answers come as `Event::Disk`.
    Watch(String),
    /// Text for the system clipboard (OSC 52).
    Clipboard(String),
    /// Unsaved text for the journal (SAFE-6), keyed by `path` (`None`: an unnamed buffer).
    Snapshot { path: Option<String>, base: Rope, text: Rope },
    /// The journal's snapshots are not needed any more: the buffer is clean, or we quit.
    Unsnapshot,
    /// Keep a copy of the version of `path` that is about to be given up (SAFE-7): the disk's
    /// (`ext` "disk") before a `:w!` writes over it, mine ("yours") before an `:e!` reloads. The
    /// answer is `Event::Kept`, and what the copy was for waits for it.
    Keep { path: String, bytes: Vec<u8>, ext: &'static str },
    /// Run `cmd` with `sh -c`, `input` on its stdin (FILT-1); the answer is `Event::FilterDone`.
    Spawn { cmd: String, input: Vec<u8> },
    /// Stop the running filter's whole process group (FILT-1).
    Kill,
    /// Run this with `sh -c` and forget it (VI-11).
    Hook(String),
    /// The file is closed with its cursor here (CLI-6): path, line and column, 0-based.
    Remember { path: String, line: usize, col: usize },
    Quit(i32),
}

/// A filter in flight (FILT-1): the buffer's text when it started, the lines it was given, the last
/// tick then, and whether Ctrl-c cancelled it.
#[rustfmt::skip]
pub(super) struct Run { pub(super) text: Rope, pub(super) lines: Range<usize>, pub(super) at: u64, pub(super) killed: bool }

/// What waits for the copy that `Effect::Keep` makes (SAFE-7): a forced write (its argument, and
/// whether it quits), or a reload (the file, and the buffer's version the copy holds).
pub(super) enum Resume {
    Write(String, bool),
    Open(String, u64),
}

/// A write in flight: what it saved and what to do when it is done.
#[rustfmt::skip]
pub(super) struct Pending { version: u64, pub(super) hash: u64, text: Rope, fmt: Format, path: String, quit: bool, force: bool }

#[derive(Default)]
pub struct App {
    pub buf: Buffer,
    pub path: Option<String>,
    pub vi: Parser,
    pub msg: Option<String>,
    /// First buffer line on the screen, and how many of its rows are above the screen when it is
    /// taller than the screen (VIS-12).
    pub top: usize,
    pub skip: usize,
    /// Whether the intro (CLI-5) may show: an unnamed buffer until the first key.
    pub intro: bool,
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
    pub search: Option<(String, bool, bool)>,
    /// A buffer of at least this many bytes is searched by several threads (PERF-4): `None` is
    /// `PAR_MIN`. Tests set it to search a small buffer in parallel, or a big one in one thread.
    pub par_min: Option<usize>,
    /// A host that has the room (sim) wraps prose at the width of its pane and not at 72 columns (VIS-6).
    pub wide: bool,
    pub(super) unit: Option<String>,
    /// The line whose indent autoindent made and nothing was typed after (VI-6).
    pub(super) ai: Option<usize>,
    pub(super) fx: Vec<Effect>,
    /// The keys of the change being typed, and of the last one (for `.`).
    pub(super) rec: Vec<Key>,
    pub(super) last: Vec<Key>,
    /// Set while a Visual-mode operation, and the Insert session it may start, is typed: none of
    /// it is a Normal-mode change, so `.` must not replay it.
    no_rec: bool,
    changed: bool,
    /// `.` is replaying keys: another `.` among them does nothing.
    pub(super) replaying: bool,
    /// Set when an operator found nothing to act on.
    pub(super) failed: bool,
    /// Hash of the file as last read or written; `None` for a file that does not exist.
    pub(super) disk: Option<u64>,
    /// The file was there and has been deleted (DISK-1).
    pub gone: bool,
    /// The disk's last version while it conflicts with mine (SAFE-7): the buffer is diverged.
    pub(super) theirs: Option<Vec<u8>>,
    /// What waits for the copy of a version to be made, and where the last copy went.
    pub(super) resume: Option<Resume>,
    pub(super) copy: Option<String>,
    pub(super) saving: Option<Pending>,
    /// The buffer version when each `:e` ran, oldest first: a load is refused if the buffer changed
    /// since. An `Opened` nobody asked for is ignored.
    pub(super) opening: VecDeque<u64>,
    /// The version whose text the journal holds, and the tick that asked for it.
    snap: Option<(u64, u64)>,
    /// `SIM_ON_NORMAL` (VI-11): the shell reads it, the core does not.
    pub hook: Option<String>,
    /// Plain mode (PLAIN-1): no program is run from here, so no filter and no hook.
    pub plain: bool,
    pub(super) filter: Option<Run>,
    /// The newest number of a read of the disk that was taken in (DISK-1).
    seen: u64,
    /// The colors for the Frame's styles, once the terminal has told its background (VIS-4).
    pub pal: Option<Palette>,
    /// The last tick, in ms (FILT-1), and the mode the last update ended in (VI-11).
    pub(super) tick: u64,
    was: Mode,
    /// While a search is typed (VI-14): where its first match is. The screen is drawn around it as if
    /// the cursor were there; the cursor itself stays where it is until Enter.
    pub pv: Option<usize>,
    /// The lines of the text that the last merge of the file from disk marked as written (`+` or `~`,
    /// a conflict's too), for the host to light (UI-19); the host clears it.
    pub wrote: Vec<usize>,
}

impl App {
    pub fn new(path: Option<String>, file: Read, size: (u16, u16)) -> App {
        let size = (size.0.max(1), size.1.max(2));
        let mut app = App { size, intro: path.is_none(), ..Default::default() };
        app.load(path, file);
        app
    }

    /// Replaces the buffer by a file just read: path, disk hash, undo, cursor and marks restart.
    fn load(&mut self, path: Option<String>, file: Read) {
        let bytes = file.as_ref().ok().and_then(Option::as_deref);
        self.buf = Buffer::decode(bytes.unwrap_or_default());
        self.disk = bytes.map(hash);
        if let Err(e) = &file {
            self.buf.readonly = Some(format!("read-only: {e}"));
        }
        (self.path, self.vi, self.top, self.skip, self.marks, self.unit, self.gone) =
            (path, Parser::default(), 0, 0, (0, 0), None, false);
        (self.theirs, self.resume) = (None, None);
        // A save for the old buffer that is still running must not touch this one when it ends, and a
        // change that was being typed is not one to repeat. A `:e` that is still to be answered was
        // typed before this load: it expects the new buffer to be as it is now (versions restart).
        (self.saving, self.rec, self.changed) = (None, vec![], false);
        self.opening.iter_mut().for_each(|v| *v = 0);
    }

    /// The first frame, then the effects of typing `keys` (`\n` is Enter).
    pub fn start(&mut self, keys: &str) -> Vec<Effect> {
        let mut fx = vec![Effect::Render(view::render(self))];
        fx.extend(self.hook.clone().filter(|_| !self.plain).map(Effect::Hook)); // starting is entering Normal mode (VI-11)
        for c in keys.chars() {
            fx.extend(self.update(Event::Key(if c == '\n' { Key::Enter } else { Key::Char(c) })));
        }
        fx
    }

    pub fn update(&mut self, ev: Event) -> Vec<Effect> {
        let answer = matches!(ev, Event::Saved(..) | Event::SaveConflict(..) | Event::Opened(..));
        match ev {
            Event::Tick(now) => {
                self.tick = now;
                let (b, v) = (&self.buf, self.buf.version());
                if b.dirty() && self.snap.is_none_or(|(s, t)| s != v && now >= t + 1000) {
                    let (path, base, text) = (self.path.clone(), b.base.clone(), b.text.clone());
                    self.fx.push(Effect::Snapshot { path, base, text });
                    self.snap = Some((v, now));
                }
                let frame = self.filter.is_some().then(|| view::render(self)); // the seconds
                self.fx.extend(frame.map(Effect::Render));
                return take(&mut self.fx);
            }
            Event::Signal(n) => {
                self.snap = None; // a signal is a tick that cannot wait
                let mut fx = self.update(Event::Tick(0));
                fx.extend(self.place());
                fx.push(Effect::Quit(128 + n));
                return fx;
            }
            Event::Key(k) => {
                (self.msg, self.intro) = (None, false);
                self.key(k);
                self.buf.see();
            }
            Event::Paste(s) => {
                self.msg = None;
                self.vi.reset();
                let s = s.replace("\r\n", "\n").replace('\r', "\n");
                if self.vi.mode == Mode::Cmd {
                    self.vi.insert_str(&s.replace('\n', " "));
                    self.pv = self.preview();
                } else {
                    self.leave_visual();
                    match self.buf.readonly.clone() {
                        Some(why) => self.say(why),
                        None => self.paste_text(&s),
                    }
                }
            }
            Event::Resize(w, h) => self.size = (w.max(1), h.max(2)),
            Event::Saved(r, seq) => self.saved(r, seq),
            Event::SaveConflict(disk, by, seq) => self.conflict(disk, by, seq),
            Event::Disk { seq, .. } if self.stale(seq) => {}
            Event::Disk { bytes, hash, deadline, .. } => self.disk(bytes, hash, deadline),
            Event::Notice(m) => self.say(m),
            Event::Kept(r) => self.kept(r),
            Event::Opened(path, file, seq) => {
                let at = self.copy.take().map_or(String::new(), |c| format!("; yours is in {c}"));
                match self.opening.pop_front() {
                    None => {}
                    Some(v) if v != self.buf.version() => {
                        self.say(format!("not opened: edited meanwhile{at}"))
                    }
                    Some(_) if self.filter.is_some() => {
                        self.say(format!("not opened: a filter is running{at}"))
                    }
                    // The same file, read before something newer was taken in: the load would undo it.
                    Some(_) if self.stale(seq) && self.path.as_deref() == Some(path.as_str()) => {
                        self.say(format!("not opened: the file changed meanwhile (:e! again){at}"))
                    }
                    // The disk came to conflict with mine after the `:e!` was typed: nothing copied
                    // mine, so this load would throw it away (SAFE-7). `:e!` again copies it first.
                    Some(_) if at.is_empty() && self.theirs.is_some() && self.buf.dirty() => {
                        self.say("not opened: the disk conflicts with yours meanwhile (:e! again)")
                    }
                    Some(_) => {
                        self.load(Some(path.clone()), file);
                        if !at.is_empty() {
                            self.say(format!("loaded {path}{at}"));
                        }
                    }
                }
            }
            Event::Recover(base, text, ms, file, by) => self.recover(&base, &text, ms, &file, by),
            Event::FilterDone(status, out, err, by) => self.filter_done(status, &out, &err, by),
            Event::Place(path, line, col) => self.resume(&path, line, col),
            Event::Bg(rgb) => self.pal = Some(palette(rgb)),
        }
        // The snapshots go when the buffer is clean again, and before we quit (a signal keeps them).
        if answer {
            // What the watcher said while the save was in flight cannot be ordered against its rename:
            // the shell reads the file once more, and `Watch` does (DISK-1). It silenced the watcher
            // when an `:e` began to read, so an answer to that is followed by a watch as well, of the
            // file just loaded or the one the buffer keeps.
            self.fx.extend(self.path.clone().map(Effect::Watch));
        }
        let quit = self.fx.iter().position(|e| matches!(e, Effect::Quit(_)));
        let clean = !self.buf.dirty() && self.snap.take().is_some();
        if let Some(at) = quit.or(clean.then_some(self.fx.len())) {
            self.fx.insert(at, Effect::Unsnapshot);
        }
        // Entering Normal mode runs the hook (VI-11); leaving the file remembers the cursor (CLI-6).
        let enter = self.was != Mode::Normal && self.vi.mode == Mode::Normal && quit.is_none();
        self.fx
            .extend(self.hook.as_ref().filter(|_| enter && !self.plain).cloned().map(Effect::Hook));
        self.was = self.vi.mode;
        if let Some(at) = quit {
            self.fx.splice(at..at, self.place()).for_each(drop);
        }
        let frame = view::render(self);
        self.fx.push(Effect::Render(frame));
        take(&mut self.fx)
    }

    /// CLI-6: where the cursor is, for the journal, if the buffer has a file.
    pub fn place(&self) -> Option<Effect> {
        let (b, path) = (&self.buf, self.path.clone()?);
        Some(Effect::Remember { path, line: b.line_of(b.cursor), col: b.col() })
    }

    /// CLI-6: the file was left here. The cursor goes there (centered) unless it has moved since the
    /// file was opened, or the file is one of git's messages, which start at the top.
    fn resume(&mut self, path: &str, line: usize, col: usize) {
        let git = ["COMMIT_EDITMSG", "MERGE_MSG", "TAG_EDITMSG", "git-rebase-todo"];
        let moved = self.path.as_deref() != Some(path) || self.buf.cursor != 0 || !self.vi.idle();
        if moved || git.iter().any(|g| Path::new(path).ends_with(g)) {
            return;
        }
        let b = &mut self.buf;
        let line = line.min(b.lines() - 1);
        (b.cursor, self.center) = (b.start(line) + col.min(b.line_len(line)), true);
        self.clamp();
    }

    /// DISK-1: whether a disk read numbered `seq` is older than what App took in already: a load, a
    /// save's answer, or a newer read. Otherwise it is the newest from now on.
    pub(super) fn stale(&mut self, seq: u64) -> bool {
        let top = self.seen.max(seq);
        std::mem::replace(&mut self.seen, top) > seq
    }

    pub(super) fn say(&mut self, m: impl Into<String>) {
        self.msg = Some(m.into());
    }

    /// Says why nothing was found, and finds nothing.
    pub(super) fn fail<T>(&mut self, m: impl Into<String>) -> Option<T> {
        self.say(m);
        None
    }

    /// Applies a snapshot as one undo step if the disk still holds its base, merges it with the
    /// disk otherwise (SAFE-6).
    fn recover(&mut self, base: &[u8], text: &[u8], ms: u64, file: &str, by: Instant) {
        #[rustfmt::skip]
        let (n, unit) = match ms / 1000 { s @ 0..60 => (s, "second"), s @ 60..3600 => (s / 60, "minute"), s @ 3600..86400 => (s / 3600, "hour"), s => (s / 86400, "day") };
        let ago = format!("{n} {unit}{} ago", if n == 1 { "" } else { "s" });
        if self.buf.readonly.is_some() {
            return self.say(format!("kept unsaved changes from {ago}, not applied: {file}"));
        }
        // A file that is gone holds nothing to merge with, and the snapshot is the only copy of its
        // text: it is applied whole (a merge would take the missing file for a deletion of every
        // line) and counts as unsaved.
        let missing = self.disk.is_none() && !base.is_empty();
        let same = missing || base == self.buf.base.to_string().as_bytes();
        if same {
            self.buf.checkpoint();
            self.buf.replace(0..usize::MAX, &String::from_utf8_lossy(text));
            if missing {
                self.vanished();
            }
        } else {
            let [b, t] = [base, text].map(|s| String::from_utf8_lossy(s) + "\n");
            self.merge_in(&b, &t, Buffer::decode(&self.buf.encode()), by);
        }
        self.snap = Some((self.buf.version(), 0));
        let (what, with) =
            if same { ("unsaved changes", "") } else { ("changes", ", merged with disk") };
        self.say(format!("recovered {what} from {ago}{with} · u to undo"));
    }

    /// A key in any mode. Ctrl-s saves in all of them; the keys of a change are recorded for `.`.
    pub(super) fn key(&mut self, mut k: Key) {
        // VI-10: full-width ASCII is the half-width key, unless it is text.
        if let Key::Char(c @ '\u{ff01}'..='\u{ff5e}') = k
            && !self.vi.text()
        {
            k = Key::Char(char::from((c as u32 - 0xfee0) as u8));
        }
        if k == Key::Ctrl('s') {
            return self.ex("w");
        }
        let (mode, version) = (self.vi.mode, self.buf.version());
        let typing = matches!(mode, Mode::Normal | Mode::Insert);
        let visual = mode.visual();
        if self.vi.idle() || visual {
            (self.rec, self.no_rec) = (vec![], visual);
        }
        if typing && !self.no_rec {
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
        self.pv = self.preview();
        self.changed |= typing && self.buf.version() != version;
        if self.vi.idle() && take(&mut self.changed) && !self.rec.is_empty() {
            self.last = take(&mut self.rec);
        }
    }

    /// `:w`. Without `force` the write is refused if the disk changed since it was read, and
    /// `:w {other}` is refused if `other` exists. A `:w!` over a version of the disk that mine
    /// conflicts with copies that version first, and writes when the copy is made (SAFE-7).
    pub(super) fn write(&mut self, arg: &str, force: bool, quit: bool) {
        let busy = self.busy();
        if let Some(why) =
            self.buf.readonly.clone().or(busy.map(|b| format!("{b} is already running")))
        {
            return self.say(why);
        }
        let Some(path) = (if arg.is_empty() { self.path.clone() } else { Some(arg.to_string()) })
        else {
            return self.say("no file name");
        };
        let same = self.path.as_deref() == Some(path.as_str());
        match (self.theirs.clone(), force, same) {
            (Some(_), false, true) => {
                self.say(format!("not saved: the disk conflicts · {WAYS_OUT}"))
            }
            (Some(theirs), true, true) => {
                self.resume = Some(Resume::Write(arg.into(), quit));
                self.fx.push(Effect::Keep { path, bytes: theirs, ext: "disk" });
            }
            _ => self.put(&path, force, quit),
        }
    }

    /// The write itself: nothing is left to keep first.
    fn put(&mut self, path: &str, force: bool, quit: bool) {
        let same = self.path.as_deref() == Some(path);
        let expect_hash = (!force).then(|| if same { self.disk.unwrap_or(ABSENT) } else { ABSENT });
        let (text, bytes) = (self.buf.text.clone(), self.buf.encode());
        let (version, hash, fmt) = (self.buf.version(), hash(&bytes), self.buf.format());
        let path = path.to_string();
        self.saving = Some(Pending { version, hash, text, fmt, path: path.clone(), quit, force });
        self.fx.push(Effect::Write { path, bytes, expect_hash });
    }

    /// What is running that a save or a reload must not start over: a save, a reload, or the copy
    /// that one of them waits for.
    pub(super) fn busy(&self) -> Option<&'static str> {
        let kind = |r: &Resume| if matches!(r, Resume::Write(..)) { "a save" } else { "a reload" };
        let asked =
            self.saving.as_ref().map(|_| "a save").or(self.opening.front().map(|_| "a reload"));
        asked.or(self.resume.as_ref().map(kind))
    }

    /// The copy of SAFE-7 is made (or could not be): what waited for it goes on, or is given up and
    /// nothing is lost.
    fn kept(&mut self, r: Result<String, String>) {
        let at = match (self.resume.take(), r) {
            (None, _) => return,
            (Some(Resume::Write(..)), Err(e)) => {
                return self.say(format!("not saved: cannot keep the disk's version: {e}"));
            }
            (Some(Resume::Open(..)), Err(e)) => {
                return self.say(format!("not reloaded: cannot keep yours: {e}"));
            }
            (Some(Resume::Write(arg, quit)), Ok(at)) => {
                self.copy = Some(at);
                let path = if arg.is_empty() { self.path.clone() } else { Some(arg) };
                if let Some(p) = path {
                    self.put(&p, true, quit);
                }
                return;
            }
            (Some(Resume::Open(path, version)), Ok(at)) => (path, version, at),
        };
        let (path, version, at) = at;
        if version != self.buf.version() {
            // Typed since the copy was asked for: the copy does not hold it.
            return self.say(format!("not reloaded: edited meanwhile; yours is in {at}"));
        }
        self.copy = Some(at);
        self.fx.extend(self.place()); // leaving this file (CLI-6)
        self.opening.push_back(version);
        self.fx.push(Effect::Open(path));
    }

    fn saved(&mut self, r: Result<(u64, Option<String>), String>, seq: u64) {
        self.stale(seq); // an answer is never dropped, but reads older than it are
        let Some(p) = self.saving.take() else { return };
        let there =
            self.copy.take().map_or(String::new(), |c| format!("; the disk's version is in {c}"));
        let (h, warn) = match r {
            Ok(done) => done,
            Err(e) => return self.say(e + &there),
        };
        self.path.get_or_insert_with(|| p.path.clone());
        let own = self.path.as_deref() == Some(p.path.as_str());
        if own {
            self.buf.mark_saved(p.version, p.text, p.fmt);
            (self.disk, self.gone) = (Some(h), false);
            if p.force && self.theirs.take().is_some() {
                // Mine is the file now: the conflict is settled (SAFE-7), and what was marked as the
                // disk's difference is not a difference any more.
                self.buf.marks = Default::default();
            }
        }
        let note = warn.map_or(String::new(), |w| format!(" (warning: {w})"));
        match (p.quit, self.buf.dirty()) {
            (true, false) => self.fx.push(Effect::Quit(0)),
            (true, true) if own => self.say("saved, but edited since: not quitting"),
            (true, true) => {
                let name = self.path.as_deref().unwrap_or("[no name]");
                self.say(format!("saved {}; not quitting: {name} has unsaved changes", p.path));
            }
            _ => self.say(format!("saved {}{note}{there}", p.path)),
        }
    }

    /// The save found something else on disk (SAFE-5): that is merged, and nothing is saved.
    fn conflict(&mut self, disk: Option<Vec<u8>>, by: Instant, seq: u64) {
        let old = self.stale(seq);
        let Some(p) = self.saving.take() else { return };
        if self.path.as_deref() != Some(p.path.as_str()) {
            return self.say(format!("not saved: {} exists (:w! to overwrite)", p.path));
        } else if old {
            // A newer read was taken in (and merged) already: this one would undo it.
            return self.say("not saved: the file changed on disk meanwhile (:w to save again)");
        }
        let Some(bytes) = disk else {
            self.vanished();
            return self.say("not saved: the file was deleted on disk (:w to create it again)");
        };
        if self.take_disk(&bytes, hash(&bytes), by) {
            let said = if self.diverged() {
                format!("not saved: the disk changed and conflicts · {WAYS_OUT}")
            } else {
                "not saved: the file changed on disk; merged its changes (:w to save)".into()
            };
            self.say(said);
        }
    }

    /// Whether the disk holds a version that mine conflicts with (SAFE-7).
    pub fn diverged(&self) -> bool {
        self.theirs.is_some()
    }
}

/// What SAFE-7 offers while the buffer is diverged.
pub(super) const WAYS_OUT: &str =
    ":w! keeps yours · :e! takes the disk's · the other is copied first";

/// `1 conflict`, `2 conflicts`.
pub(super) fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}
