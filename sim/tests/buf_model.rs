//! BUF-1, BUF-3: a model of sim's shell (one event channel, a save thread, a watcher and a filter for
//! each file, late directory listings, late highlights, the real journal thread) driving
//! `Sim::update` with random, interleaved events over several files, two names of one file,
//! a symlink, a hard link, a file that does not exist yet, snapshots a dead session left, and an
//! outside process that appends to files, replaces them, deletes them or moves them away, and
//! names that hold an escape sequence or a line break (the panel must not send them raw). Words are the unit of account: `mt*` are typed by the
//! user, `rc*` come from a recovered snapshot, `ex*` are written by the outside process. A word
//! typed into one file must never show up in another (ids never cross), a word must never be lost
//! (it is on the disk, in a snapshot, or was discarded on purpose), and the screen must stay whole.
//! One test per property because the property changes the process's directory (the real shell
//! resolves relative names against it). `SI_FUZZ_CASES` sets the number of cases.
#![allow(
    clippy::panic, // the model fails by panicking, with the log of what led there
    clippy::needless_range_loop,
    clippy::collapsible_if,
    clippy::manual_is_multiple_of,
    clippy::cmp_owned,
    clippy::unwrap_used,
    clippy::type_complexity,
    clippy::cognitive_complexity,
    clippy::too_many_lines
)]
mod common;
use common::{NOW, far, keys, temp_dir};
use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use proptest::prelude::*;
use si::core::app::{Effect, Event};
use si::core::buffer::hash;
use si::io::file::{self, Saved};
use si::io::journal::{self, Snapshot};
use sim::io::{self, Job};
use sim::layout::Skin;
use sim::sim::{Env, Req, Sim, SimEffect, SimEvent, start};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

/// The names of each file the world has (the first is its canonical one).
/// (`ev` has an escape sequence in its name and `nl` a line break: only the panel can open them.)
const NAMES: [&[&str]; 7] = [
    &["f0", "./f0", "link0"],
    &["f1", "hard1"],
    &["f2", "./sub/../f2"],
    &["sub/f3"],
    &["f9"],
    &["ev\u{1b}[2Jx"],
    &["nl\nx"],
];
const OPEN: [&str; 12] = [
    "f0",
    "./f0",
    "link0",
    "f1",
    "hard1",
    "f2",
    "./sub/../f2",
    "sub/f3",
    "f9",
    "sub",
    "f0",
    "sub/f3",
];
// (Every group of keys below that is meant for the text starts with two Escapes: the first ends a
// search that is typed or lit, the second gives the focus back to the text, so that the letters
// that follow are commands of the text and not, in part, the panel's: a `:wa` of which only the
// `a` reached the text is the user's `a` and an Enter, which cut a typed word in two: not a loss.)
/// Keys that move around and open things, but never delete or overwrite text.
const NAV: [&str; 28] = [
    "<Tab>",
    "<C-n>",
    "<C-p>",
    "j",
    "k",
    "l",
    "h",
    "<CR>",
    ".",
    "-",
    "/",
    "f",
    "<Esc>",
    "n",
    "N",
    "gg",
    "G",
    "zh",
    "<C-d>",
    "<C-u>",
    "<PageDown>",
    "<Down>",
    "<Up>",
    "q",
    "/ev\n",
    "/nl\n",
    "/f2\n",
    "<C-f>",
];
const CMDS: [&str; 9] = [
    "<Esc><Esc>:w\n",
    "<Esc><Esc>:w!\n",
    "<Esc><Esc>:wq\n",
    "<Esc><Esc>:x\n",
    "<Esc><Esc>ZZ",
    "<Esc><Esc>:q\n",
    "<Esc><Esc>:wa\n",
    "<Esc><Esc>:qa\n",
    "<Esc><Esc>:seen\n",
];
const DISCARD: [&str; 4] =
    ["<Esc><Esc>:q!\n", "<Esc><Esc>ZQ", "<Esc><Esc>:qa!\n", "<Esc><Esc>:cq\n"];

static CASE: AtomicUsize = AtomicUsize::new(0);
/// Coverage: cases, cases with two buffers at once, saves done, conflicts, closes, recovered words,
/// cases with open proposals, exits by code (0, 1, 143), cases that ended with a mouse click used.
static STAT: [AtomicUsize; 10] = [const { AtomicUsize::new(0) }; 10];
fn stat(i: usize) {
    STAT[i].fetch_add(1, Ordering::Relaxed);
}

