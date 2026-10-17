//! Helpers shared by the integration tests.
#![allow(dead_code, clippy::unwrap_used, clippy::panic)]
use std::path::{Path, PathBuf};
use std::process::Command;

/// A unique empty directory, removed when dropped.
pub struct TempDir(pub PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Made here and never before: `mkdir` (not `create_dir_all`) fails on a name that exists, a link
/// planted at a guessable name too, and the next name is tried. Private (0700), as a state directory
/// is (SAFE-6, STATE-1): sim keeps its choices only in such a directory.
pub fn temp_dir(name: &str) -> TempDir {
    use std::os::unix::fs::DirBuilderExt;
    static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    loop {
        let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("sim-it-{}-{n}-{name}", std::process::id()));
        match std::fs::DirBuilder::new().mode(0o700).create(&d) {
            Ok(()) => return TempDir(d),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => panic!("{}: {e}", d.display()),
        }
    }
}

/// Writes `files` (relative paths; a trailing `/` makes a directory) under `dir`.
pub fn make(dir: &Path, files: &[&str]) {
    for f in files {
        let p = dir.join(f);
        if f.ends_with('/') {
            std::fs::create_dir_all(&p).unwrap();
        } else {
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, f).unwrap();
        }
    }
}

/// Runs git in `dir` with none of the user's configuration (no global config, no global ignore
/// file) and a fixed identity. Returns stdout.
pub fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(["-c", "user.name=sim", "-c", "user.email=sim@example.com"])
        .args(["-c", "commit.gpgsign=false", "-c", "init.defaultBranch=main"])
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("XDG_CONFIG_HOME", "/nonexistent")
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// A git command as the user would run it, for the control of a witness test (the command that does
/// run the script, to show that the test can tell): `git` without the environment that chooses an
/// editor, a prompt, a transport, a pager or an external diff, and with no input. The caller runs it
/// (`.status()`); its answer does not matter, the witness it leaves does.
pub fn plain(dir: &Path, args: &[&str]) -> Command {
    let mut c = Command::new("git");
    c.args(["-c", "user.name=sim", "-c", "user.email=sim@example.com"])
        .args(["-c", "commit.gpgsign=false", "-c", "init.defaultBranch=main"])
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("XDG_CONFIG_HOME", "/nonexistent")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    for var in ["GIT_EDITOR", "VISUAL", "EDITOR", "GIT_ASKPASS", "GIT_ALLOW_PROTOCOL"] {
        c.env_remove(var);
    }
    c
}

/// A repository in `dir` with these files committed.
pub fn repo(dir: &Path, files: &[&str]) {
    git(dir, &["init", "-q"]);
    make(dir, files);
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", "init"]);
}

/// A repository in `dir` (the files `src/main.rs`, `docs/guide.md`, `top.md` and `old/gone.rs` already
/// made) with a changes view's worth of changes from HEAD: `src/main.rs` changed (+2 -1), `docs/new.md`
/// untracked (3 lines), `old/gone.rs` deleted (-4) and `top.md` moved to `docs/` with a line added (a rename).
pub fn changed_repo(dir: &Path) {
    git(dir, &["init", "-q"]);
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", "init"]);
    let w = |f: &str, c: &str| std::fs::write(dir.join(f), c).unwrap();
    w("src/main.rs", "fn main() {}\nfn a1() {}\nfn a2() {}\n");
    w("docs/new.md", "x\ny\nz\n");
    w(LONG, "a name the panel has to cut\n");
    std::fs::remove_file(dir.join("old/gone.rs")).unwrap();
    git(dir, &["mv", "top.md", "docs/top.md"]);
    w("docs/top.md", "one\ntwo\nthree\nfour\n");
}

/// An untracked file of `changed_repo` whose name is longer than the panel is wide.
pub const LONG: &str = "docs/a-file-with-a-name-too-long-for-the-panel-to-show.md";

/// Moves the panel's cursor, a row at a time, to the row that says `name` (in the depth look the cursor
/// row is a layer of the screen).
pub fn walk_to(r: &mut Rig, name: &str) {
    for _ in 0..40 {
        let y = r.shot.frame.rows.iter().position(|row| row.text.contains(name));
        let at = r.shot.layers.iter().find(|l| l.kind == LayerKind::Cursor);
        if y.is_some() && y == at.map(|l| usize::from(l.rect.y)) {
            return;
        }
        r.keys("j");
    }
    panic!("no row says {name}");
}

/// The files `changed_repo` expects.
pub const CHANGED_FILES: [&str; 4] = [
    "src/main.rs=fn main() {}\nfn a() {}\n",
    "docs/guide.md=# guide\n",
    "top.md=one\ntwo\nthree\n",
    "old/gone.rs=a\nb\nc\nd\n",
];

