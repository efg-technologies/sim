//! TREE-1, BUF-1, BUF-3, FIND-3, MARK-1, MARK-2, SYN-1, SYN-3, PTR-1, SPEED-1, SPEED-2
//! The event loop: several si Apps, the panel, the threads, running Effects. It is si's main.rs
//! for many files: one save thread and one journal thread for all of them, a watcher and a
//! filter for each, and every job and answer tagged with the id of its file. The file list, git's
//! status, the history and the highlighter each run in the background. The terminal is asked for
//! its colors as si asks (the background at once, the first frame waiting for it at most 3 ms),
//! and the mouse is reported until sim leaves, by any way.
#![forbid(unsafe_code)]
use si::core::app::{Effect, Event};
use si::core::buffer::hash;
use si::io::file::{self, read};
use si::io::watch::watch;
use si::io::{filter, journal, term};
use signal_hook::consts::{SIGHUP, SIGTERM, SIGWINCH};
use sim::color::{self, Input, Syntax};
use sim::io::{self as sio, Job, MERGE};
use sim::paint::{MOUSE_ON, Painter, leave};
use sim::rank::History;
use sim::sim::{Env, Id, Sim, SimEffect, SimEvent, start};
use std::any::Any;
use std::collections::HashMap;
use std::io::{self, IsTerminal, Write};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
use std::sync::mpsc::{self, Sender};
use std::time::{Duration, Instant};
use std::{env, fs, thread};

/// How long a quit waits for the terminal to finish answering (VIS-4).
const DRAIN: Duration = Duration::from_millis(100);
/// Set by a quit that is waiting for the terminal: the input thread then asks it and reports.
static QUITTING: AtomicBool = AtomicBool::new(false);
const USAGE: &str = "usage: sim [+cmd] [dir | file]\n       sim [+cmd] -- file";

/// A watcher, kept alive by holding it (PROP-1).
type Held = Box<dyn Any>;

/// Watches `path` for file `id` (PROP-1), as si's main does for its one file. The deadline of each
/// change is stamped when the watcher reads the file, since the core reads no clock (PROP-7), and
/// its hash is made there too, so that no key waits for it (P3). With `first`, the watcher's thread
/// also reads the file once at the start, ordered before the debounced reads.
fn watching(tx: &Sender<SimEvent>, id: Id, path: &str, first: bool) -> Option<(Id, Held)> {
    let t = tx.clone();
    let on = move |seq: u64, bytes: Option<Vec<u8>>| {
        let hash = bytes.as_deref().map(hash);
        let ev = Event::Disk { bytes, hash, deadline: Instant::now() + MERGE, seq };
        drop(t.send(SimEvent::App(id, ev)))
    };
    watch(&file::resolve(path), on, first)
        .inspect_err(|e| drop(tx.send(SimEvent::Notice(format!("cannot watch {path}: {e}")))))
        .ok()
        .map(|w| (id, Box::new(w) as Held))
}

/// Tells file `id`'s App that its filter ended (PROP-6). The diff of its result gives up after
/// `MERGE` (PROP-7).
fn filter_done(tx: &Sender<SimEvent>, id: Id, status: Option<i32>, out: Vec<u8>, err: Vec<u8>) {
    let ev = Event::FilterDone(status, out, err, Instant::now() + MERGE);
    drop(tx.send(SimEvent::App(id, ev)));
}

/// A hook for the E2E test that the terminal is restored after a panic. Debug builds only.
#[allow(clippy::panic)]
fn e2e_panic() {
    if cfg!(debug_assertions) && env::var_os("SIM_E2E_PANIC").is_some() {
        panic!("SIM_E2E_PANIC is set");
    }
}