#[derive(Clone, Debug)]
enum Op {
    Type,
    Paste,
    Nav(u8),
    Open(u8),
    Cmd(u8),
    Discard(u8),
    Filter,
    Ext(u8, bool),
    /// The file is deleted, or renamed to a name nothing opens.
    Rm(u8, bool),
    SaveExec,
    SaveSend,
    WRead(u8),
    WSend(u8),
    Deliver(u8),
    ListSend(u8),
    Hl(u8),
    Worker(bool),
    FilterEnd(u8),
    Tick,
    Resize(u16, u16),
    Mouse(u8, u16, u16),
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        8 => Just(Op::Type),
        1 => Just(Op::Paste),
        6 => (0..NAV.len() as u8).prop_map(Op::Nav),
        4 => (0..OPEN.len() as u8).prop_map(Op::Open),
        4 => (0..CMDS.len() as u8).prop_map(Op::Cmd),
        1 => (0..DISCARD.len() as u8).prop_map(Op::Discard),
        1 => Just(Op::Filter),
        3 => (0..7u8, any::<bool>()).prop_map(|(f, a)| Op::Ext(f, a)),
        1 => (0..7u8, any::<bool>()).prop_map(|(f, m)| Op::Rm(f, m)),
        6 => Just(Op::SaveExec),
        6 => Just(Op::SaveSend),
        4 => (0..6u8).prop_map(Op::WRead),
        4 => (0..6u8).prop_map(Op::WSend),
        10 => (1..5u8).prop_map(Op::Deliver),
        2 => (0..4u8).prop_map(Op::ListSend),
        2 => (0..4u8).prop_map(Op::Hl),
        1 => any::<bool>().prop_map(Op::Worker),
        1 => (0..3u8).prop_map(Op::FilterEnd),
        2 => Just(Op::Tick),
        1 => (1..120u16, 1..40u16).prop_map(|(w, h)| Op::Resize(w, h)),
        3 => (0..9u8, 0..130u16, 0..45u16).prop_map(|(k, x, y)| Op::Mouse(k, x, y)),
    ]
}

struct Watch {
    target: PathBuf,
    flag: bool,
    held: Option<(u64, Option<Vec<u8>>)>,
}

struct W {
    sim: Sim,
    dir: PathBuf,
    state: PathBuf,
    hist: PathBuf,
    size: (u16, u16),
    chan: VecDeque<SimEvent>,
    saves: VecDeque<(usize, String, Vec<u8>, Option<u64>)>,
    answers: VecDeque<SimEvent>,
    watchers: BTreeMap<usize, Watch>,
    hl: io::Highlighter,
    hl_pending: Vec<Req>,
    reads: Vec<SimEvent>,
    files_req: bool,
    status_req: bool,
    filters: BTreeMap<usize, Vec<u8>>,
    jobs: Option<Sender<Job>>,
    finished: Receiver<()>,
    told: Receiver<SimEvent>,
    screen: Option<sim::paint::Screen>,
    quit: Option<i32>,
    // the model
    fid_of_key: HashMap<String, usize>,
    typed: Vec<(usize, String)>,
    externals: Vec<(usize, String)>,
    registered: HashSet<String>,
    discarded: HashSet<String>,
    forced: HashSet<usize>,
    forced_any: bool,
    discard_ok: bool,
    prev: HashMap<usize, (usize, HashSet<String>)>,
    closed: HashSet<usize>,
    n: usize,
    two: bool,
    hunks: bool,
    log: Vec<String>,
    _guards: Vec<common::TempDir>,
}

fn words(s: &str) -> HashSet<String> {
    s.split_whitespace().map(String::from).collect()
}

fn buf_words(b: &sim::sim::Buf) -> HashSet<String> {
    let mut w = words(&b.app.buf.text.to_string());
    for h in b.app.buf.hunks.iter() {
        w.extend(words(&h.theirs));
    }
    w
}

fn hostile(c: char) -> bool {
    c.is_control()
        || matches!(c, '\u{61c}' | '\u{200b}' | '\u{200e}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{feff}')
}

/// Snapshots in the state directory: (path key, pid, base, text).
fn snapshots(state: &Path) -> Vec<(String, u32, Vec<u8>, Vec<u8>)> {
    let mut out = vec![];
    for e in fs::read_dir(state).into_iter().flatten().flatten() {
        if e.path().extension().is_none_or(|x| x != "snap") {
            continue;
        }
        let data = fs::read(e.path()).unwrap();
        let nl = data.iter().position(|&b| b == b'\n').unwrap();
        let head = String::from_utf8_lossy(&data[..nl]).into_owned();
        let f: Vec<&str> = head.split_whitespace().collect();
        let (pid, p, b, t): (u32, usize, usize, usize) = (
            f[2].parse().unwrap(),
            f[4].parse().unwrap(),
            f[5].parse().unwrap(),
            f[6].parse().unwrap(),
        );
        let rest = &data[nl + 1..];
        assert_eq!(rest.len(), p + b + t, "torn snapshot {head}");
        out.push((
            String::from_utf8(rest[..p].to_vec()).unwrap(),
            pid,
            rest[p..p + b].to_vec(),
            rest[p + b..].to_vec(),
        ));
    }
    out
}

fn dead_pid() -> u32 {
    let mut c = std::process::Command::new("true").spawn().unwrap();
    let pid = c.id();
    c.wait().unwrap();
    assert!(!Path::new(&format!("/proc/{pid}")).exists());
    pid
}