/// What `begin_in` made: sim as a front end gets it, and the directories of its world.
pub struct Started {
    pub begun: sim::shell::Begun,
    /// The world's state directory, and the one directory its `PATH` has, with an `rg` in it.
    pub state: PathBuf,
    pub bin: PathBuf,
    _guards: Vec<TempDir>,
}

/// `shell::begin` as every front end calls it (PLAIN-1, GUI-4): on `args` in `dir`, in a world that is
/// `privileged` or not, whose state directory is a private one of its own and whose `PATH` is one
/// directory that has an `rg`; a front end that brings `hook`, the command that runs when Normal mode is
/// entered.
pub fn begin_in(dir: &Path, args: &[&str], privileged: bool, hook: Option<String>) -> Started {
    let (state, bin) = (temp_dir("begin-state"), temp_dir("begin-bin"));
    let rg = bin.0.join("rg");
    std::fs::write(&rg, "#!/bin/sh\nexit 0\n").unwrap();
    std::fs::set_permissions(&rg, Permissions::from_mode(0o755)).unwrap();
    let world = sim::shell::World {
        privileged,
        path: Some(bin.0.clone().into_os_string()),
        state: Some(state.0.clone()),
        history: None,
        moved: None,
    };
    let front = sim::shell::Front {
        size: (100, 24),
        syn: sim::color::Syntax::new(true),
        skin: Some(sim::layout::Skin::Depth),
        icons: Some(false),
        tab_uses: Some(3),
        hook,
        home: None,
    };
    let args: Vec<String> = args.iter().map(ToString::to_string).collect();
    let s = sim::shell::arguments(&args, dir).unwrap();
    let begun = sim::shell::begin(s, dir, world, front);
    Started { begun, state: state.0.clone(), bin: bin.0.clone(), _guards: vec![state, bin] }
}

/// ICON-1: the first private-use code point of a text (U+E000 to U+F8FF, planes 15 and 16): what
/// only a Nerd Font draws.
pub fn private_use(text: &str) -> Option<char> {
    text.chars().find(|&c| matches!(c, '\u{e000}'..='\u{f8ff}' | '\u{f0000}'..='\u{10ffff}'))
}

/// Compares `actual` with `tests/golden/{name}.txt`; `SIM_BLESS=1` rewrites the file. A golden is
/// drawn without icons: it holds no private-use code point (ICON-1).
pub fn golden(name: &str, actual: &str) {
    assert_eq!(
        private_use(actual),
        None,
        "{name}: a private-use code point is drawn without icons"
    );
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/golden/{name}.txt"));
    if std::env::var_os("SIM_BLESS").is_some() {
        std::fs::write(&path, actual).unwrap();
    }
    let want = std::fs::read_to_string(&path).unwrap_or_default();
    assert_eq!(actual, want, "{name}: differs from {} (SIM_BLESS=1 rewrites it)", path.display());
}

// ---- driving Sim without threads or a terminal ----

use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use sim::color::Tones;
use sim::grep::Request;
use sim::io;
use sim::paint::{LayerKind, Look, Looks, Paints, Screen};
use sim::rank::History;
use sim::sim::{Env, Req, Sim, SimEffect, SimEvent, start};
use sim_core::core::app::{App, Effect, Event, Key};
use sim_core::core::view::{Frame, Style};
use std::collections::VecDeque;
use std::fs::Permissions;
use std::mem::take;
use std::os::unix::fs::PermissionsExt;
use unicode_width::UnicodeWidthChar;

/// Key notation of si's tests: a char is itself (`\n` Enter, `\x1b` Esc, `\t` Tab); `<Esc> <CR>
/// <BS> <Tab> <Del> <Up> <Down> <Left> <Right> <Home> <End> <PageUp> <PageDown> <C-x> <lt>` are
/// the others.
pub fn keys(s: &str) -> Vec<Key> {
    let (mut out, mut rest) = (vec![], s);
    while let Some(c) = rest.chars().next() {
        let named =
            rest.strip_prefix('<').and_then(|r| r.split_once('>')).and_then(|(name, tail)| {
                let k = match name {
                    "Esc" => Key::Esc,
                    "CR" => Key::Enter,
                    "BS" => Key::Backspace,
                    "Tab" => Key::Tab,
                    "Space" => Key::Char(' '),
                    "Del" => Key::Delete,
                    "Up" => Key::Up,
                    "Down" => Key::Down,
                    "Left" => Key::Left,
                    "Right" => Key::Right,
                    "Home" => Key::Home,
                    "End" => Key::End,
                    "PageUp" => Key::PageUp,
                    "PageDown" => Key::PageDown,
                    "lt" => Key::Char('<'),
                    n if n.len() == 3 && n.starts_with("C-") => Key::Ctrl(n.chars().nth(2)?),
                    _ => return None,
                };
                Some((k, tail))
            });
        if let Some((k, tail)) = named {
            out.push(k);
            rest = tail;
            continue;
        }
        out.push(match c {
            '\n' => Key::Enter,
            '\x1b' => Key::Esc,
            '\t' => Key::Tab,
            c => Key::Char(c),
        });
        rest = &rest[c.len_utf8()..];
    }
    out
}

