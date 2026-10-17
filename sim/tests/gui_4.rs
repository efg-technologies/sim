//! GUI-4: what is done with the Effects `Sim::update` returns (read a directory, open, save and watch
//! a file, ask git, highlight, search, run a filter) is `sim::shell::Shell`, the code the terminal's
//! loop runs, and a window runs it too. An inline shell runs every job at once on the calling thread, so
//! the same input gives the same screen; a threaded one is the binary's.
#![allow(clippy::unwrap_used, clippy::panic)] // helpers; the tests themselves may unwrap
mod common;
use common::{TempDir, keys, temp_dir};
use sim::color::Syntax;
use sim::layout::Skin;
use sim::paint::{LayerKind, Screen};
use sim::shell::{Config, Out, Shell};
use sim::sim::{Env, Sim, SimEvent, start};
use sim_core::core::app::Event;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// The sim and the shell that runs it, in a directory of its own, and what the shell has left so far.
struct Rig {
    sim: Sim,
    shell: Shell,
    cwd: PathBuf,
    frame: Option<Screen>,
    clipboard: Vec<String>,
    quit: Option<i32>,
    _dirs: (TempDir, TempDir),
}

fn config(cwd: &Path, state: &Path) -> Config {
    Config { cwd: cwd.into(), root: cwd.into(), state: Some(state.into()), history: None, rg: None }
}

fn env(cwd: &Path) -> Env {
    Env {
        cwd: cwd.into(),
        size: (100, 24),
        now: 1_700_000_000,
        tab_uses: 3,
        skin: Skin::Depth,
        syn: Syntax::new(true),
        chrome: true,
        ..Env::default()
    }
}

/// A directory with `files` (`name=content`), a private state directory, and `args` to start on.
fn dirs(name: &str, files: &[&str]) -> (TempDir, TempDir) {
    let (dir, state) = (temp_dir(name), temp_dir(&format!("{name}-state")));
    std::fs::set_permissions(&state.0, std::fs::Permissions::from_mode(0o700)).unwrap();
    for f in files {
        let (n, c) = f.split_once('=').unwrap();
        std::fs::write(dir.0.join(n), c).unwrap();
    }
    (dir, state)
}

impl Rig {
    fn take(&mut self, out: Out) {
        self.frame = out.frame.or(self.frame.take());
        self.clipboard.extend(out.clipboard);
        self.quit = out.quit.or(self.quit);
    }

    fn keys(&mut self, s: &str) {
        for k in keys(s) {
            let out = self.shell.send(&mut self.sim, SimEvent::Term(Event::Key(k)));
            self.take(out);
        }
    }

    fn rows(&self) -> Vec<&str> {
        self.frame.as_ref().unwrap().frame.rows.iter().map(|r| r.text.as_str()).collect()
    }

    fn shows(&self, what: &str) -> bool {
        self.rows().iter().any(|r| r.contains(what))
    }

    fn kinds(&self) -> Vec<LayerKind> {
        self.frame.as_ref().unwrap().layers.iter().map(|l| l.kind).collect()
    }
}

/// An inline shell started on `args` (names relative to the directory).
fn inline(name: &str, files: &[&str], args: &[&str]) -> Rig {
    let dirs = dirs(name, files);
    let (cwd, state) = (dirs.0.0.clone(), dirs.1.0.clone());
    let args: Vec<String> = args.iter().map(ToString::to_string).collect();
    let s = start(&args, &cwd, |f| cwd.join(f).exists(), |d| d.is_dir()).unwrap();
    let (mut sim, fx) = Sim::new(s, env(&cwd));
    let mut shell = Shell::inline(config(&cwd, &state));
    let out = shell.run(&mut sim, fx);
    let mut r = Rig { sim, shell, cwd, frame: None, clipboard: vec![], quit: None, _dirs: dirs };
    r.take(out);
    // a window has no terminal to ask: it tells the colors (GUI-6)
    let out = r.shell.send(&mut r.sim, SimEvent::Colors((0x1e, 0x1e, 0x1e), [None; 8]));
    r.take(out);
    r
}

#[test]
fn gui_4_an_inline_shell_opens_the_file_and_draws_the_screen() {
    let r = inline("g4-open", &["a.txt=hello\nworld\n"], &["a.txt"]);
    assert!(r.shows("hello") && r.shows("world"), "{:?}", r.rows());
    assert!(r.kinds().iter().any(|k| matches!(k, LayerKind::Text { .. })));
    assert_eq!(r.quit, None);
}

#[test]
fn gui_4_a_save_is_written_at_once_and_answered() {
    let mut r = inline("g4-save", &["a.txt=hello\n"], &["a.txt"]);
    r.keys("iX<Esc>:w\n");
    assert_eq!(std::fs::read_to_string(r.cwd.join("a.txt")).unwrap(), "Xhello\n");
    // (the answer came back: the buffer is not modified any more)
    assert!(!r.sim.bufs[0].app.buf.dirty(), "saved");
    assert!(r.shows("Xhello"));
}

#[test]
fn gui_4_a_change_from_outside_is_read_when_the_window_says_the_file_changed() {
    let mut r = inline("g4-disk", &["a.txt=hello\n"], &["a.txt"]);
    std::fs::write(r.cwd.join("a.txt"), "from outside\n").unwrap();
    let id = r.sim.bufs[0].id;
    let out = r.shell.disk_changed(&mut r.sim, id, "a.txt");
    r.take(out);
    assert!(r.shows("from outside"), "{:?}", r.rows());
}