impl W {
    fn new(rec: u8, size: (u16, u16)) -> W {
        let n = CASE.fetch_add(1, Ordering::SeqCst);
        let (d, s, h) = (
            temp_dir(&format!("hm{n}")),
            temp_dir(&format!("hm{n}-state")),
            temp_dir(&format!("hm{n}-hist")),
        );
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&s.0, fs::Permissions::from_mode(0o700)).unwrap();
        let dir = fs::canonicalize(&d.0).unwrap();
        for (name, text) in [
            ("f0", "f0 a\nf0 b\n"),
            ("f1", "f1 a\nf1 b\n"),
            ("f2", "f2 a\nf2 b\n"),
            ("sub/f3", "f3 a\n"),
            ("ev\u{1b}[2Jx", "f5 a\n"),
            ("nl\nx", "f6 a\n"),
        ] {
            fs::create_dir_all(dir.join(name).parent().unwrap()).unwrap();
            fs::write(dir.join(name), text).unwrap();
        }
        fs::hard_link(dir.join("f1"), dir.join("hard1")).unwrap();
        std::os::unix::fs::symlink("f0", dir.join("link0")).unwrap();
        std::env::set_current_dir(&dir).unwrap();
        let mut fid_of_key = HashMap::new();
        for (fid, names) in NAMES.iter().enumerate() {
            for name in *names {
                let key = file::resolve(&dir.join(name).to_string_lossy());
                fid_of_key.insert(key.to_string_lossy().into_owned(), fid);
            }
        }
        // snapshots that a dead session left
        let dead = dead_pid();
        for fid in 0..4 {
            if rec >> fid & 1 == 1 {
                let name = NAMES[fid][0];
                let key =
                    file::resolve(&dir.join(name).to_string_lossy()).to_string_lossy().into_owned();
                let base = fs::read(dir.join(name)).unwrap();
                let mut text = base.clone();
                text.extend(format!("rc{fid}z\n").as_bytes());
                let snap = Snapshot { path: key, pid: dead, time_ms: 1, base, text };
                journal::write(&s.0, &snap).unwrap();
            }
        }
        let (tx, told) = mpsc::channel();
        let (jobs, finished) = io::journal_thread(Some(s.0.clone()), Some(h.0.join("sim")), tx);
        // The look is depth, with a palette and truecolor: its boxes, tints, card and fog are checked
        // with every other invariant (LOOK-3 to LOOK-7).
        let (skin, syn) = (Skin::Depth, sim::color::Syntax::new(true));
        let env = Env { cwd: dir.clone(), size, now: NOW, skin, syn, ..Env::default() };
        let args: Vec<String> = vec![];
        let st = start(&args, &dir, |f| Path::new(f).exists(), |d| d.is_dir()).unwrap();
        let (mut sim, fx) = Sim::new(st, env);
        sim.root = fs::canonicalize(&sim.root).unwrap();
        let mut w = W {
            sim,
            dir,
            state: s.0.clone(),
            hist: h.0.join("sim"),
            size,
            chan: VecDeque::new(),
            saves: VecDeque::new(),
            answers: VecDeque::new(),
            watchers: BTreeMap::new(),
            hl: io::Highlighter::default(),
            hl_pending: vec![],
            reads: vec![],
            files_req: false,
            status_req: false,
            filters: BTreeMap::new(),
            jobs: Some(jobs),
            finished,
            told,
            screen: None,
            quit: None,
            fid_of_key,
            typed: vec![],
            externals: vec![],
            registered: HashSet::new(),
            discarded: HashSet::new(),
            forced: HashSet::new(),
            forced_any: false,
            discard_ok: false,
            prev: HashMap::new(),
            closed: HashSet::new(),
            n: 0,
            two: false,
            hunks: false,
            log: vec![],
            _guards: vec![d, s, h],
        };
        w.exec(fx);
        let fx = w.step(SimEvent::Term(Event::Bg((0x1e, 0x1e, 0x1e))));
        w.exec(fx);
        w
    }

    fn tok(&mut self, p: &str) -> String {
        self.n += 1;
        format!("{p}{}z", self.n)
    }

    fn fid(&self, key: &str) -> usize {
        *self.fid_of_key.get(key).unwrap_or_else(|| panic!("a buffer on an unknown key {key}"))
    }

    // ---- the shell ----

    fn flag(&mut self, target: &Path) {
        for w in self.watchers.values_mut() {
            if w.target == target {
                w.flag = true;
            }
        }
    }

    fn exec(&mut self, mut fx: Vec<SimEffect>) {
        loop {
            let mut local = vec![];
            for e in fx.drain(..) {
                if self.quit.is_some() {
                    return;
                }
                match e {
                    SimEffect::Render(s) => self.screen = Some(s),
                    SimEffect::Read(dir) => {
                        let r = io::read_dir(&dir);
                        self.reads.push(SimEvent::Listing(dir, r));
                    }
                    // GREP-5: a content search is answered at once (its threads are the shell's)
                    SimEffect::Grep(req) => {
                        io::grep(&req, None, &|| true, &mut |b| local.push(SimEvent::Grep(b)))
                    }
                    SimEffect::Preview(path) => local.push(io::preview(&self.sim.root, &path)),
                    SimEffect::Look(_) => {}
                    SimEffect::Files => self.files_req = true,
                    SimEffect::Status => self.status_req = true,
                    SimEffect::History(h) => {
                        let _ = self.jobs.as_ref().map(|j| j.send(Job::History(h)));
                    }
                    SimEffect::Highlight(req) => {
                        assert!(
                            self.sim.bufs.iter().any(|b| b.id == req.id),
                            "a highlight was asked for a file that is not open: {}",
                            req.id
                        );
                        self.hl_pending.push(req)
                    }
                    SimEffect::Open(id, path) => {
                        let target = file::resolve(&path);
                        self.watchers.insert(id, Watch { target, flag: false, held: None });
                        local.push(io::open(Some(&self.state), id, &path, Path::new(&path)));
                    }
                    SimEffect::Close(id) => {
                        self.watchers.remove(&id);
                        self.filters.remove(&id);
                    }
                    SimEffect::Hold(id, key, pid) => {
                        let _ = self.jobs.as_ref().map(|j| j.send(Job::Hold(id, key, pid)));
                    }
                    SimEffect::App(id, e) => match e {
                        Effect::Write { path, bytes, expect_hash } => {
                            self.saves.push_back((id, path, bytes, expect_hash))
                        }
                        Effect::Open(path) => {
                            let seq = file::seq();
                            let opened = Event::Opened(path.clone(), file::read(&path), seq);
                            local.push(SimEvent::App(id, opened));
                            let _ = self
                                .jobs
                                .as_ref()
                                .map(|j| j.send(Job::App(id, Effect::Open(path))));
                        }
                        Effect::Watch(path) => {
                            if let Some(old) = self.watchers.remove(&id)
                                && let Some((seq, bytes)) = old.held
                            {
                                let hash = bytes.as_deref().map(hash);
                                let ev = Event::Disk { bytes, hash, deadline: far(), seq };
                                self.chan.push_back(SimEvent::App(id, ev));
                            }
                            let target = file::resolve(&path);
                            self.watchers.insert(id, Watch { target, flag: true, held: None });
                        }
                        Effect::Snapshot { .. } | Effect::Unsnapshot | Effect::Remember { .. } => {
                            let _ = self.jobs.as_ref().map(|j| j.send(Job::App(id, e)));
                        }
                        Effect::Spawn { input, .. } => {
                            self.filters.insert(id, input);
                        }
                        Effect::Kill => {
                            if self.filters.remove(&id).is_some() {
                                let ev = Event::FilterDone(None, vec![], vec![], far());
                                self.chan.push_back(SimEvent::App(id, ev));
                            }
                        }
                        Effect::Clipboard(_) | Effect::Hook(_) => {}
                        Effect::Render(_) | Effect::Quit(_) => {}
                    },
                    SimEffect::Quit(code) => {
                        self.log.push(format!("QUIT {code}"));
                        self.on_quit(code);
                        self.quit = Some(code);
                        return;
                    }
                }
            }
            if local.is_empty() {
                break;
            }
            fx = local.into_iter().flat_map(|ev| self.step(ev)).collect();
        }
    }

    /// One event into `Sim::update`, with PTR-2 checked for the mouse. Returns the effects.
    fn step(&mut self, ev: SimEvent) -> Vec<SimEffect> {
        if let SimEvent::Term(Event::Resize(w, h)) = &ev {
            self.size = ((*w).max(1), (*h).max(2));
        }
        let mouse = matches!(ev, SimEvent::Mouse(_));
        let before: Vec<(usize, String, bool, usize)> = self
            .sim
            .bufs
            .iter()
            .map(|b| (b.id, b.app.buf.text.to_string(), b.app.buf.dirty(), b.app.buf.hunks.len()))
            .collect();
        let fx = self.sim.update(ev);
        if mouse {
            for e in &fx {
                match e {
                    SimEffect::App(
                        _,
                        Effect::Write { .. } | Effect::Spawn { .. } | Effect::Clipboard(_),
                    )
                    | SimEffect::Quit(_) => panic!("PTR-2: the mouse caused {e:?}"),
                    _ => {}
                }
            }
            for (id, text, dirty, hunks) in before {
                if let Some(b) = self.sim.bufs.iter().find(|b| b.id == id) {
                    assert_eq!(b.app.buf.text.to_string(), text, "PTR-2: the mouse edited {id}");
                    assert_eq!(
                        (b.app.buf.dirty(), b.app.buf.hunks.len()),
                        (dirty, hunks),
                        "PTR-2: the mouse changed the state of {id}"
                    );
                }
            }
        }
        fx
    }

    /// What must hold after every event.
    fn observe(&mut self) {
        // ids
        let mut seen = HashSet::new();
        for b in &self.sim.bufs {
            assert!(seen.insert(b.id), "two buffers with id {}", b.id);
            assert!(!self.closed.contains(&b.id), "id {} was reused after a close", b.id);
        }
        let keys: HashSet<&String> = self.sim.bufs.iter().map(|b| &b.key).collect();
        assert_eq!(keys.len(), self.sim.bufs.len(), "two buffers on one journal key");
        if !self.sim.bufs.is_empty() {
            assert!(self.sim.cur < self.sim.bufs.len(), "cur out of range");
        }
        for id in self.watchers.keys().chain(self.filters.keys()) {
            assert!(
                self.sim.bufs.iter().any(|b| b.id == *id),
                "a watcher or a filter outlives its file: id {id} (log {:?})",
                self.log.iter().rev().take(6).collect::<Vec<_>>()
            );
        }
        // words: register, and never in another file
        let mut now: HashMap<usize, (usize, HashSet<String>)> = HashMap::new();
        for b in &self.sim.bufs {
            let fid = self.fid(&b.key);
            let ws = buf_words(b);
            for t in &ws {
                let mine = (t.starts_with("mt") || t.starts_with("rc")) && t.ends_with('z');
                if mine && self.registered.insert(t.clone()) {
                    self.typed.push((fid, t.clone()));
                }
            }
            now.insert(b.id, (fid, ws));
        }
        for (id, (fid, ws)) in &now {
            for (owner, t) in &self.typed {
                if owner != fid && ws.contains(t) {
                    // the two names of one file are one file
                    panic!("word {t} typed into file {owner} is in the buffer {id} of file {fid}");
                }
            }
        }
        if self.sim.bufs.len() >= 2 {
            self.two = true;
        }
        if self.sim.bufs.iter().any(|b| !b.app.buf.hunks.is_empty()) {
            self.hunks = true;
        }
        // closed buffers
        for (id, (fid, ws)) in self.prev.clone() {
            if now.contains_key(&id) {
                continue;
            }
            stat(4);
            self.closed.insert(id);
            if self.discard_ok {
                self.discarded.extend(ws.iter().cloned());
                continue;
            }
            let disk = self.disk(fid);
            for (owner, t) in &self.typed {
                if *owner == fid && ws.contains(t) && !self.discarded.contains(t) {
                    assert!(
                        words(&disk).contains(t),
                        "buffer {id} of file {fid} was closed with {t} unsaved (disk: {disk:?})"
                    );
                }
            }
        }
        self.prev = now;
        // the screen
        if let (Some(s), None) = (&self.screen, self.quit) {
            let f = &s.frame;
            assert_eq!((f.width, f.height), self.size, "frame size");
            assert_eq!(f.rows.len(), usize::from(f.height), "rows");
            assert!(
                (f.cursor.0 < f.width || f.cursor.1 + 1 == f.height) && f.cursor.1 < f.height,
                "cursor {:?} off {:?}",
                f.cursor,
                self.size
            );
            for (i, r) in f.rows.iter().enumerate() {
                let cs: Vec<char> = r.text.chars().collect();
                let mut cols = 0;
                for (k, &c) in cs.iter().enumerate() {
                    assert!(!hostile(c), "row {i} has a hostile char {c:?}: {:?}", r.text);
                    cols += si::core::view::width(&cs, k, cols);
                }
                assert!(
                    cols <= usize::from(f.width),
                    "row {i} is {cols} columns of {}: {:?}",
                    f.width,
                    r.text
                );
            }
        }
    }

    fn disk(&self, fid: usize) -> String {
        let mut out = String::new();
        for n in NAMES[fid] {
            if let Ok(b) = fs::read(self.dir.join(n)) {
                out += &String::from_utf8_lossy(&b);
            }
        }
        out
    }

    fn on_quit(&mut self, code: i32) {
        if self.discard_ok && code == 0 {
            for (_, ws) in self.prev.values() {
                self.discarded.extend(ws.iter().cloned());
            }
        }
        if code == 1 {
            // :cq gives up the current file
            if let Some(b) = self.sim.bufs.get(self.sim.cur) {
                self.discarded.extend(buf_words(b));
            }
        }
    }

    // ---- ops ----

    fn push_keys(&mut self, s: &str) {
        for k in keys(s) {
            self.chan.push_back(SimEvent::Term(Event::Key(k)));
        }
    }

    fn deliver(&mut self, n: usize) {
        let mut fx = vec![];
        for _ in 0..n {
            let Some(ev) = self.chan.pop_front() else { break };
            if self.quit.is_some() {
                return;
            }
            // A paste is meant for the empty line the keys before it made. When they went to the
            // panel instead, it would land in a typed word and cut it in two: not a loss.
            if matches!(ev, SimEvent::Term(Event::Paste(_))) {
                let empty = |b: &sim::sim::Buf| {
                    b.app.buf.line_len(b.app.buf.line_of(b.app.buf.cursor)) == 0
                };
                if !self.sim.bufs.get(self.sim.cur).is_none_or(empty) {
                    continue;
                }
            }
            fx.extend(self.step(ev));
        }
        self.exec(fx);
        self.observe();
    }

    fn drain(&mut self) {
        while !self.chan.is_empty() && self.quit.is_none() {
            self.deliver(1);
        }
    }

    fn apply(&mut self, op: &Op) {
        if self.quit.is_some() {
            return;
        }
        self.log.push(format!("{op:?}"));
        match op {
            Op::Type => {
                let t = self.tok("mt");
                self.push_keys(&format!("<Esc><Esc>Go{t}<Esc>"));
            }
            Op::Paste => {
                let t = self.tok("mt");
                self.push_keys("<Esc><Esc>Go<Esc>"); // an empty line of its own
                self.chan.push_back(SimEvent::Term(Event::Paste(t)));
            }
            Op::Nav(k) => self.push_keys(NAV[usize::from(*k)]),
            Op::Open(k) => {
                // (si takes a range before `:e` and ignores it: it must not replace the buffer)
                let range = ["", "%", "1", ".,$"][usize::from(*k) % 4];
                self.push_keys(&format!("<Esc><Esc>:{range}e {}\n", OPEN[usize::from(*k)]))
            }
            Op::Cmd(k) => {
                // `:w!` keeps mine over whatever the disk holds by then. Which file it is typed in
                // is known only when the keys are delivered, so no outside word is checked after one.
                self.forced_any |= usize::from(*k) == 1;
                self.push_keys(CMDS[usize::from(*k)])
            }
            Op::Discard(k) => {
                self.drain();
                self.discard_ok = true;
                self.push_keys(DISCARD[usize::from(*k)]);
                self.drain();
                self.discard_ok = false;
            }
            Op::Filter => self.push_keys("<Esc><Esc>:%!f\n"),
            Op::Ext(f, atomic) => self.ext(usize::from(*f), *atomic),
            Op::Rm(f, rename) => self.rm(usize::from(*f), *rename),
            Op::SaveExec => self.save_exec(),
            Op::SaveSend => {
                if let Some(a) = self.answers.pop_front() {
                    self.chan.push_back(a);
                }
            }
            Op::WRead(k) => {
                let ids: Vec<usize> = self.watchers.keys().copied().collect();
                if let Some(&id) = ids.get(usize::from(*k) % ids.len().max(1)) {
                    let w = self.watchers.get_mut(&id).unwrap();
                    if w.flag && w.held.is_none() {
                        w.held = Some((file::seq(), fs::read(&w.target).ok()));
                        w.flag = false;
                    }
                }
            }
            Op::WSend(k) => {
                let ids: Vec<usize> = self.watchers.keys().copied().collect();
                if let Some(&id) = ids.get(usize::from(*k) % ids.len().max(1)) {
                    if let Some((seq, bytes)) = self.watchers.get_mut(&id).unwrap().held.take() {
                        let hash = bytes.as_deref().map(hash);
                        let ev = Event::Disk { bytes, hash, deadline: far(), seq };
                        self.chan.push_back(SimEvent::App(id, ev));
                    }
                }
            }
            Op::Deliver(n) => self.deliver(usize::from(*n)),
            Op::ListSend(k) => {
                if !self.reads.is_empty() {
                    let i = usize::from(*k) % self.reads.len();
                    let ev = self.reads.remove(i);
                    self.chan.push_back(ev);
                }
            }
            Op::Hl(k) => {
                if !self.hl_pending.is_empty() {
                    let i = usize::from(*k) % self.hl_pending.len();
                    let req = self.hl_pending.remove(i);
                    let ev = self.hl.run(req);
                    self.chan.push_back(ev);
                }
            }
            Op::Worker(files) => {
                if *files && std::mem::take(&mut self.files_req) {
                    let mut l: Vec<String> = NAMES.iter().map(|n| n[0].to_string()).collect();
                    l.sort();
                    self.chan.push_back(SimEvent::Files(l));
                } else if !*files && std::mem::take(&mut self.status_req) {
                    let ev = if self.n % 2 == 0 {
                        SimEvent::Status(Err("not a git repository: x".into()))
                    } else {
                        SimEvent::Status(Ok((b" M f0\0?? sub/f3\0".to_vec(), String::new())))
                    };
                    self.chan.push_back(ev);
                }
            }
            Op::FilterEnd(k) => {
                let ids: Vec<usize> = self.filters.keys().copied().collect();
                if let Some(&id) = ids.get(usize::from(*k) % ids.len().max(1)) {
                    let input = self.filters.remove(&id).unwrap();
                    let t = self.tok("fl");
                    let mut out = input;
                    out.extend(format!("{t}\n").as_bytes());
                    let ev = Event::FilterDone(Some(0), out, vec![], far());
                    self.chan.push_back(SimEvent::App(id, ev));
                }
            }
            Op::Tick => {
                self.n += 1;
                let ms = 1000 * self.n as u64;
                self.chan.push_back(SimEvent::Tick(ms));
            }
            Op::Resize(w, h) => {
                self.chan.push_back(SimEvent::Term(Event::Resize(*w, *h)));
            }
            Op::Mouse(k, x, y) => {
                let kind = match k {
                    0 => MouseEventKind::Down(MouseButton::Left),
                    1 => MouseEventKind::Up(MouseButton::Left),
                    2 => MouseEventKind::Drag(MouseButton::Left),
                    3 => MouseEventKind::ScrollUp,
                    4 => MouseEventKind::ScrollDown,
                    5 => MouseEventKind::Moved,
                    6 => MouseEventKind::Down(MouseButton::Right),
                    7 => MouseEventKind::ScrollLeft,
                    _ => MouseEventKind::Down(MouseButton::Left),
                };
                let m = MouseEvent { kind, column: *x, row: *y, modifiers: KeyModifiers::NONE };
                self.chan.push_back(SimEvent::Mouse(m));
            }
        }
    }

    fn ext(&mut self, fid: usize, atomic: bool) {
        let t = self.tok("ex");
        let p = self.dir.join(NAMES[fid][0]);
        let mut bytes = fs::read(&p).unwrap_or_default();
        bytes.extend(format!("{t}\n").as_bytes());
        if atomic && fid != 1 {
            let tmp = self.dir.join(format!(".ext-{}", self.n));
            fs::write(&tmp, &bytes).unwrap();
            fs::rename(&tmp, &p).unwrap();
        } else if bytes.len() <= t.len() + 1 {
            fs::write(&p, &bytes).unwrap();
        } else {
            let mut f = fs::OpenOptions::new().append(true).open(&p).unwrap();
            f.write_all(format!("{t}\n").as_bytes()).unwrap();
        }
        self.externals.push((fid, t));
        let target = file::resolve(&p.to_string_lossy());
        self.flag(&target);
    }

    /// An outside process deletes a file or moves it away (the hard link and its twin stay).
    fn rm(&mut self, fid: usize, rename: bool) {
        let p = self.dir.join(NAMES[fid][0]);
        let target = file::resolve(&p.to_string_lossy());
        let gone = if rename {
            fs::rename(&p, self.dir.join(format!(".moved-{}", self.n)))
        } else {
            fs::remove_file(&p)
        };
        if gone.is_ok() {
            self.forced.insert(fid); // its outside words went with it
            self.flag(&target);
        }
    }

    fn save_exec(&mut self) {
        let Some((id, path, bytes, expect)) = self.saves.pop_front() else { return };
        let done = file::save(&path, &bytes, expect, Some(&self.state));
        let seq = file::seq();
        let ev = match done {
            Ok(Saved::Done(h, w)) => (stat(2), Event::Saved(Ok((h, w)), seq)).1,
            Ok(Saved::Conflict(d)) => (stat(3), Event::SaveConflict(d, far(), seq)).1,
            Err(e) => Event::Saved(Err(e), seq),
        };
        self.answers.push_back(SimEvent::App(id, ev));
        let target = file::resolve(&path);
        self.flag(&target);
    }

    /// Everything that is waiting is done, in the order a quiet machine would: the saves, their
    /// answers, the watchers, the keys.
    fn settle(&mut self) {
        for _ in 0..200 {
            let mut idle = true;
            if !self.saves.is_empty() {
                self.save_exec();
                idle = false;
            }
            while let Some(a) = self.answers.pop_front() {
                self.chan.push_back(a);
                idle = false;
            }
            let ids: Vec<usize> = self.watchers.keys().copied().collect();
            for id in ids {
                let w = self.watchers.get_mut(&id).unwrap();
                if w.flag && w.held.is_none() {
                    w.held = Some((file::seq(), fs::read(&w.target).ok()));
                    w.flag = false;
                    idle = false;
                }
                if let Some((seq, bytes)) = self.watchers.get_mut(&id).unwrap().held.take() {
                    let hash = bytes.as_deref().map(hash);
                    let ev = Event::Disk { bytes, hash, deadline: far(), seq };
                    self.chan.push_back(SimEvent::App(id, ev));
                    idle = false;
                }
            }
            for ev in std::mem::take(&mut self.reads) {
                self.chan.push_back(ev);
                idle = false;
            }
            if !self.chan.is_empty() {
                idle = false;
            }
            self.drain();
            if self.quit.is_some() || idle {
                return;
            }
        }
    }

    /// Quiet: a buffer that is clean and has no proposal is its file.
    fn check_settled(&self) -> Result<(), String> {
        for b in &self.sim.bufs {
            let fid = self.fid(&b.key);
            let a = &b.app;
            if fid == 1
                || a.buf.dirty()
                || !a.buf.hunks.is_empty()
                || a.gone
                || a.buf.readonly.is_some()
            {
                continue;
            }
            let Some(path) = &a.path else { continue };
            let disk = fs::read(self.dir.join(path)).ok();
            let want = disk.map(|d| si::core::buffer::Buffer::decode(&d).text.to_string());
            if want.is_some_and(|w| w != a.buf.text.to_string()) {
                return Err(format!(
                    "a clean buffer {} ({path}) is not its file: buffer {:?} disk {:?}",
                    b.id,
                    a.buf.text.to_string(),
                    self.disk(fid)
                ));
            }
        }
        Ok(())
    }

    fn finish(&mut self) {
        // The shell's quit: close the journal's queue and wait for it.
        self.jobs = None;
        let r = self.finished.recv_timeout(Duration::from_secs(10));
        assert!(
            matches!(r, Err(mpsc::RecvTimeoutError::Disconnected)),
            "the journal thread did not end"
        );
        while let Ok(ev) = self.told.try_recv() {
            if let SimEvent::Notice(m) = ev {
                panic!("a notice from the journal: {m}");
            }
        }
    }

    fn check_end(&mut self) -> Result<(), String> {
        let snaps = snapshots(&self.state);
        let mut keys_of: HashMap<usize, Vec<String>> = HashMap::new();
        for (k, f) in &self.fid_of_key {
            keys_of.entry(*f).or_default().push(k.clone());
        }
        for (fid, t) in &self.typed {
            if self.discarded.contains(t) {
                continue;
            }
            let disk = self.disk(*fid);
            let in_snap = snaps.iter().any(|(k, _, _, text)| {
                keys_of[fid].contains(k)
                    && String::from_utf8_lossy(text).split_whitespace().any(|w| w == t)
            });
            if !words(&disk).contains(t) && !in_snap {
                return Err(format!(
                    "word {t} typed into file {fid} is lost (quit {:?}): disk {disk:?}, snapshots {:?}",
                    self.quit,
                    snaps
                        .iter()
                        .map(|(k, p, _, t)| (k, p, String::from_utf8_lossy(t).into_owned()))
                        .collect::<Vec<_>>()
                ));
            }
            // and nowhere else
            for other in 0..NAMES.len() {
                if other == *fid {
                    continue;
                }
                if words(&self.disk(other)).contains(t) {
                    return Err(format!(
                        "word {t} typed into file {fid} is on the disk of file {other}"
                    ));
                }
                for (k, _, _, text) in &snaps {
                    if keys_of[&other].contains(k)
                        && String::from_utf8_lossy(text).split_whitespace().any(|w| w == t)
                    {
                        return Err(format!(
                            "word {t} typed into file {fid} is in a snapshot of file {other}"
                        ));
                    }
                }
            }
        }
        for (fid, t) in &self.externals {
            if self.forced_any || self.forced.contains(fid) {
                continue;
            }
            let disk = self.disk(*fid);
            if !words(&disk).contains(t) {
                return Err(format!(
                    "the outside word {t} of file {fid} was overwritten: {disk:?}"
                ));
            }
        }
        // privacy of what was written
        use std::os::unix::fs::PermissionsExt;
        for e in fs::read_dir(&self.state).unwrap().flatten() {
            let m = e.metadata().unwrap().permissions().mode() & 0o777;
            if m & 0o077 != 0 {
                return Err(format!("{:?} has mode {m:o}", e.path()));
            }
        }
        if let Ok(m) = fs::metadata(self.hist.join("history")) {
            if m.permissions().mode() & 0o077 != 0 {
                return Err("history is not private".into());
            }
        }
        Ok(())
    }
}

