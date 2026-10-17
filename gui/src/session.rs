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
pub struct Settings {
    /// What the system says of the world sim starts in: the privilege, `PATH`, the state directory
    /// (`World::here()` for a window; a tool or a test says its own).
    pub world: World,
    /// `$HOME`, for the root the header shows as `~/...`.
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
    /// caller gives each to `send`. None: the arguments are not sim's. The start itself, with the rules of
    /// plain mode, of the state directory and of git's hooks, is `begin`'s, which the terminal goes
    /// through too (PLAIN-1, GUI-4): nothing here decides them.
    pub fn new(
        args: &[String],
        cwd: &Path,
        set: Settings,
        theme: &Theme,
        exec: Exec,
    ) -> Option<(Session, Option<Receiver<SimEvent>>)> {
        let front = Front {
            size: set.size,
            syn: Syntax::new(true),
            skin: Some(Skin::Depth), // (the window draws depth)
            icons: Some(set.icons),
            tab_uses: Some(3), // (the hint of Tab is for a terminal's new user)
            hook: None,
            home: set.home,
        };
        let Begun { sim, fx, cfg, moved, .. } = begin(arguments(args, cwd)?, cwd, set.world, front);
        let (shell, rx) = match exec {
            Exec::Inline => (Shell::inline(cfg), None),
            Exec::Threads => {
                let (tx, rx) = mpsc::channel();
                signals(tx.clone()); // SAFE-6: as the terminal's loop does
                (Shell::threads(cfg, tx), Some(rx))
            }
        };
        let mut me = Session {
            shell,
            screen: None,
            renders: 0,
            quit: None,
            clipboard: Box::new(MemClipboard::default()),
            sim,
        };
        let out = me.shell.run(&mut me.sim, fx);
        me.apply(out);
        for ev in theme.events() {
            me.send(ev);
        }
        if let Some(e) = moved {
            me.send(SimEvent::Notice(format!("cannot copy the old state directory (si): {e}")));
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
