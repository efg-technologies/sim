//! CLI-4, SAFE-6, PROP-1, VI-7, VI-8, VI-9, PERF-1, PERF-5
//! The event loop, and running Effects.
use notify::RecommendedWatcher;
use si::core::app::{App, Effect, Event};
use si::core::cli::parse;
use si::io::file::{self, Saved, read};
use si::io::journal::{self, NO_STATE, Snapshot};
use si::io::term::{self, Term};
use si::io::watch::watch;
use signal_hook::consts::{SIGHUP, SIGTERM};
use std::io::{self, IsTerminal};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use std::{env, thread};

/// How long a diff may take before it gives up (PROP-7).
const MERGE: Duration = Duration::from_millis(50);
const USAGE: &str = "usage: si [+cmd] [file]\n       si [+cmd] -- file";

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

/// The journal key of a buffer (SAFE-6): the canonical path of its file, `""` without one.
fn key(path: Option<&str>) -> String {
    path.map_or(String::new(), |p| file::resolve(p).to_string_lossy().into_owned())
}

/// Watches `path` (PROP-1) from now on. The deadline of each change is stamped when the watcher
/// reads the file, since the core reads no clock (PROP-7). With `first`, the watcher thread also
/// reads the file once at the start (never the main thread, P3).
fn watching(tx: &Sender<Event>, path: &str, first: bool) -> Option<RecommendedWatcher> {
    let t = tx.clone();
    // The hash is made here, on the watcher's thread, so that no key waits for it (P3).
    let on = move |bytes: Option<Vec<u8>>| {
        let hash = bytes.as_deref().map(si::core::buffer::hash);
        drop(t.send(Event::Disk { bytes, hash, deadline: Instant::now() + MERGE }));
    };
    watch(&file::resolve(path), on, first)
        .inspect_err(|e| drop(tx.send(Event::Notice(format!("cannot watch {path}: {e}")))))
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
            if let Effect::Snapshot { path, base, text } = job {
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
    let args: Vec<String> = env::args().skip(1).collect();
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
    let found = state.as_deref().and_then(|d| {
        journal::find(d, &key(cli.file.as_deref()), std::process::id()).map(|s| (d, s))
    });
    if let Some((dir, s)) = found {
        let file = journal::file(dir, &s.path, s.pid).display().to_string();
        let (v, age, by) =
            (app.buf.version(), now_ms().saturating_sub(s.time_ms), Instant::now() + MERGE);
        fx = app.update(Event::Recover(s.base, s.text, age, file, by));
        held.extend((app.buf.version() != v).then_some((s.path, s.pid))); // used: ours to delete
    }

    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        term::leave(&mut io::stdout());
        prev(info);
    }));
    if term::enter(&mut io::stdout()).is_err() {
        eprintln!("si: cannot set up the terminal");
        return ExitCode::FAILURE;
    }
    let t = tx.clone();
    thread::spawn(move || {
        while let Ok(ev) = crossterm::event::read() {
            if term::convert_all(ev).into_iter().any(|e| t.send(e).is_err()) {
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
    fx.extend(app.start(&cli.keys));
    let mut first = true;
    loop {
        let mut frame = None;
        for e in fx.drain(..) {
            match e {
                Effect::Render(f) => frame = Some(f),
                Effect::Write { path, bytes, expect_hash } => {
                    let (tx, state) = (tx.clone(), state.clone());
                    thread::spawn(move || {
                        let ev = match file::save(&path, &bytes, expect_hash, state.as_deref()) {
                            Ok(Saved::Done(h, warn)) => Event::Saved(Ok((h, warn))),
                            Ok(Saved::Conflict(disk)) => {
                                Event::SaveConflict(disk, Instant::now() + MERGE)
                            }
                            Err(e) => Event::Saved(Err(e)),
                        };
                        drop(tx.send(ev));
                    });
                }
                Effect::Open(path) => drop(tx.send(Event::Opened(path.clone(), read(&path)))),
                Effect::Watch(path) => {
                    drop(watcher.take()); // the old watcher's thread ends before the new one starts
                    // The new watcher's first read is ordered after everything sent so far: the App
                    // asks for it after a save, since what the watcher said meanwhile may be older.
                    watcher = watching(&tx, &path, true);
                }
                Effect::Clipboard(text) => drop(term.clipboard(&text)),
                Effect::Snapshot { .. } | Effect::Unsnapshot => drop(jobs.send(e)),
                Effect::Quit(code) => {
                    term::leave(&mut io::stdout());
                    drop(jobs); // the journal thread ends once its queue is done: wait for that
                    let _ = finished.recv_timeout(Duration::from_secs(2));
                    return ExitCode::from(code as u8);
                }
            }
        }
        if let Some(f) = frame
            && term.draw(f).is_err()
        {
            drop(tx.send(Event::Signal(SIGHUP))); // the terminal is gone
        }
        if std::mem::take(&mut first) {
            e2e_panic();
        }
        let Ok(ev) = rx.recv() else { return ExitCode::FAILURE };
        fx = app.update(ev);
        while let Ok(ev) = rx.try_recv() {
            fx.extend(app.update(ev));
        }
    }
}
