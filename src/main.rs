//! CLI-4, SAFE-6, VI-7, VI-8, PERF-1, PERF-5
//! The event loop, and running Effects.
#![forbid(unsafe_code)]
use si::core::app::{App, Effect, Event, Read};
use si::core::cli::parse;
use si::io::file::{self, Saved};
use si::io::journal::{self, Snapshot};
use si::io::term::{self, Term};
use signal_hook::consts::{SIGHUP, SIGTERM};
use std::io::{self, IsTerminal};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::{env, fs, thread};

const USAGE: &str = "usage: si [+cmd] [file]\n       si [+cmd] -- file";

/// `Ok(None)`: no such file yet.
fn read(path: &str) -> Read {
    match fs::metadata(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(file::why(&e)),
        Ok(m) if !m.is_file() => Err("not a regular file".into()),
        Ok(_) => fs::read(path).map(Some).map_err(|e| file::why(&e)),
    }
}

fn state_dir() -> PathBuf {
    let var = |k| env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    var("XDG_STATE_HOME")
        .or_else(|| var("HOME").map(|h| h.join(".local/state")))
        .unwrap_or_else(env::temp_dir)
        .join("si")
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

/// The journal key of a buffer (SAFE-6): the canonical path of its file, `""` without one.
fn key(path: Option<&str>) -> String {
    path.map_or(String::new(), |p| file::resolve(p).to_string_lossy().into_owned())
}

/// The journal thread (SAFE-6) runs Snapshots and Unsnapshots in order, so no key waits for a
/// disk. `held` are the snapshots to delete once ours replaces them, or when we are done. The
/// receiver reports `Disconnected` when the thread has finished what was sent to `Sender`.
fn journal_thread(state: PathBuf, mut held: Vec<(String, u32)>) -> (Sender<Effect>, Receiver<()>) {
    let (jobs, rx) = mpsc::channel();
    let (done, finished) = mpsc::channel();
    thread::spawn(move || {
        let (pid, _done) = (std::process::id(), done);
        let rm = |h: &(String, u32)| journal::remove(&state, &h.0, h.1);
        for job in rx {
            if let Effect::Snapshot { path, base, text } = job {
                let [base, text] = [base, text].map(|r| r.to_string().into_bytes());
                let snap =
                    Snapshot { path: key(path.as_deref()), pid, time_ms: now_ms(), base, text };
                let mine = (snap.path.clone(), pid);
                if journal::write(&state, &snap).is_ok() {
                    held.iter().filter(|h| **h != mine).for_each(&rm);
                    held = vec![mine];
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
    let size = crossterm::terminal::size();
    let (Ok(size), true) = (size, io::stdout().is_terminal()) else {
        eprintln!("si: the standard output is not a terminal");
        return ExitCode::FAILURE;
    };
    let file = cli.file.as_deref().map_or(Ok(None), read);
    let state = state_dir();
    let mut app = App::new(cli.file.clone(), file, size);
    let (mut fx, mut held) = (vec![], vec![]);
    if let Some(s) = journal::find(&state, &key(cli.file.as_deref()), std::process::id()) {
        let file = journal::file(&state, &s.path, s.pid).display().to_string();
        fx = app.update(Event::Recover(s.base, s.text, now_ms().saturating_sub(s.time_ms), file));
        held.extend(app.buf.dirty().then_some((s.path, s.pid))); // applied: ours to delete
    }

    let (tx, rx) = mpsc::channel();
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
            if let Some(e) = term::convert(ev)
                && t.send(e).is_err()
            {
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
    let (jobs, finished) = journal_thread(state.clone(), held);
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
                        let _ = tx.send(match file::save(&path, &bytes, expect_hash, &state) {
                            Ok(Saved::Done(h, warn)) => Event::Saved(Ok((h, warn))),
                            Ok(Saved::Conflict(disk)) => Event::SaveConflict(disk),
                            Err(e) => Event::Saved(Err(e)),
                        });
                    });
                }
                Effect::Open(path) => drop(tx.send(Event::Opened(path.clone(), read(&path)))),
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