/// Sim with the shell played by the test: its effects are run at once, in order, on a real
/// directory. `cwd` is the directory Sim believes it runs in, and the one relative paths are
/// resolved against here (the process itself never changes directory). Nothing is watched and
/// nothing is snapshotted (the snapshots are in `log`); the journal is a private directory.
pub struct Rig {
    pub sim: Sim,
    pub cwd: PathBuf,
    pub state: PathBuf,
    pub screen: Frame,
    /// The whole last screen: the frame, its looks and its palette.
    pub shot: Screen,
    /// The background jobs Sim asked for, by name (`files`, `status`, `history`, `highlight`, `diff`).
    pub jobs: Vec<&'static str>,
    /// The last history Sim asked to have saved.
    pub history: Option<History>,
    /// Hold back the highlighter's requests until `deliver_spans` (SYN-3).
    pub hold_spans: bool,
    pub held: Vec<Req>,
    /// Hold back the content searches until `deliver_grep` (GREP-5); the rg to search with, if any.
    pub hold_grep: bool,
    pub grep_held: Vec<Request>,
    pub rg: Option<PathBuf>,
    hl: io::Highlighter,
    /// Every effect Sim has asked for, except the frames.
    pub log: Vec<SimEffect>,
    pub quit: Option<i32>,
    /// Hold back directory reads until `deliver` (TREE-5).
    pub defer: bool,
    pub waiting: Vec<PathBuf>,
    queue: VecDeque<SimEvent>,
    _guards: Vec<TempDir>,
}

/// A directory with `files` in it (see `make`), and a Rig started on `args` in it.
pub fn rig(name: &str, files: &[&str], args: &[&str]) -> Rig {
    rig_sized(name, files, args, (80, 24))
}

pub fn rig_sized(name: &str, files: &[&str], args: &[&str], size: (u16, u16)) -> Rig {
    rig_env(name, files, args, size, |_| {})
}

/// The time (seconds) a Rig starts at.
pub const NOW: u64 = 1_700_000_000;

/// `rig_sized`, with the chance to change what Sim starts with (the history, the palette).
pub fn rig_env(
    name: &str,
    files: &[&str],
    args: &[&str],
    size: (u16, u16),
    tweak: impl FnOnce(&mut Env),
) -> Rig {
    let (dir, state) = (temp_dir(name), temp_dir(&format!("{name}-state")));
    // SAFE-6: si uses a state directory only if it is private.
    std::fs::set_permissions(&state.0, Permissions::from_mode(0o700)).unwrap();
    for f in files {
        // `name=content` makes a file with that content; a bare name holds its own name.
        match f.split_once('=') {
            Some((n, c)) => {
                std::fs::create_dir_all(dir.0.join(n).parent().unwrap()).unwrap();
                std::fs::write(dir.0.join(n), c).unwrap();
            }
            None => make(&dir.0, &[f]),
        }
    }
    let (cwd, state_dir) = (dir.0.clone(), state.0.clone());
    let args: Vec<String> = args.iter().map(ToString::to_string).collect();
    let s = start(&args, &cwd, |f| cwd.join(f).exists(), |d| d.is_dir()).unwrap();
    // (the tests start with Tab learned: the hint of KEY-6 has its own tests)
    let mut env = Env { cwd: cwd.clone(), size, now: NOW, tab_uses: 3, ..Env::default() };
    tweak(&mut env);
    let (sim, fx) = Sim::new(s, env);
    let blank = Frame {
        width: 0,
        height: 0,
        rows: vec![],
        cursor: (0, 0),
        shape: sim_core::core::view::Shape::Block,
        pal: None,
    };
    let mut r = Rig {
        shot: Screen {
            frame: blank.clone(),
            looks: vec![],
            paints: vec![],
            syn: Default::default(),
            layers: vec![],
        },
        screen: blank,
        jobs: vec![],
        history: None,
        hold_spans: false,
        held: vec![],
        hold_grep: false,
        grep_held: vec![],
        rg: None,
        hl: io::Highlighter::default(),
        sim,
        cwd,
        state: state_dir,
        log: vec![],
        quit: None,
        defer: false,
        waiting: vec![],
        queue: VecDeque::new(),
        _guards: vec![dir, state],
    };
    r.exec(fx);
    r
}

