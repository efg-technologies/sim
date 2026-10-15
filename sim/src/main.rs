//! TREE-1, BUF-1, BUF-3, FIND-3, MARK-1, MARK-2, SYN-1, SYN-3, SPEED-1, SPEED-2
//! The event loop: several si Apps, the panel, the threads, running Effects. It is si's main.rs
//! for many files: one save thread and one journal thread for all of them, a watcher for each,
//! and every job and answer tagged with the id of its file. The file list, git's status, the
//! history and the highlighter each run in the background. The first frame is drawn before the
//! terminal is asked for its colors.
#![forbid(unsafe_code)]
use crossterm::event::{Event as Ev, KeyCode};
use si::core::app::{Effect, Event};
use si::core::buffer::hash;
use si::io::file::{self, read};
use si::io::journal;
use si::io::term;
use si::io::watch::watch;
use signal_hook::consts::{SIGHUP, SIGTERM};
use sim::color::{self, Palette};
use sim::io::{self as sio, Job, MERGE};
use sim::paint::Painter;
use sim::rank::History;
use sim::sim::{Env, Id, Sim, SimEffect, SimEvent, start};
use std::any::Any;
use std::collections::HashMap;
use std::io::{self, IsTerminal};
use std::process::ExitCode;
use std::sync::mpsc::{self, Sender};
use std::time::{Duration, Instant};
use std::{env, fs, thread};

const USAGE: &str = "usage: sim [+cmd] [dir | file]\n       sim [+cmd] -- file";

/// A watcher, kept alive by holding it (PROP-1).
type Held = Box<dyn Any>;

/// Watches `path` for file `id` (PROP-1), as si's main does for its one file. The deadline of each
/// change is stamped when the watcher reads the file, since the core reads no clock (PROP-7), and
/// its hash is made there too, so that no key waits for it (P3). With `first`, the watcher's thread
/// also reads the file once at the start, ordered before the debounced reads.
fn watching(tx: &Sender<SimEvent>, id: Id, path: &str, first: bool) -> Option<(Id, Held)> {
    let t = tx.clone();
    let on = move |bytes: Option<Vec<u8>>| {
        let hash = bytes.as_deref().map(hash);
        let ev = Event::Disk { bytes, hash, deadline: Instant::now() + MERGE };
        drop(t.send(SimEvent::App(id, ev)))
    };
    watch(&file::resolve(path), on, first)
        .inspect_err(|e| drop(tx.send(SimEvent::Notice(format!("cannot watch {path}: {e}")))))
        .ok()
        .map(|w| (id, Box::new(w) as Held))
}

/// What the terminal sent, as sim's events: si's (`convert_all`: Alt+x is Esc, then x), and
/// Shift-Tab, which si has no key for (crossterm reads both the legacy `CSI Z` and the kitty
/// `CSI 9;2u` as `BackTab`).
fn from_term(ev: Ev) -> Vec<SimEvent> {
    match ev {
        Ev::Key(k) if k.code == KeyCode::BackTab => vec![SimEvent::BackTab],
        ev => term::convert_all(ev).into_iter().map(SimEvent::Term).collect(),
    }
}