#[test]
fn gui_4_a_pause_on_a_prefix_is_over_at_once_so_which_key_opens() {
    let mut r = inline("g4-pause", &["a.txt=hello\n"], &["a.txt"]);
    assert!(!r.kinds().iter().any(|k| matches!(k, LayerKind::Float { .. })));
    r.keys(" ");
    assert!(r.kinds().contains(&LayerKind::Float { modal: false }), "{:?}", r.kinds());
    r.keys("<Esc>");
    assert!(!r.kinds().iter().any(|k| matches!(k, LayerKind::Float { .. })));
}

#[test]
fn gui_4_what_is_yanked_goes_to_the_clipboard_for_the_window_to_put_there() {
    let mut r = inline("g4-clip", &["a.txt=hello\nworld\n"], &["a.txt"]);
    r.keys("yy");
    assert_eq!(r.clipboard, ["hello\n"]);
    r.keys("jyl");
    assert_eq!(r.clipboard, ["hello\n", "w"]);
}

#[test]
fn gui_4_the_files_and_the_directories_are_read_here_too() {
    let mut r = inline("g4-files", &["a.txt=1\n", "b.txt=2\n"], &["a.txt"]);
    r.keys("<C-p>");
    assert!(r.shows("b.txt"), "the picker lists the files the shell found: {:?}", r.rows());
    r.keys("<Esc><C-n>");
    assert!(r.shows("b.txt"), "and so does the tree: {:?}", r.rows());
}

#[test]
fn gui_4_a_filter_runs_to_its_end_and_its_output_replaces_the_lines() {
    let mut r = inline("g4-filter", &["a.txt=hello\n"], &["a.txt"]);
    r.keys(":%!tr a-z A-Z\n");
    assert!(r.shows("HELLO"), "{:?}", r.rows());
}

#[test]
fn gui_4_a_quit_is_said_in_out_with_its_code() {
    let mut r = inline("g4-quit", &["a.txt=hello\n"], &["a.txt"]);
    assert_eq!(r.quit, None);
    r.keys(":q\n");
    assert_eq!(r.quit, Some(0));
    let mut r = inline("g4-cq", &["a.txt=hello\n"], &["a.txt"]);
    r.keys(":cq\n");
    assert_eq!(r.quit, Some(1), "an error exit has its own code");
}

#[test]
fn gui_4_an_inline_shell_is_deterministic_the_same_keys_give_the_same_screen() {
    let run = |name: &str| {
        let mut r = inline(name, &["a.txt=one\ntwo\nthree\n"], &["a.txt"]);
        r.keys("jyyp<C-p>b<Esc> ");
        let f = r.frame.as_ref().unwrap();
        (
            f.frame.rows.iter().map(|r| r.text.clone()).collect::<Vec<_>>(),
            f.layers.clone(),
            f.paints.clone(),
        )
    };
    // (the directories differ only in their names, which are not on the screen)
    assert_eq!(run("g4-det-1"), run("g4-det-2"));
}

/// The binary's shell: its jobs answer on their threads, through a channel, as the terminal's loop gets them.
struct Threaded {
    sim: Sim,
    shell: Shell,
    rx: mpsc::Receiver<SimEvent>,
    frame: Option<Screen>,
    files: bool,
    status: bool,
}

impl Threaded {
    fn send(&mut self, ev: SimEvent) {
        self.frame = self.shell.send(&mut self.sim, ev).frame.or(self.frame.take());
    }

    /// Takes in what the threads answer until `until` holds; false if it did not in ten seconds.
    fn pump(&mut self, until: impl Fn(&Threaded) -> bool) -> bool {
        let end = Instant::now() + Duration::from_secs(10);
        while !until(self) && Instant::now() < end {
            let Ok(ev) = self.rx.recv_timeout(Duration::from_millis(50)) else { continue };
            self.files |= matches!(ev, SimEvent::Files(_));
            self.status |= matches!(ev, SimEvent::Status(_));
            self.send(ev);
        }
        until(self)
    }
}

#[test]
fn gui_4_the_threaded_shell_answers_through_its_channel_and_finishes_in_time() {
    let dirs = dirs("g4-threads", &["a.txt=hello\n", "b.txt=2\n"]);
    let (cwd, state) = (dirs.0.0.clone(), dirs.1.0.clone());
    // (names are absolute: the process is not in the directory)
    let file = cwd.join("a.txt").to_string_lossy().into_owned();
    let s = start(std::slice::from_ref(&file), &cwd, |f| Path::new(f).exists(), |d| d.is_dir())
        .unwrap();
    let (tx, rx) = mpsc::channel();
    let (mut sim, fx) = Sim::new(s, env(&cwd));
    let mut shell = Shell::threads(config(&cwd, &state), tx);
    let frame = shell.run(&mut sim, fx).frame;
    let mut t = Threaded { sim, shell, rx, frame, files: false, status: false };
    // the jobs of the start answer on their threads
    assert!(t.pump(|t| t.files && t.status), "the files and git's status came");
    // a save goes to the save thread and its answer comes through the channel
    for k in keys("iX<Esc>:w\n") {
        t.send(SimEvent::Term(Event::Key(k)));
    }
    let want = cwd.join("a.txt");
    assert!(t.pump(|_| std::fs::read_to_string(&want).is_ok_and(|s| s == "Xhello\n")), "saved");
    assert!(t.pump(|t| !t.sim.bufs[0].app.buf.dirty()), "and the answer came back");
    assert!(t.frame.is_some());
    let at = Instant::now();
    t.shell.finish();
    assert!(
        at.elapsed() < Duration::from_secs(3),
        "the journal's queue is waited for, 2 s at most"
    );
}