impl Rig {
    fn exec(&mut self, fx: Vec<SimEffect>) {
        let mut todo: VecDeque<SimEffect> = fx.into();
        loop {
            while let Some(e) = todo.pop_front() {
                match e {
                    SimEffect::Render(s) => {
                        // ICON-1: without icons, every frame of every test is free of them
                        let rows = s.frame.rows.iter().map(|r| r.text.as_str());
                        let lit = rows.filter_map(private_use).next().filter(|_| !self.sim.icons);
                        assert_eq!(
                            lit, None,
                            "a frame draws a private-use code point without icons"
                        );
                        (self.screen, self.shot) = (s.frame.clone(), s)
                    }
                    SimEffect::Files => {
                        self.jobs.push("files");
                        self.queue.push_back(SimEvent::Files(io::files(&self.sim.root)));
                    }
                    SimEffect::Status => {
                        self.jobs.push("status");
                        self.queue.push_back(SimEvent::Status(io::status(&self.sim.root)));
                    }
                    SimEffect::Changes(untracked) => {
                        self.jobs.push("changes");
                        self.queue
                            .push_back(SimEvent::Changes(io::changes(&self.sim.root, &untracked)));
                    }
                    SimEffect::Show(path) => {
                        self.jobs.push("show");
                        let blob = io::show_head(&self.sim.root, &path);
                        self.queue.push_back(SimEvent::Shown(path, blob));
                    }
                    SimEffect::History(h) => (self.jobs.push("history"), self.history = Some(h)).0,
                    SimEffect::Highlight(req) if self.hold_spans => self.held.push(req),
                    SimEffect::Highlight(req) => {
                        self.jobs.push("highlight");
                        self.queue.push_back(self.hl.run(req));
                    }
                    SimEffect::Grep(req) if self.hold_grep => self.grep_held.push(req),
                    SimEffect::Grep(req) => {
                        self.jobs.push("grep");
                        self.grep(req);
                    }
                    SimEffect::Diff(req) => {
                        self.jobs.push("diff");
                        let answer = io::diff(&self.cwd, &req);
                        self.queue.push_back(SimEvent::Diff(req.id, answer, req.lines));
                    }
                    SimEffect::Preview(path) => {
                        self.jobs.push("preview");
                        self.queue.push_back(io::preview(&self.sim.root, &path));
                    }
                    SimEffect::Read(dir) if self.defer => self.waiting.push(dir),
                    SimEffect::Read(dir) => {
                        let r = io::read_dir(&dir);
                        self.queue.push_back(SimEvent::Listing(dir, r));
                    }
                    SimEffect::Open(id, path) => {
                        let at = self.cwd.join(&path);
                        self.queue.push_back(io::open(Some(&self.state), id, &path, &at));
                    }
                    SimEffect::App(id, Effect::Write { path, bytes, expect_hash }) => {
                        let at = self.cwd.join(&path).to_string_lossy().into_owned();
                        let (done, seq) = (
                            sim_core::io::file::save(&at, &bytes, expect_hash, Some(&self.state)),
                            sim_core::io::file::seq(),
                        );
                        let ev = match done {
                            Ok(sim_core::io::file::Saved::Done(h, w)) => {
                                Event::Saved(Ok((h, w)), seq)
                            }
                            Ok(sim_core::io::file::Saved::Conflict(d)) => {
                                Event::SaveConflict(d, far(), seq)
                            }
                            Err(e) => Event::Saved(Err(e), seq),
                        };
                        self.queue.push_back(SimEvent::App(id, ev));
                    }
                    SimEffect::App(id, Effect::Keep { path, bytes, ext }) => {
                        let at = sim_core::io::journal::keep(&self.state, &path, &bytes, ext);
                        let at = at.map(|p| p.display().to_string()).map_err(|e| e.to_string());
                        self.queue.push_back(SimEvent::App(id, Event::Kept(at)));
                    }
                    SimEffect::App(id, Effect::Open(p)) => {
                        let seq = sim_core::io::file::seq();
                        let read = sim_core::io::file::read(&self.cwd.join(&p).to_string_lossy());
                        let ev = Event::Opened(p.clone(), read, seq);
                        self.queue.push_back(SimEvent::App(id, ev));
                    }
                    SimEffect::Quit(c) => {
                        self.quit.get_or_insert(c);
                        self.log.push(SimEffect::Quit(c));
                    }
                    e => self.log.push(e),
                }
            }
            let Some(ev) = self.queue.pop_front() else { return };
            todo.extend(self.sim.update(ev));
        }
    }

    /// The shell's search thread, run at once: its batches are queued as events.
    fn grep(&mut self, req: Request) {
        let mut batches = vec![];
        io::grep(&req, self.rg.as_deref(), &|| true, &mut |b| batches.push(SimEvent::Grep(b)));
        self.queue.extend(batches);
    }