/// A hook for the E2E test that the terminal is restored after a panic. Debug builds only.
#[allow(clippy::panic)]
fn e2e_panic() {
    if cfg!(debug_assertions) && env::var_os("SIM_E2E_PANIC").is_some() {
        panic!("SIM_E2E_PANIC is set");
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let cwd = env::current_dir().unwrap_or_default();
    let Some(mut s) = start(&args, &cwd, |f| std::path::Path::new(f).exists(), |d| d.is_dir())
    else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    s.root = fs::canonicalize(&s.root).unwrap_or(s.root);
    let size = crossterm::terminal::size();
    let (Ok(size), true) = (size, io::stdout().is_terminal()) else {
        eprintln!("sim: the standard output is not a terminal");
        return ExitCode::FAILURE;
    };
    let (tx, rx) = mpsc::channel();
    // SAFE-6, as in si: without a state directory (and unless it is private) there are no
    // snapshots and no recovery copies; the first snapshot that cannot be written says so.
    let state = journal::state_dir(env::var_os("XDG_STATE_HOME"), env::var_os("HOME"));
    let history = sio::state_dir(env::var_os("XDG_STATE_HOME"), env::var_os("HOME"));
    let truecolor = matches!(env::var("COLORTERM").as_deref(), Ok("truecolor" | "24bit"));
    let env = Env {
        cwd,
        size,
        now: sio::now_ms() / 1000,
        history: history.as_deref().map_or_else(History::default, sio::load_history),
        pal: Palette::new(truecolor),
    };
    let (mut sim, mut fx) = Sim::new(s, env);

    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        term::leave(&mut io::stdout());
        prev(info);
    }));
    if term::enter(&mut io::stdout()).is_err() {
        eprintln!("sim: cannot set up the terminal");
        return ExitCode::FAILURE;
    }
    let t = tx.clone();
    thread::spawn(move || {
        // The terminal's replies to the color query come in as keys (Alt+`]`, the payload, Alt+`\`):
        // they are put together here for a moment after the start and never reach the App.
        let (mut reply, until) = (None, Instant::now() + Duration::from_secs(2));
        while let Ok(ev) = crossterm::event::read() {
            if let (Ev::Key(k), true) = (&ev, truecolor && Instant::now() < until)
                && let Some(done) = color::feed(&mut reply, k)
            {
                done.into_iter().for_each(|s| drop(t.send(SimEvent::Osc(s))));
                continue;
            }
            if from_term(ev).into_iter().any(|e| t.send(e).is_err()) {
                return;
            }
        }
        let _ = t.send(SimEvent::Signal(SIGHUP)); // the terminal is gone
    });
    if let Ok(mut signals) = signal_hook::iterator::Signals::new([SIGTERM, SIGHUP]) {
        let t = tx.clone();
        thread::spawn(move || signals.forever().for_each(|s| drop(t.send(SimEvent::Signal(s)))));
    }
    let t = tx.clone(); // a tick every second, however busy the keys keep the loop (SAFE-6)
    thread::spawn(move || {
        for n in 1..=u64::MAX {
            thread::sleep(Duration::from_secs(1));
            drop(t.send(SimEvent::Tick(n * 1000)));
        }
    });
    let (jobs, finished) = sio::journal_thread(state.clone(), history, tx.clone());
    let saves = sio::save_thread(state.clone(), tx.clone());
    let highlights = sio::highlight_thread(tx.clone());
    let (r1, r2) = (sim.root.clone(), sim.root.clone());
    let files = sio::worker(tx.clone(), move || SimEvent::Files(sio::files(&r1)));
    let git = sio::worker(tx.clone(), move || SimEvent::Status(sio::status(&r2)));
    let (mut painter, tmux) = (Painter::new(io::stdout()), env::var_os("TMUX").is_some());
    let (mut watchers, mut local, mut first) = (HashMap::<Id, Held>::new(), vec![], true);
    loop {
        let mut frame = None;
        loop {
            for e in fx.drain(..) {
                match e {
                    SimEffect::Render(f) => frame = Some(f),
                    SimEffect::Read(dir) => {
                        let t = tx.clone();
                        thread::spawn(move || {
                            let r = sio::read_dir(&dir);
                            drop(t.send(SimEvent::Listing(dir, r)));
                        });
                    }
                    SimEffect::Files => drop(files.send(())),
                    SimEffect::Status => drop(git.send(())),
                    SimEffect::History(h) => drop(jobs.send(Job::History(h))),
                    SimEffect::Highlight(req) => drop(highlights.send(req)),
                    SimEffect::Open(id, path) => {
                        // Watching starts before the file is read, so that no change slips in between.
                        watchers.extend(watching(&tx, id, &path, false));
                        let at = std::path::Path::new(&path);
                        local.push(sio::open(state.as_deref(), id, &path, at));
                    }
                    SimEffect::Close(id) => drop(watchers.remove(&id)),
                    SimEffect::Hold(id, key, pid) => drop(jobs.send(Job::Hold(id, key, pid))),
                    SimEffect::App(id, e) => match e {
                        Effect::Write { path, bytes, expect_hash } => {
                            drop(saves.send((id, path, bytes, expect_hash)))
                        }
                        Effect::Open(path) => {
                            local.push(SimEvent::App(id, Event::Opened(path.clone(), read(&path))));
                        }
                        Effect::Watch(path) => {
                            drop(watchers.remove(&id)); // the old watcher's thread ends before the new one starts
                            // Its first read is ordered after everything sent so far (see si's main).
                            watchers.extend(watching(&tx, id, &path, true));
                        }
                        Effect::Clipboard(text) => {
                            drop(painter.write(&term::clipboard(&text, tmux)))
                        }
                        Effect::Snapshot { .. } | Effect::Unsnapshot => {
                            drop(jobs.send(Job::Snap(id, e)))
                        }
                        Effect::Render(_) | Effect::Quit(_) => {}
                    },
                    SimEffect::Quit(code) => {
                        term::leave(&mut io::stdout());
                        drop(jobs); // the journal thread ends once its queue is done: wait for that
                        let _ = finished.recv_timeout(Duration::from_secs(2));
                        return ExitCode::from(code as u8);
                    }
                }
            }
            if local.is_empty() {
                break;
            }
            fx = local.drain(..).flat_map(|ev| sim.update(ev)).collect();
        }
        if let Some(f) = frame
            && painter.draw(f).is_err()
        {
            drop(tx.send(SimEvent::Signal(SIGHUP))); // the terminal is gone
        }
        if std::mem::take(&mut first) {
            e2e_panic();
            if truecolor {
                // SYN-1: asked after the first frame; the replies come back as `SimEvent::Osc`.
                drop(painter.write(color::QUERY.as_bytes()));
            }
        }
        let Ok(ev) = rx.recv() else { return ExitCode::FAILURE };
        fx = sim.update(ev);
        while let Ok(ev) = rx.try_recv() {
            fx.extend(sim.update(ev));
        }
    }
}