fn main() -> ExitCode {
    // (`env::args` panics on an argument that is not UTF-8; every path in sim is text)
    let args: Option<Vec<String>> = env::args_os().skip(1).map(|a| a.into_string().ok()).collect();
    let Some(args) = args else {
        eprintln!("sim: a file name that is not valid UTF-8 cannot be opened\n{USAGE}");
        return ExitCode::from(2);
    };
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
        syn: Syntax::new(truecolor),
        hook: env::var("SI_ON_NORMAL").ok().filter(|h| !h.is_empty()),
    };
    let (mut sim, mut fx) = Sim::new(s, env);

    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        leave(&mut io::stdout());
        prev(info);
    }));
    // VIS-4: the background is asked for right after the screen is switched (si's `enter`), the
    // mouse is reported from here on (PTR-1).
    let entered = term::enter(&mut io::stdout(), true);
    let mouse = |out: &mut io::Stdout| out.write_all(MOUSE_ON).and_then(|()| out.flush());
    if entered.and_then(|()| mouse(&mut io::stdout())).is_err() {
        leave(&mut io::stdout());
        eprintln!("sim: cannot set up the terminal");
        return ExitCode::FAILURE;
    }
    let asked = Instant::now(); // the query for the background color went out with `enter`
    let (t, (settled, drained)) = (tx.clone(), mpsc::channel());
    thread::spawn(move || {
        let mut input = Input::new(truecolor); // the terminal's replies arrive as keys
        while let Ok(ev) = crossterm::event::read() {
            if QUITTING.load(SeqCst) {
                // A terminal answers in order, and the answer to this one, which crossterm keeps from
                // us and waits for, comes after the others': when it is in, nothing is left to read
                drop(crossterm::terminal::supports_keyboard_enhancement());
                let _ = settled.send(());
                return;
            }
            if input.feed(ev).into_iter().any(|e| t.send(e).is_err()) {
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
    let (mut pgids, mut waiting) = (HashMap::<Id, u32>::new(), true);
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
                    SimEffect::Close(id) => {
                        drop(watchers.remove(&id));
                        pgids.remove(&id).into_iter().for_each(filter::cancel); // nobody waits for it
                    }
                    SimEffect::Hold(id, key, pid) => drop(jobs.send(Job::Hold(id, key, pid))),
                    SimEffect::App(id, e) => match e {
                        Effect::Write { path, bytes, expect_hash } => {
                            drop(saves.send((id, path, bytes, expect_hash)))
                        }
                        Effect::Open(path) => {
                            let seq = file::seq(); // taken before the read starts (PROP-1)
                            let opened = Event::Opened(path.clone(), read(&path), seq);
                            local.push(SimEvent::App(id, opened));
                            drop(jobs.send(Job::App(id, Effect::Open(path))));
                        }
                        Effect::Watch(path) => {
                            drop(watchers.remove(&id)); // the old watcher's thread ends before the new one starts
                            // Its first read is ordered after everything sent so far (see si's main).
                            watchers.extend(watching(&tx, id, &path, true));
                        }
                        Effect::Clipboard(text) => {
                            drop(painter.write(&term::clipboard(&text, tmux)))
                        }
                        Effect::Snapshot { .. } | Effect::Unsnapshot | Effect::Remember { .. } => {
                            drop(jobs.send(Job::App(id, e)))
                        }
                        Effect::Spawn { cmd, input } => {
                            let t = tx.clone();
                            let done = move |o: filter::Output| {
                                filter_done(&t, id, o.status, o.stdout, o.stderr)
                            };
                            let started = filter::spawn(&cmd, input, done).inspect_err(|e| {
                                let why = format!("cannot run: {e}").into_bytes();
                                filter_done(&tx, id, Some(127), vec![], why)
                            });
                            pgids.extend(started.ok().map(|g| (id, g)));
                        }
                        // `kill` waits for a process: not on this thread.
                        Effect::Kill => {
                            let g = pgids.get(&id).copied();
                            drop(g.map(|g| thread::spawn(move || filter::cancel(g))))
                        }
                        Effect::Hook(cmd) => drop(filter::spawn(&cmd, vec![], |_| {})),
                        Effect::Render(_) | Effect::Quit(_) => {}
                    },
                    SimEffect::Quit(code) => {
                        if sim.asking() {
                            // An answer may still be on its way: the input thread is woken (as a
                            // resize would) to wait for it, since a late one would be typed into the shell
                            QUITTING.store(true, SeqCst);
                            drop(signal_hook::low_level::raise(SIGWINCH));
                            let _ = drained.recv_timeout(DRAIN);
                        }
                        leave(&mut io::stdout());
                        pgids.values().for_each(|&g| filter::cancel(g)); // a filter nobody waits for
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
        // VIS-4: the first frame waits at most 3 ms for the terminal's color, which it gets as it is
        // drawn. The wait overlaps the work above. What came meanwhile is the Sim's too.
        if std::mem::take(&mut waiting) {
            let mut early = vec![];
            while let Some(left) =
                (asked + Duration::from_millis(3)).checked_duration_since(Instant::now())
                && let Ok(ev) = rx.recv_timeout(left)
            {
                let bg = matches!(ev, SimEvent::Term(Event::Bg(_)));
                early.push(ev);
                if bg {
                    break;
                }
            }
            if !early.is_empty() {
                fx = early.into_iter().flat_map(|ev| sim.update(ev)).collect();
                continue; // the frame of this round is made again with them in
            }
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
        fx = vec![];
        for ev in [ev].into_iter().chain(rx.try_iter()) {
            if let SimEvent::App(id, Event::FilterDone { .. }) = &ev {
                pgids.remove(id); // its group is gone: the id could be somebody else's
            }
            fx.extend(sim.update(ev));
        }
    }
}