    /// Delivers the searches held back by `hold_grep`.
    pub fn deliver_grep(&mut self) {
        for req in take(&mut self.grep_held) {
            self.grep(req);
        }
        self.exec(vec![]);
    }

    pub fn send(&mut self, ev: SimEvent) {
        let fx = self.sim.update(ev);
        self.exec(fx);
    }

    /// Types keys (si's notation).
    pub fn keys(&mut self, s: &str) -> &mut Rig {
        // `<S-Tab>` and `<F1>` are events of sim's own: the core's `Key` has neither
        let mut rest = s;
        while !rest.is_empty() {
            let cut =
                ["<S-Tab>", "<F1>"].iter().filter_map(|t| rest.find(t).map(|i| (i, *t))).min();
            let (now, special, tail) = match cut {
                Some((i, t)) => (&rest[..i], Some(t), &rest[i + t.len()..]),
                None => (rest, None, ""),
            };
            for k in keys(now) {
                self.send(SimEvent::Term(Event::Key(k)));
            }
            match special {
                Some("<S-Tab>") => self.send(SimEvent::BackTab),
                Some(_) => self.send(SimEvent::Help),
                None => {}
            }
            rest = tail;
        }
        self
    }

    /// The pause (KEY-3) that the last key asked for is over: which-key opens if its prefix is still typed.
    pub fn pause(&mut self) -> &mut Rig {
        let n = self
            .log
            .iter()
            .rev()
            .find_map(|e| if let SimEffect::Pause(n) = e { Some(*n) } else { None });
        self.send(SimEvent::Pause(n.unwrap_or(0)));
        self
    }

    /// Delivers the highlighter's requests held back by `hold_spans`.
    pub fn deliver_spans(&mut self) {
        for req in take(&mut self.held) {
            self.jobs.push("highlight");
            let ev = self.hl.run(req);
            self.send(ev);
        }
    }

    /// Delivers the directory reads held back by `defer`.
    pub fn deliver(&mut self) {
        for dir in std::mem::take(&mut self.waiting) {
            let r = io::read_dir(&dir);
            self.send(SimEvent::Listing(dir, r));
        }
    }

    /// The mouse at (`col`, `row`) of the screen, 0-based (PTR-1).
    pub fn mouse(&mut self, kind: MouseEventKind, col: u16, row: u16) -> &mut Rig {
        let m = MouseEvent { kind, column: col, row, modifiers: KeyModifiers::NONE };
        self.send(SimEvent::Mouse(m));
        self
    }

    /// A click with the left button: the press, then the release.
    pub fn click(&mut self, col: u16, row: u16) -> &mut Rig {
        self.mouse(MouseEventKind::Down(MouseButton::Left), col, row);
        self.mouse(MouseEventKind::Up(MouseButton::Left), col, row)
    }

    /// One notch of the wheel.
    pub fn wheel(&mut self, up: bool, col: u16, row: u16) -> &mut Rig {
        let kind = if up { MouseEventKind::ScrollUp } else { MouseEventKind::ScrollDown };
        self.mouse(kind, col, row)
    }

    pub fn app(&self) -> &App {
        &self.sim.bufs[self.sim.cur].app
    }

    pub fn text(&self) -> String {
        self.app().buf.text.to_string()
    }

    /// The names of the open files in order, as the status line shows them.
    pub fn open_files(&self) -> Vec<String> {
        self.sim.bufs.iter().map(|b| b.app.path.clone().unwrap_or_default()).collect()
    }

    pub fn status(&self) -> &str {
        &self.screen.rows.last().unwrap().text
    }

    /// The text in columns `from..to` of the screen's body rows, without trailing blanks.
    fn cols(&self, from: usize, to: usize) -> Vec<String> {
        let body = self.screen.rows.len() - 1;
        let cut = |text: &str| {
            let (mut col, mut out) = (0, String::new());
            for c in text.chars() {
                let w = c.width().unwrap_or(0);
                if col >= from && col + w <= to {
                    out.push(c);
                }
                col += w;
            }
            out.trim_end().to_string()
        };
        self.screen.rows[..body].iter().map(|r| cut(&r.text)).collect()
    }

    /// The panel's rows, without the blank ones at the bottom.
    pub fn panel(&self) -> Vec<String> {
        let (pw, _) = self.sim.widths();
        let mut rows = self.cols(0, usize::from(pw));
        while rows.last().is_some_and(String::is_empty) {
            rows.pop();
        }
        rows
    }

    /// The editor's rows, without the blank ones at the bottom.
    pub fn editor(&self) -> Vec<String> {
        let (pw, ew) = self.sim.widths();
        let from = if pw > 0 { usize::from(pw) + 1 } else { 0 };
        let mut rows = self.cols(from, from + usize::from(ew));
        while rows.last().is_some_and(String::is_empty) {
            rows.pop();
        }
        rows
    }

