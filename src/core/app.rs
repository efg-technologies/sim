//! CLI-1, CLI-3, SAFE-1, SAFE-4, SAFE-6, SAFE-7, PROP-1, PROP-9, VI-2, VI-7, VI-9, VI-12, VI-13
//! `App::update(&mut self, Event) -> Vec<Effect>`: the state, and what each Event does to it.
use super::buffer::{ABSENT, Buffer, Format, hash};
use super::review::{CONFLICTS, PROPOSALS};
use super::vi::{Mode, Parser};
use super::view::{self, Frame};
use ropey::Rope;
use std::collections::VecDeque;
use std::mem::take;
use std::sync::Arc;
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
    /// The hash of what was written and a warning, or why it could not be written.
    Saved(Result<(u64, Option<String>), String>),
    /// The save was refused because the disk holds this instead (`None`: no file). The diff of
    /// the merge gives up at the instant, as for `Disk` (PROP-7).
    SaveConflict(Option<Vec<u8>>, Instant),
    /// The file changed on disk (PROP-1): its new content, `None` if it is gone. The shell stamps
    /// each one with the time its diff must be done by (PROP-7), since the core reads no clock, and
    /// with the hash of the content when the thread that read the file has made it: 10 MB take 8 ms
    /// to hash, which no key should wait for (P3). `None`: App makes it.
    #[rustfmt::skip]
    Disk { bytes: Option<Vec<u8>>, hash: Option<u64>, deadline: Instant },
    /// A thread has something to tell the user, e.g. that snapshots cannot be written (SAFE-6).
    Notice(String),
    /// The result of reading the file an `Effect::Open` asked for.
    Opened(String, Read),
    /// Unsaved text left by a session that ended abnormally (SAFE-6): the base it was edited
    /// from, the text, its age in ms, the file that holds it, and the instant its diff gives up
    /// at. It is applied (as one undo step) if the disk still holds the base, merged otherwise.
    Recover(Vec<u8>, Vec<u8>, u64, String, Instant),
    /// Milliseconds on the shell's tick clock. The shell sends one at least every second
    /// however busy the keys keep it, and its ticks are 1000 apart.
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
    /// Watch this file from now on, and no other (PROP-1): answers come as `Event::Disk`.
    Watch(String),
    /// Text for the system clipboard (OSC 52).
    Clipboard(String),
    /// Unsaved text for the journal (SAFE-6), keyed by `path` (`None`: an unnamed buffer).
    Snapshot { path: Option<String>, base: Rope, text: Rope },
    /// The journal's snapshots are not needed any more: the buffer is clean, or we quit.
    Unsnapshot,
    Quit(i32),
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
    /// The hunk REVIEW is on (PROP-8); `review::current` checks it against the cursor.
    pub(super) rev: usize,
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
    /// The file was there and has been deleted (PROP-1).
    pub gone: bool,
    pub(super) saving: Option<Pending>,
    /// The buffer version when each `:e` ran, oldest first: a load is refused if the buffer changed
    /// since. An `Opened` nobody asked for is ignored.
    pub(super) opening: VecDeque<u64>,
    /// The version whose text the journal holds, and the tick that asked for it.
    snap: Option<(u64, u64)>,
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
        // A save for the old buffer that is still running must not touch this one when it ends, and a
        // change that was being typed is not one to repeat. A `:e` that is still to be answered was
        // typed before this load: it expects the new buffer to be as it is now (versions restart).
        (self.saving, self.rec, self.changed) = (None, vec![], false);
        self.opening.iter_mut().for_each(|v| *v = 0);
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
        let answer = matches!(ev, Event::Saved(_) | Event::SaveConflict(..));
        match ev {
            Event::Tick(now) => {
                let (b, v) = (&self.buf, self.buf.version());
                if b.dirty() && self.snap.is_none_or(|(s, t)| s != v && now >= t + 1000) {
                    let (path, base, text) = (self.path.clone(), b.base.clone(), b.text.clone());
                    self.fx.push(Effect::Snapshot { path, base, text });
                    self.snap = Some((v, now));
                }
                return take(&mut self.fx);
            }
            Event::Signal(n) => {
                self.snap = None; // a signal is a tick that cannot wait
                let mut fx = self.update(Event::Tick(0));
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
                if self.vi.mode == Mode::Review {
                    self.vi.mode = Mode::Normal;
                }
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
            Event::SaveConflict(disk, by) => self.conflict(disk, by),
            Event::Disk { bytes, hash, deadline } => self.disk(bytes, hash, deadline),
            Event::Notice(m) => self.say(m),
            Event::Opened(path, file) => match self.opening.pop_front() {
                None => {}
                Some(v) if v != self.buf.version() => self.say("not opened: edited meanwhile"),
                Some(_) => {
                    self.load(Some(path.clone()), file);
                    self.fx.push(Effect::Watch(path));
                }
            },
            Event::Recover(base, text, ms, file, by) => self.recover(&base, &text, ms, &file, by),
        }
        // The snapshots go when the buffer is clean again, and before we quit (a signal keeps them).
        if answer {
            // What the watcher said while the save was in flight cannot be ordered against its rename:
            // the shell reads the file once more, and `Watch` does (PROP-1).
            self.fx.extend(self.path.clone().map(Effect::Watch));
        }
        if self.buf.hunks.is_empty() && self.vi.mode == Mode::Review {
            self.vi.mode = Mode::Normal; // a save of mine settled the conflicts (SAFE-7)
        }
        let quit = self.fx.iter().position(|e| matches!(e, Effect::Quit(_)));
        let clean = !self.buf.dirty() && self.snap.take().is_some();
        if let Some(at) = quit.or(clean.then_some(self.fx.len())) {
            self.fx.insert(at, Effect::Unsnapshot);
        }
        let frame = view::render(self);
        self.fx.push(Effect::Render(frame));
        take(&mut self.fx)
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
    pub(super) fn key(&mut self, k: Key) {
        if k == Key::Ctrl('s') {
            return self.ex("w");
        }
        // REVIEW: its keys decide, `]` and `[` wait for their `c`, any other key ends it (PROP-8).
        if self.vi.mode == Mode::Review && !self.vi.waiting() {
            match k {
                Key::Char(c @ ('y' | 'n' | 'Y' | 'N')) => {
                    return self.decide(c.eq_ignore_ascii_case(&'y'), c.is_ascii_uppercase());
                }
                Key::Esc => return self.vi.mode = Mode::Normal,
                Key::Char('[' | ']') => {}
                _ => self.vi.mode = Mode::Normal,
            }
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
        self.changed |= typing && self.buf.version() != version;
        if self.vi.idle() && take(&mut self.changed) && !self.rec.is_empty() {
            self.last = take(&mut self.rec);
        }
    }

    /// `:w`. Without `force` the write is refused if the disk changed since it was read, and
    /// `:w {other}` is refused if `other` exists.
    pub(super) fn write(&mut self, arg: &str, force: bool, quit: bool) {
        let busy =
            self.saving.as_ref().map(|_| "a save").or(self.opening.front().map(|_| "a reload"));
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
        // SAFE-7: my version does not go over the disk's, nor out of the door, with a hunk open.
        let (open, disk) = (self.buf.hunks.len(), self.buf.hunks.iter().any(|h| h.disk));
        if !force && same && disk {
            return self.say(CONFLICTS);
        } else if !force && quit && open > 0 {
            return self.say(PROPOSALS);
        }
        let expect_hash = (!force).then(|| if same { self.disk.unwrap_or(ABSENT) } else { ABSENT });
        let (text, bytes) = (self.buf.text.clone(), self.buf.encode());
        let (version, hash, fmt) = (self.buf.version(), hash(&bytes), self.buf.format());
        self.saving = Some(Pending { version, hash, text, fmt, path: path.clone(), quit, force });
        self.fx.push(Effect::Write { path, bytes, expect_hash });
    }

    fn saved(&mut self, r: Result<(u64, Option<String>), String>) {
        let Some(p) = self.saving.take() else { return };
        let (h, warn) = match r {
            Ok(done) => done,
            Err(e) => return self.say(e),
        };
        self.path.get_or_insert_with(|| p.path.clone());
        let own = self.path.as_deref() == Some(p.path.as_str());
        if own {
            self.buf.mark_saved(p.version, p.text, p.fmt);
            (self.disk, self.gone) = (Some(h), false);
            if p.force {
                // Mine is the file now: the conflicts with the disk are settled. Filters' stay.
                Arc::make_mut(&mut self.buf.hunks).retain(|h| !h.disk);
            }
        }
        let note = warn.map_or(String::new(), |w| format!(" (warning: {w})"));
        match (p.quit, self.buf.dirty()) {
            // A hunk may have come back (undo) while the save was running: SAFE-7 holds for the wait too.
            (true, false) if !p.force && !self.buf.hunks.is_empty() => {
                self.say(format!("saved {}; {PROPOSALS}", p.path));
            }
            (true, false) => self.fx.push(Effect::Quit(0)),
            (true, true) if own => self.say("saved, but edited since: not quitting"),
            (true, true) => {
                let name = self.path.as_deref().unwrap_or("[no name]");
                self.say(format!("saved {}; not quitting: {name} has unsaved changes", p.path));
            }
            _ => self.say(format!("saved {}{note}", p.path)),
        }
    }

    /// The save found something else on disk (SAFE-5): that is merged, and nothing is saved.
    fn conflict(&mut self, disk: Option<Vec<u8>>, by: Instant) {
        let Some(p) = self.saving.take() else { return };
        if self.path.as_deref() != Some(p.path.as_str()) {
            return self.say(format!("not saved: {} exists (:w! to overwrite)", p.path));
        }
        let Some(bytes) = disk else {
            self.vanished();
            return self.say("not saved: the file was deleted on disk (:w to create it again)");
        };
        if let Some(n) = self.take_disk(&bytes, hash(&bytes), by) {
            let tail = if n == 0 {
                "(:w to save)".into()
            } else {
                format!("· {} to review", plural(n, "conflict", "conflicts"))
            };
            self.say(format!("not saved: the file changed on disk; merged its changes {tail}"));
        }
    }
}

/// `1 conflict`, `2 conflicts`.
pub(super) fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}
