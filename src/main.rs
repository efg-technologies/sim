//! CLI-4, CLI-6, VIS-4, SAFE-6, SAFE-7, DISK-1, FILT-1, VI-7, VI-8, VI-9, VI-11, PERF-1, PERF-5
//! The event loop, and running Effects.
use notify::RecommendedWatcher;
use si::core::app::{App, Effect, Event};
use si::core::cli::parse;
use si::core::color;
use si::io::file::{self, Saved, read};
use si::io::filter;
use si::io::journal::{self, NO_STATE, Snapshot};
use si::io::term::{self, Term};
use si::io::watch::watch;
use signal_hook::consts::{SIGHUP, SIGTERM, SIGWINCH};
use std::io::{self, IsTerminal};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use std::{env, thread};

/// How long a diff may take before it gives up (DISK-5).
const MERGE: Duration = Duration::from_millis(50);
/// How long a quit waits for the terminal to finish answering (VIS-4).
const DRAIN: Duration = Duration::from_millis(100);
/// Set by a quit that is waiting for the terminal: the input thread then asks it and reports.
static QUITTING: AtomicBool = AtomicBool::new(false);
const USAGE: &str = "usage: si [+cmd] [file]\n       si [+cmd] -- file";

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

/// The journal key of a buffer (SAFE-6): the canonical path of its file, `""` without one.
fn key(path: Option<&str>) -> String {
    path.map_or(String::new(), |p| file::resolve(p).to_string_lossy().into_owned())
}

/// Tells App the filter ended (FILT-2). The diff of its result gives up after `MERGE` (DISK-5).
fn filter_done(tx: &Sender<Event>, status: Option<i32>, out: Vec<u8>, err: Vec<u8>) {
    drop(tx.send(Event::FilterDone(status, out, err, Instant::now() + MERGE)));
}

/// A watcher, and the switch that silences it (DISK-1).
type Watching = (RecommendedWatcher, Arc<AtomicBool>);

/// Watches `path` (DISK-1) from now on. The deadline of each change is stamped when the watcher
/// reads the file, since the core reads no clock (DISK-5). With `first`, the watcher thread also
/// reads the file once at the start (never the main thread, P3).
fn watching(tx: &Sender<Event>, path: &str, first: bool) -> Option<Watching> {
    let (t, live) = (tx.clone(), Arc::new(AtomicBool::new(true)));
    let on_ = live.clone();
    // The hash is made here, on the watcher's thread, so that no key waits for it (P3).
    let on = move |seq: u64, bytes: Option<Vec<u8>>| {
        let hash = bytes.as_deref().map(si::core::buffer::hash);
        let ev = Event::Disk { bytes, hash, deadline: Instant::now() + MERGE, seq };
        on_.load(SeqCst).then(|| drop(t.send(ev)));
    };
    watch(&file::resolve(path), on, first)
        .inspect_err(|e| drop(tx.send(Event::Notice(format!("cannot watch {path}: {e}")))))
        .map(|w| (w, live))
        .ok()
}

/// The journal thread (SAFE-6) runs Snapshots and Unsnapshots in order, so no key waits for a
/// disk. `held` are the snapshots to delete once ours replaces them, or when we are done. The
/// receiver reports `Disconnected` when the thread has finished what was sent to `Sender`. The
/// first snapshot that cannot be written is told to the user through `tell`.
fn journal_thread(
    state: Option<PathBuf>,
    mut held: Vec<(String, u32)>,
    tell: Sender<Event>,
) -> (Sender<Effect>, Receiver<()>) {
    let (jobs, rx) = mpsc::channel();
    let (done, finished) = mpsc::channel();
    thread::spawn(move || {
        let (pid, _done) = (std::process::id(), done);
        let rm = |h: &(String, u32)| state.iter().for_each(|d| journal::remove(d, &h.0, h.1));
        let mut told = false;
        for job in rx {
            if let Effect::Remember { path, line, col } = job {
                // Read again, so that another session's places are kept (CLI-6).
                for d in state.iter() {
                    let mut all = journal::places(d);
                    journal::touch(&mut all, key(Some(&path)), line, col);
                    drop(journal::save_places(d, &all));
                }
            } else if let Effect::Open(path) = job {
                // The place of the file `:e` opens, asked after the place of the one it leaves.
                let at = state.as_deref().and_then(|d| journal::place(d, &key(Some(&path))));
                at.into_iter().for_each(|(l, c)| drop(tell.send(Event::Place(path.clone(), l, c))));
            } else if let Effect::Keep { path, bytes, ext } = job {
                // SAFE-7: a `:w!` or an `:e!` waits for this copy, so the answer is always sent.
                let at = state.as_deref().ok_or_else(|| io::Error::other(NO_STATE));
                let at = at.and_then(|d| journal::keep(d, &path, &bytes, ext));
                let at = at.map(|p| p.display().to_string()).map_err(|e| file::why(&e));
                drop(tell.send(Event::Kept(at)));
            } else if let Effect::Snapshot { path, base, text } = job {
                let [base, text] = [base, text].map(|r| r.to_string().into_bytes());
                let snap =
                    Snapshot { path: key(path.as_deref()), pid, time_ms: now_ms(), base, text };
                let mine = (snap.path.clone(), pid);
                let wrote = state.as_deref().ok_or_else(|| io::Error::other(NO_STATE));
                let wrote = wrote.and_then(|d| journal::write(d, &snap));
                if wrote.is_ok() {
                    held.iter().filter(|h| **h != mine).for_each(&rm);
                    held = vec![mine];
                }
                if let Err(e) = wrote
                    && !std::mem::replace(&mut told, true)
                {
                    let why = format!("cannot write snapshots: {}", file::why(&e));
                    drop(tell.send(Event::Notice(why)));
                }
            } else {
                held.drain(..).for_each(|h| rm(&h));
            }
        }
    });
    (jobs, finished)
}