    /// PICK-1: where the picker's window is, if it is open and drawn.
    pub fn win(&self) -> Option<sim::pick::Win> {
        self.sim.pick_win()
    }

    /// The text of a rectangle of the screen (the body rows), one string for each of its rows.
    pub fn rect(&self, r: sim::pick::Rect) -> Vec<String> {
        let rows = self.cols(usize::from(r.x), usize::from(r.x) + usize::from(r.w));
        rows[usize::from(r.y)..usize::from(r.y) + usize::from(r.h)].to_vec()
    }

    /// PICK-1: the rows of the picker's window, border and all.
    pub fn float(&self) -> Vec<String> {
        let Some(w) = self.win() else { return vec![] };
        self.rect(sim::pick::Rect { x: w.x, y: w.y, w: w.w, h: w.h })
    }

    /// PICK-1: the query in the prompt (after the `❯ `).
    pub fn query(&self) -> String {
        let Some(w) = self.win() else { return String::new() };
        let text = self.rect(w.prompt).pop().unwrap_or_default();
        text.strip_prefix(" ❯ ").unwrap_or("").trim_end().to_string()
    }

    /// PICK-3: the rows of the list, the selected one starting with `▎`, without the blank ones
    /// at the bottom.
    pub fn list(&self) -> Vec<String> {
        let Some(w) = self.win() else { return vec![] };
        let mut rows = self.rect(w.list);
        while rows.last().is_some_and(String::is_empty) {
            rows.pop();
        }
        rows
    }

    /// PICK-4: the rows of the preview, without the blank ones at the bottom.
    pub fn pane(&self) -> Vec<String> {
        let Some(w) = self.win() else { return vec![] };
        let mut rows = self.rect(w.pane);
        while rows.last().is_some_and(String::is_empty) {
            rows.pop();
        }
        rows
    }

    /// PICK-5: the text in the window's bottom border, between its corner and the first `─` run
    /// (`3 / 12 ⠋`, with the blanks around it taken off).
    pub fn count(&self) -> String {
        let Some(b) = self.float().pop() else { return String::new() };
        let inner = b.trim_start_matches(['╰', '─']);
        inner.split('─').next().unwrap_or("").trim().to_string()
    }

    /// The row of the window's list that is underlined (calm marks the selection so), its index.
    pub fn selected_in_float(&self) -> Option<usize> {
        let w = self.win()?;
        let rows = &self.screen.rows[usize::from(w.list.y)..usize::from(w.list.y + w.list.h)];
        rows.iter().position(|r| !r.underline.is_empty())
    }

    /// The row of the panel that is underlined (the cursor row while the panel has focus).
    pub fn selected(&self) -> Option<String> {
        let row = self.screen.rows.iter().find(|r| r.underline.iter().any(|u| u.start == 0))?;
        let (pw, _) = self.sim.widths();
        self.cols(0, usize::from(pw))
            .into_iter()
            .nth(self.screen.rows.iter().position(|r| std::ptr::eq(r, row))?)
    }
}

/// The columns a text takes.
pub fn width(s: &str) -> usize {
    unicode_width::UnicodeWidthStr::width(s)
}

/// A Frame as text: its rows between bars, each followed by one style symbol per column (`.`
/// Primary, `-` Secondary, `#` Change, `x` Deleted, `_` underlined, blank past the text).
pub fn dump(f: &Frame) -> String {
    dump_rows(f, &[], &[], None)
}

/// A Screen as text: the same, and under a row that has looks one more line with a symbol per
/// column: `K` keyword, `S` string, `N` number or constant, `T` type, `F` function, `c` comment,
/// `B` bold, `g` the selection's gray; `.` where there is none.
pub fn dump_screen(s: &Screen) -> String {
    dump_rows(&s.frame, &s.looks, &[], None)
}

/// A Screen of the looks as text (LOOK-2 to LOOK-7): `dump_screen`'s rows, and under each row that
/// has paints one more line with a symbol per column: backgrounds `g` the tint of a gain, `r` of a
/// loss, `k` the card, `w` the shadow, `A` green and `D` red (words, the bar, the
/// badge); colors of text `l` the lines of a box, `d` the dimmed title, `f` the secondary gray, `a`
/// green and `e` red (the ruler), `u` the card's color on the page, `p` primary; a
/// digit the tens of a fade in percent (`4` is 45); `.` where there is none.
pub fn dump_look(s: &Screen, t: &Tones) -> String {
    dump_rows(&s.frame, &s.looks, &s.paints, Some(t))
}