impl Drop for W {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir("/");
    }
}

fn cases() -> u32 {
    std::env::var("SI_FUZZ_CASES").ok().and_then(|v| v.parse().ok()).unwrap_or(2000)
}

fn run(rec: u8, size: (u16, u16), ops: &[Op]) -> Result<(), String> {
    let mut w = W::new(rec, size);
    for o in ops {
        w.apply(o);
    }
    w.drain();
    if w.quit.is_none() && rec % 2 == 0 {
        // the quiet machine: the buffers that are clean are the files
        w.settle();
        if w.quit.is_none() {
            w.check_settled().map_err(|e| format!("{e}\nops: {ops:#?}\nlog: {:#?}", w.log))?;
        }
    }
    if w.quit.is_none() {
        w.chan.push_back(SimEvent::Signal(15));
        w.drain();
        assert_eq!(w.quit, Some(143), "a signal ends sim");
    }
    w.finish();
    stat(0);
    if w.two {
        stat(1);
    }
    if w.hunks {
        stat(6);
    }
    stat(5);
    match w.quit {
        Some(0) => stat(7),
        Some(1) => stat(8),
        _ => stat(9),
    }
    if STAT[0].load(Ordering::Relaxed) % 5000 == 0 {
        let v: Vec<usize> = STAT.iter().map(|a| a.load(Ordering::Relaxed)).collect();
        eprintln!(
            "stats [cases, two bufs, saves, conflicts, closes, ?, hunks, quit0, quit1, signal]: {v:?}"
        );
    }
    w.check_end().map_err(|e| format!("{e}\nops: {:#?}\nlog: {:#?}", ops, w.log))
}

proptest! {
    #![proptest_config(ProptestConfig { cases: cases(), max_shrink_iters: 4000, ..ProptestConfig::default() })]

    #[test]
    fn buf_1_model_no_word_is_lost_or_crosses_between_files(
        rec in 0u8..16,
        size in (prop_oneof![Just(80u16), Just(40), Just(120), Just(9), Just(1)], prop_oneof![Just(24u16), Just(10), Just(3), Just(2)]),
        ops in prop::collection::vec(op(), 1..70),
    ) {
        run(rec, size, &ops).map_err(TestCaseError::fail)?;
    }
}
