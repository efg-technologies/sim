//! CLI-4, VI-7, VI-8, PERF-1, PERF-5
//! The event loop, and running Effects.
#![forbid(unsafe_code)]
use si::core::app::{App, Effect, Event, Read};
use si::core::cli::parse;
use si::io::file::{self, Saved};
use si::io::term::{self, Term};
use signal_hook::consts::{SIGHUP, SIGTERM};
use std::io::{self, IsTerminal};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::mpsc;
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
    let mut app = App::new(cli.file, file, size);

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
    let state = state_dir();
    let mut term = Term::new(io::stdout(), env::var_os("TMUX").is_some());
    let mut fx = app.start(&cli.keys);
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
                Effect::Quit(code) => {
                    term::leave(&mut io::stdout());
                    return ExitCode::from(code as u8);
                }
            }
        }
        if let Some(f) = frame
            && term.draw(f).is_err()
        {
            term::leave(&mut io::stdout());
            return ExitCode::FAILURE;
        }
        if std::mem::take(&mut first) {
            e2e_panic();
        }
        let ev = match rx.recv_timeout(Duration::from_secs(1)) {
            Ok(ev) => ev,
            Err(mpsc::RecvTimeoutError::Timeout) => Event::Tick(
                SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64),
            ),
            Err(mpsc::RecvTimeoutError::Disconnected) => return ExitCode::FAILURE,
        };
        fx = app.update(ev);
        while let Ok(ev) = rx.try_recv() {
            fx.extend(app.update(ev));
        }
    }
}
