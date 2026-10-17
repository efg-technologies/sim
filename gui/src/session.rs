//! GUI-4, GUI-6
//! sim and its shell, held together: what a window (or an offscreen render) starts, sends
//! events to and reads the screen of. The editor is `sim::sim::Sim`, the jobs are `sim::shell::Shell`;
//! nothing here decides anything about a key or a cell.
use crate::api::*;
use crate::theme::Theme;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};

/// The system clipboard, as far as the window needs it: sim yanks to it (OSC 52 in a terminal) and a
/// paste key reads it (a terminal's bracketed paste).
pub trait Clipboard: Send {
    fn set(&mut self, text: &str);
    fn get(&mut self) -> Option<String>;
}

/// A clipboard that lives in memory (the tests', and the fallback).
#[derive(Default)]
pub struct MemClipboard(pub String);

impl Clipboard for MemClipboard {
    fn set(&mut self, text: &str) {
        self.0 = text.to_string();
    }

    fn get(&mut self) -> Option<String> {
        (!self.0.is_empty()).then(|| self.0.clone())
    }
}

/// How the jobs run: on threads (the window), or at once (the tests and the offscreen renders).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Exec {
    Threads,
    Inline,
}

/// What sim starts with.
#[derive(Clone, Default)]
pub struct Settings {
    /// The journal's directory (snapshots, places); none: nothing is journaled.
    pub state: Option<PathBuf>,
    /// Where the history and the choices are kept; none: nothing is remembered.
    pub history: Option<PathBuf>,
    pub home: Option<PathBuf>,
    pub icons: bool,
    pub size: (u16, u16),
}

pub struct Session {
    pub sim: Sim,
    pub shell: Shell,
    screen: Option<Screen>,
    /// How many screens sim has drawn: a window redraws when it grows.
    pub renders: u64,
    pub quit: Option<i32>,
    pub clipboard: Box<dyn Clipboard>,
}

impl Session {
    /// sim started on `args` (its own command line: `[+cmd] [dir | file]`) in `cwd`, handed the
    /// theme's colors. With `Exec::Threads` the receiver is where the threads' answers arrive; the
    /// caller gives each to `send`. None: the arguments are not sim's.
    pub fn new(
        args: &[String],
        cwd: &Path,
        set: &Settings,
        theme: &Theme,
        exec: Exec,
    ) -> Option<(Session, Option<Receiver<SimEvent>>)> {
        let mut s = start(args, cwd, |f| cwd.join(f).exists(), |d| d.is_dir())?;
        s.root = std::fs::canonicalize(&s.root).unwrap_or(s.root);
        let env = Env {
            cwd: cwd.to_path_buf(),
            size: set.size,
            now: sio::now_ms() / 1000,
            history: set.history.as_deref().map_or_else(History::default, sio::load_history),
            syn: Syntax::new(true),
            skin: Skin::Depth,
            home: set.home.clone(),
            icons: set.icons,
            tab_uses: 3, // (the hint of Tab is for a terminal's new user)
            chrome: true,
            ..Env::default()
        };
        // GIT-1: the directory git's hooks are pointed at is made once, before any git command is built
        set.state.as_deref().iter().for_each(|d| sio::use_no_hooks(d));
        let (mut sim, fx) = Sim::new(s, env);
        let cfg = Config {
            cwd: cwd.to_path_buf(),
            root: sim.root.clone(),
            state: set.state.clone(),
            history: set.history.clone(),
            rg: sio::which_rg(std::env::var_os("PATH")),
        };
        let (shell, rx) = match exec {
            Exec::Inline => (Shell::inline(cfg), None),
            Exec::Threads => {
                let (tx, rx) = mpsc::channel();
                (Shell::threads(cfg, tx), Some(rx))
            }
        };
        let mut me = Session {
            shell,
            screen: None,
            renders: 0,
            quit: None,
            clipboard: Box::new(MemClipboard::default()),
            sim: Sim::default(),
        };
        std::mem::swap(&mut me.sim, &mut sim);
        let out = me.shell.run(&mut me.sim, fx);
        me.apply(out);
        for ev in theme.events() {
            me.send(ev);
        }
        Some((me, rx))
    }

    fn apply(&mut self, out: Out) {
        if let Some(f) = out.frame {
            self.screen = Some(f);
            self.renders += 1;
        }
        out.clipboard.iter().for_each(|t| self.clipboard.set(t));
        self.quit = self.quit.or(out.quit);
    }

    /// Gives sim one event and runs what it asks for (and, inline, what that asks for in turn).
    pub fn send(&mut self, ev: SimEvent) {
        let out = self.shell.send(&mut self.sim, ev);
        self.apply(out);
    }

    /// The file changed outside sim, as its watcher would tell (an inline shell has none): its bytes now.
    pub fn disk_changed(&mut self, id: Id, name: &str) {
        let out = self.shell.disk_changed(&mut self.sim, id, name);
        self.apply(out);
    }

    /// The latest screen sim drew.
    pub fn screen(&self) -> Option<&Screen> {
        self.screen.as_ref()
    }

    /// The end: the journal has what it was sent, the filters are stopped.
    pub fn finish(&mut self) {
        self.shell.finish();
    }
}