fn paint_symbols(p: &Paints, t: &Tones, width: u16) -> String {
    (0..width)
        .map(|col| match p.iter().find(|(r, _)| r.contains(&col)).map(|x| x.1) {
            None => '.',
            Some(x) => match (x.bg, x.fg, x.fade) {
                (Some(c), ..) if c == t.gain => 'g',
                (Some(c), ..) if c == t.loss => 'r',
                (Some(c), ..) if c == t.card => 'k',
                (Some(c), ..) if c == t.shadow => 'w',
                (Some(c), ..) if c == t.add => 'A',
                (Some(c), ..) if c == t.del => 'D',
                (_, Some(c), _) if c == t.line => 'l',
                (_, Some(c), _) if c == t.dim => 'd',
                (_, Some(c), _) if c == t.focus => 'f',
                (_, Some(c), _) if c == t.add => 'a',
                (_, Some(c), _) if c == t.del => 'e',
                (_, Some(c), _) if c == t.card => 'u',
                (_, Some(c), _) if c == t.primary => 'p',
                (.., f) if f > 0 => char::from_digit(u32::from(f) / 10, 10).unwrap_or('?'),
                _ => '?',
            },
        })
        .collect()
}

fn dump_rows(f: &Frame, looks: &[Looks], paints: &[Paints], t: Option<&Tones>) -> String {
    let mut out =
        format!("{}x{} cursor {},{} {:?}\n", f.width, f.height, f.cursor.0, f.cursor.1, f.shape);
    for (i, r) in f.rows.iter().enumerate() {
        let mut styles = String::new();
        for c in r.text.chars() {
            for _ in 0..c.width().unwrap_or(0) {
                let col = styles.len() as u16;
                let style =
                    r.spans.iter().find(|(s, _)| s.contains(&col)).map_or(Style::Primary, |s| s.1);
                styles.push(match (r.underline.iter().any(|u| u.contains(&col)), style) {
                    (true, _) => '_',
                    (_, Style::Primary) => '.',
                    (_, Style::Secondary) => '-',
                    (_, Style::Change) => '#',
                    (_, Style::Hidden) => 'h',
                });
            }
        }
        let pad = " ".repeat(f.width as usize - styles.len());
        out += &format!("|{}{pad}|\n|{styles}{pad}|\n", r.text);
        if let Some(l) = looks.get(i).filter(|l| !l.is_empty()) {
            let sym = |col: u16| match l.iter().find(|(r, _)| r.contains(&col)).map(|l| l.1) {
                None => '.',
                Some(Look::Kw) => 'K',
                Some(Look::Str) => 'S',
                Some(Look::Num) => 'N',
                Some(Look::Ty) => 'T',
                Some(Look::Fun) => 'F',
                Some(Look::Com) => 'c',
                Some(Look::Bold) => 'B',
                Some(Look::Sel) => 'g',
                Some(Look::H1) => '1',
                Some(Look::H2) => '2',
                Some(Look::H3) => '3',
                Some(Look::H4) => '4',
                Some(Look::H5) => '5',
                Some(Look::H6) => '6',
                Some(Look::Sub) => 'h',
                Some(Look::Done) => 'd',
                Some(Look::Rule) => 'r',
                Some(Look::Mark) => 'm',
                Some(Look::Tick) => 'k',
                Some(Look::Code) => 'C',
                Some(Look::Em) => 'e',
                Some(Look::Link) => 'L',
                Some(Look::Url) => 'U',
                Some(Look::Bullet) => 'b',
                Some(Look::Quote) => 'q',
                Some(Look::Lang) => 'l',
                Some(Look::Band) => '~',
            };
            let line: String = (0..f.width).map(sym).collect();
            out += &format!("|{line}|\n");
        }
        if let (Some(p), Some(t)) = (paints.get(i).filter(|p| !p.is_empty()), t) {
            out += &format!("|{}|\n", paint_symbols(p, t, f.width));
        }
    }
    out
}

/// What the watcher's thread sends when the file now holds `bytes`: the content with its hash.
pub fn disk(bytes: &[u8]) -> Event {
    let hash = Some(sim_core::core::buffer::hash(bytes));
    Event::Disk {
        bytes: Some(bytes.to_vec()),
        hash,
        deadline: far(),
        seq: sim_core::io::file::seq(),
    }
}

/// A deadline far away: a diff in a test never gives up.
pub fn far() -> std::time::Instant {
    std::time::Instant::now() + std::time::Duration::from_secs(3600)
}

/// Runs si's scripts/pty_bench.py (which spawns a real binary in a pty and plays the terminal on
/// the other side) and returns the key=value pairs it prints; none without python3.
pub fn pty_bench(args: &[&str]) -> Option<std::collections::HashMap<String, f64>> {
    let script = concat!(env!("CARGO_MANIFEST_DIR"), "/../scripts/pty_bench.py");
    let out = match Command::new("python3").arg(script).args(args).output() {
        Ok(out) => out,
        Err(e) => {
            eprintln!("skipped: python3 is needed for scripts/pty_bench.py, but: {e}");
            return None;
        }
    };
    eprint!("{}", String::from_utf8_lossy(&out.stderr));
    assert!(out.status.success(), "pty_bench.py {args:?} failed");
    let out = String::from_utf8(out.stdout).unwrap();
    let pairs = out.lines().filter_map(|l| l.split_once('='));
    Some(pairs.map(|(k, v)| (k.to_string(), v.parse().unwrap())).collect())
}

