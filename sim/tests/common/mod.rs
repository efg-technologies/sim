//! Helpers shared by the integration tests.
#![allow(dead_code, clippy::unwrap_used)]
use std::path::{Path, PathBuf};
use std::process::Command;

/// A unique empty directory, removed when dropped.
pub struct TempDir(pub PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub fn temp_dir(name: &str) -> TempDir {
    let d = std::env::temp_dir().join(format!("sim-it-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    TempDir(d)
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

/// A repository in `dir` with these files committed.
pub fn repo(dir: &Path, files: &[&str]) {
    git(dir, &["init", "-q"]);
    make(dir, files);
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", "init"]);
}

/// Compares `actual` with `tests/golden/{name}.txt`; `SIM_BLESS=1` rewrites the file.
pub fn golden(name: &str, actual: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/golden/{name}.txt"));
    if std::env::var_os("SIM_BLESS").is_some() {
        std::fs::write(&path, actual).unwrap();
    }
    let want = std::fs::read_to_string(&path).unwrap_or_default();
    assert_eq!(actual, want, "{name}: differs from {} (SIM_BLESS=1 rewrites it)", path.display());
}

// ---- driving Sim without threads or a terminal ----

use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use si::core::app::{App, Effect, Event, Key};
use si::core::view::{Frame, Style};
use sim::io;
use sim::paint::{Look, Looks, Screen};
use sim::rank::History;
use sim::sim::{Env, Req, Sim, SimEffect, SimEvent, start};
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
    /// The background jobs Sim asked for, by name (`files`, `status`, `history`, `highlight`).
    pub jobs: Vec<&'static str>,
    /// The last history Sim asked to have saved.
    pub history: Option<History>,
    /// Hold back the highlighter's requests until `deliver_spans` (SYN-3).
    pub hold_spans: bool,
    pub held: Vec<Req>,
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
    let mut env = Env { cwd: cwd.clone(), size, now: NOW, ..Env::default() };
    tweak(&mut env);
    let (sim, fx) = Sim::new(s, env);
    let blank = Frame {
        width: 0,
        height: 0,
        rows: vec![],
        cursor: (0, 0),
        shape: si::core::view::Shape::Block,
        pal: None,
    };
    let mut r = Rig {
        shot: Screen { frame: blank.clone(), looks: vec![], syn: Default::default() },
        screen: blank,
        jobs: vec![],
        history: None,
        hold_spans: false,
        held: vec![],
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
                    SimEffect::Render(s) => (self.screen, self.shot) = (s.frame.clone(), s),
                    SimEffect::Files => {
                        self.jobs.push("files");
                        self.queue.push_back(SimEvent::Files(io::files(&self.sim.root)));
                    }
                    SimEffect::Status => {
                        self.jobs.push("status");
                        self.queue.push_back(SimEvent::Status(io::status(&self.sim.root)));
                    }
                    SimEffect::History(h) => (self.jobs.push("history"), self.history = Some(h)).0,
                    SimEffect::Highlight(req) if self.hold_spans => self.held.push(req),
                    SimEffect::Highlight(req) => {
                        self.jobs.push("highlight");
                        self.queue.push_back(self.hl.run(req));
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
                            si::io::file::save(&at, &bytes, expect_hash, Some(&self.state)),
                            si::io::file::seq(),
                        );
                        let ev = match done {
                            Ok(si::io::file::Saved::Done(h, w)) => Event::Saved(Ok((h, w)), seq),
                            Ok(si::io::file::Saved::Conflict(d)) => {
                                Event::SaveConflict(d, far(), seq)
                            }
                            Err(e) => Event::Saved(Err(e), seq),
                        };
                        self.queue.push_back(SimEvent::App(id, ev));
                    }
                    SimEffect::App(id, Effect::Open(p)) => {
                        let seq = si::io::file::seq();
                        let read = si::io::file::read(&self.cwd.join(&p).to_string_lossy());
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

    pub fn send(&mut self, ev: SimEvent) {
        let fx = self.sim.update(ev);
        self.exec(fx);
    }

    /// Types keys (si's notation).
    pub fn keys(&mut self, s: &str) -> &mut Rig {
        for k in keys(s) {
            self.send(SimEvent::Term(Event::Key(k)));
        }
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

    /// The row of the panel that is underlined (the cursor row while the panel has focus).
    pub fn selected(&self) -> Option<String> {
        let row = self.screen.rows.iter().find(|r| r.underline.iter().any(|u| u.start == 0))?;
        let (pw, _) = self.sim.widths();
        self.cols(0, usize::from(pw))
            .into_iter()
            .nth(self.screen.rows.iter().position(|r| std::ptr::eq(r, row))?)
    }
}

/// A Frame as text: its rows between bars, each followed by one style symbol per column (`.`
/// Primary, `-` Secondary, `#` Change, `x` Deleted, `_` underlined, blank past the text).
pub fn dump(f: &Frame) -> String {
    dump_rows(f, &[])
}

/// A Screen as text: the same, and under a row that has looks one more line with a symbol per
/// column: `K` keyword, `S` string, `N` number or constant, `T` type, `F` function, `c` comment,
/// `B` bold, `g` the selection's gray; `.` where there is none.
pub fn dump_screen(s: &Screen) -> String {
    dump_rows(&s.frame, &s.looks)
}

fn dump_rows(f: &Frame, looks: &[Looks]) -> String {
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
                    (_, Style::Deleted) => 'x',
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
            };
            let line: String = (0..f.width).map(sym).collect();
            out += &format!("|{line}|\n");
        }
    }
    out
}

/// What the watcher's thread sends when the file now holds `bytes`: the content with its hash.
pub fn disk(bytes: &[u8]) -> Event {
    let hash = Some(si::core::buffer::hash(bytes));
    Event::Disk { bytes: Some(bytes.to_vec()), hash, deadline: far(), seq: si::io::file::seq() }
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