/// A hook for the E2E test of CLI-4. It exists only in debug builds.
#[allow(clippy::panic)]
fn e2e_panic() {
    if cfg!(debug_assertions) && env::var_os("SI_E2E_PANIC").is_some() {
        panic!("SI_E2E_PANIC is set");
    }
}

fn main() -> ExitCode {
    // `env::args` panics on a name that is not text, and paths here are text.
    let Ok(args) = env::args_os().skip(1).map(|a| a.into_string()).collect::<Result<Vec<_>, _>>()
    else {
        eprintln!("si: an argument is not valid UTF-8");
        return ExitCode::from(2);
    };
    let Some(cli) = parse(&args, |f| Path::new(f).exists()) else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    let (Ok(size), true) = (crossterm::terminal::size(), io::stdout().is_terminal()) else {
        eprintln!("si: the standard output is not a terminal");
        return ExitCode::FAILURE;
    };
    let (tx, rx) = mpsc::channel();
    // Watching starts before the file is read, so that no change slips in between.
    let mut watcher = cli.file.as_deref().and_then(|p| watching(&tx, p, false));
    let file = cli.file.as_deref().map_or(Ok(None), read);
    let state = journal::state_dir(env::var_os("XDG_STATE_HOME"), env::var_os("HOME"));
    let mut app = App::new(cli.file.clone(), file, size);
    let (mut fx, mut held) = (vec![], vec![]);
    let k = key(cli.file.as_deref());
    let found =
        state.as_deref().and_then(|d| journal::find(d, &k, std::process::id()).map(|s| (d, s)));
    if let Some((dir, s)) = found {
        let file = journal::file(dir, &s.path, s.pid).display().to_string();
        let (v, age, by) =
            (app.buf.version(), now_ms().saturating_sub(s.time_ms), Instant::now() + MERGE);
        fx = app.update(Event::Recover(s.base, s.text, age, file, by));
        held.extend((app.buf.version() != v).then_some((s.path, s.pid))); // used: ours to delete
    }
    app.hook = env::var("SI_ON_NORMAL").ok().filter(|h| !h.is_empty());
    // A `+cmd` or `file:line` says where to go; otherwise it is where the file was closed (CLI-6).
    let at = state.as_deref().and_then(|d| journal::place(d, &k)).filter(|_| cli.keys.is_empty());
    if let (Some(f), Some((l, c))) = (&cli.file, at) {
        fx.extend(app.update(Event::Place(f.clone(), l, c)));
    }

    // The terminal is not touched yet, so a panic in here needs no restoring (CLI-4). VIS-4: it is
    // asked for its background unless the keys given at startup quit.
    fx.extend(app.start(&cli.keys));
    let ask = !fx.iter().any(|e| matches!(e, Effect::Quit(_)));
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        term::leave(&mut io::stdout());
        prev(info);
    }));
    if term::enter(&mut io::stdout(), ask).is_err() {
        eprintln!("si: cannot set up the terminal");
        return ExitCode::FAILURE;
    }
    let asked = Instant::now(); // the query for the background color went out with `enter`
    let (t, (settled, drained)) = (tx.clone(), mpsc::channel());
    thread::spawn(move || {
        let mut reply = term::Reply::default(); // VIS-4: the background color arrives as keys
        while let Ok(ev) = crossterm::event::read() {
            if QUITTING.load(SeqCst) {
                // A terminal answers in order, and the answer to this one, which crossterm keeps from
                // us and waits for, comes after the color's: when it is in, nothing is left to read
                drop(crossterm::terminal::supports_keyboard_enhancement());
                let _ = settled.send(());
                return;
            }
            if reply.feed(ev).into_iter().any(|e| t.send(e).is_err()) {
                return;
            }
        }
        let _ = t.send(Event::Signal(SIGHUP)); // the terminal is gone
    });
    if let Ok(mut signals) = signal_hook::iterator::Signals::new([SIGTERM, SIGHUP]) {
        let t = tx.clone();
        thread::spawn(move || signals.forever().for_each(|s| drop(t.send(Event::Signal(s)))));
    }
    let t = tx.clone(); // a tick every second, however busy the keys keep the loop (SAFE-6)
    thread::spawn(move || {
        for n in 1..=u64::MAX {
            thread::sleep(Duration::from_secs(1));
            drop(t.send(Event::Tick(n * 1000)));
        }
    });
    let (jobs, finished) = journal_thread(state.clone(), held, tx.clone());
    let mut term = Term::new(io::stdout(), env::var_os("TMUX").is_some());
    // VIS-4: the first frame waits at most 3 ms for the terminal's color, which it gets as it is
    // drawn. The wait overlaps the work above, and a quit needs no frame. What else came meanwhile
    // is App's too.
    if ask {
        let (bg, early) = term::wait_bg(&rx, asked + Duration::from_millis(3));
        app.pal = bg.map(color::palette);
        fx.extend(early.into_iter().flat_map(|e| app.update(e)));
    }
    let (mut first, mut pgid) = (true, None::<u32>);
    loop {
        let mut frame = None;
        for e in fx.drain(..) {
            match e {
                Effect::Render(f) => frame = Some(f),
                Effect::Write { path, bytes, expect_hash } => {
                    let (tx, state) = (tx.clone(), state.clone());
                    thread::spawn(move || {
                        let ev = match file::save(&path, &bytes, expect_hash, state.as_deref()) {
                            Ok(Saved::Done(h, warn)) => Event::Saved(Ok((h, warn)), file::seq()),
                            Ok(Saved::Conflict(disk)) => {
                                Event::SaveConflict(disk, Instant::now() + MERGE, file::seq())
                            }
                            Err(e) => Event::Saved(Err(e), file::seq()),
                        };
                        drop(tx.send(ev));
                    });
                }
                Effect::Open(path) => {
                    // The watcher of the file being left goes quiet before the read's number is
                    // taken: what it says later began earlier, and App drops it (DISK-1).
                    watcher.iter().for_each(|(_, live)| live.store(false, SeqCst));
                    let seq = file::seq(); // taken before the read starts
                    drop(tx.send(Event::Opened(path.clone(), read(&path), seq)));
                    drop(jobs.send(Effect::Open(path)));
                }
                Effect::Spawn { cmd, input } => {
                    let t = tx.clone();
                    let done =
                        move |o: filter::Output| filter_done(&t, o.status, o.stdout, o.stderr);
                    pgid = filter::spawn(&cmd, input, done)
                        .inspect_err(|e| {
                            filter_done(
                                &tx,
                                Some(127),
                                vec![],
                                format!("cannot run: {e}").into_bytes(),
                            )
                        })
                        .ok();
                }
                // `kill` waits for a process: not on this thread.
                Effect::Kill => drop(pgid.map(|g| thread::spawn(move || filter::cancel(g)))),
                Effect::Hook(cmd) => drop(filter::spawn(&cmd, vec![], |_| {})),
                Effect::Watch(path) => {
                    drop(watcher.take()); // the old watcher's thread ends before the new one starts
                    // The new watcher's first read is ordered after everything sent so far: the App
                    // asks for it after a save, since what the watcher said meanwhile may be older.
                    watcher = watching(&tx, &path, true);
                }
                Effect::Clipboard(text) => drop(term.clipboard(&text)),
                Effect::Snapshot { .. }
                | Effect::Unsnapshot
                | Effect::Remember { .. }
                | Effect::Keep { .. } => drop(jobs.send(e)),
                Effect::Quit(code) => {
                    if ask && app.pal.is_none() {
                        // The color may still be on its way: the input thread is woken (as a resize
                        // would) to wait for it, since a late answer would be typed into the shell
                        QUITTING.store(true, SeqCst);
                        drop(signal_hook::low_level::raise(SIGWINCH));
                        let _ = drained.recv_timeout(DRAIN);
                    }
                    term::leave(&mut io::stdout());
                    pgid.into_iter().for_each(filter::cancel); // a filter nobody waits for
                    drop(jobs); // the journal thread ends once its queue is done: wait for that
                    let _ = finished.recv_timeout(Duration::from_secs(2));
                    return ExitCode::from(code as u8);
                }
            }
        }
        if let Some(f) = frame
            && term.draw(si::core::view::Frame { pal: app.pal, ..f }).is_err()
        {
            drop(tx.send(Event::Signal(SIGHUP))); // the terminal is gone
        }
        if std::mem::take(&mut first) {
            e2e_panic();
        }
        let Ok(ev) = rx.recv() else { return ExitCode::FAILURE };
        fx = [ev]
            .into_iter()
            .chain(rx.try_iter())
            .flat_map(|ev| {
                if matches!(ev, Event::FilterDone { .. }) {
                    pgid = None; // its group is gone: the id could be somebody else's
                }
                app.update(ev)
            })
            .collect();
    }
}