// ---- benchmarks ----

/// A small deterministic generator (xorshift), so the benchmarks see the same text every run.
pub struct Rng(pub u64);

impl Rng {
    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    pub fn below(&mut self, n: usize) -> usize {
        (self.next() >> 11) as usize % n
    }
}

/// Code-like text of `lines` lines averaging about `avg` bytes: indented statements, comments,
/// blank lines, a few Japanese comments, some long lines and a few very long ones. `foo` and
/// `foobar` appear now and then (in about 1% of the lines), but `foo` followed by digits and
/// `bar`, and `zzqx`, never do.
pub fn code_text(lines: usize, avg: usize) -> String {
    const WORDS: [&str; 22] = [
        "let", "mut", "self", "buf", "text", "line", "cursor", "if", "else", "match", "fn", "pub",
        "Some(x)", "None", "return", "&mut", "u64", "usize", "count", "start", "end", "i + 1",
    ];
    const JA: [&str; 4] = ["行を", "カーソル", "保存して終了", "未保存の変更"];
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let mut out = String::with_capacity(lines * (avg + 1));
    for i in 0..lines {
        let kind = rng.below(100);
        let target = match (i % 2500 == 7, kind) {
            (true, _) => 3000,
            (_, 90..=92) => avg * 5,
            _ => avg / 3 + rng.below(avg * 4 / 3),
        };
        let start = out.len();
        if kind < 8 {
            out.push('\n');
            continue;
        }
        out.push_str(&"    ".repeat(rng.below(4)));
        if kind < 14 {
            out.push_str("// ");
        }
        while out.len() - start < target {
            if (14..20).contains(&kind) {
                out.push_str(JA[rng.below(JA.len())]);
            } else {
                out.push_str(WORDS[rng.below(WORDS.len())]);
            }
            out.push([' ', ' ', '.', '(', ')', ';', ','][rng.below(7)]);
            if rng.below(9) == 0 {
                out.push_str(&rng.below(5000).to_string());
            }
            if rng.below(900) == 0 {
                out.push_str(["foo(", "foobar "][rng.below(2)]);
            }
        }
        out.push('\n');
    }
    out
}

/// The p-th percentile (0..=100) of sorted samples.
pub fn percentile<T: Copy>(sorted: &[T], p: usize) -> T {
    sorted[(sorted.len() * p).div_ceil(100).max(1) - 1]
}

/// Benchmarks mean something only on a release build.
#[allow(clippy::assertions_on_constants)]
pub fn release_only() {
    assert!(
        !cfg!(debug_assertions),
        "benchmarks need a release build: cargo test --release -- --ignored bench"
    );
}

/// What is not sim's own in the bytes a Painter wrote: a control char other than the ESC that
/// starts one of its CSI sequences (`ESC [ params final`), a C1 control, a bidirectional control.
pub fn foreign(bytes: &[u8]) -> Vec<String> {
    let (mut bad, mut i) = (vec![], 0);
    let text = String::from_utf8_lossy(bytes);
    bad.extend(
        text.chars()
            .filter(|c| "\u{202a}\u{202e}\u{2066}\u{2069}\u{200b}\u{feff}".contains(*c))
            .map(|c| format!("{c:?}")),
    );
    while i < bytes.len() {
        match bytes[i] {
            0x1b if bytes.get(i + 1) == Some(&b'[') => {
                let end = bytes[i + 2..].iter().position(|b| (0x40..=0x7e).contains(b));
                let end = i + 2 + end.unwrap_or(bytes.len());
                let params = &bytes[i + 2..end.min(bytes.len())];
                let sgr = bytes.get(end).is_some_and(|f| b"mHKJhlq".contains(f));
                if !sgr || !params.iter().all(|b| b"0123456789;? ".contains(b)) {
                    bad.push(format!(
                        "{:?}",
                        String::from_utf8_lossy(&bytes[i..=end.min(bytes.len() - 1)])
                    ));
                }
                i = end + 1;
            }
            b @ (0..=0x1f | 0x7f) => {
                bad.push(format!("control {b:#x}"));
                i += 1;
            }
            0xc2 if bytes.get(i + 1).is_some_and(|b| (0x80..=0x9f).contains(b)) => {
                bad.push("C1 control".into());
                i += 2;
            }
            _ => i += 1,
        }
    }
    bad
}
