//! TREE-1, BUF-1, BUF-3, FIND-3, MARK-1, MARK-2, SYN-1, SYN-3, GREP-5, PTR-1, SPEED-1, SPEED-2, UI-5, UI-13, PICK-5, PLAIN-1, STATE-1, COLOR-2, ICON-1, VIS-4, PERF-1, PERF-5, CLI-7, GUI-4
//! The event loop: several core Apps, the panel, the terminal, and the shell (`shell.rs`) that runs
//! the Effects: one save thread and
//! one journal thread for all of them, a watcher and a filter for each, and every job and answer
//! tagged with the id of its file. The file list, git's
//! status, the history, the highlighter and the searches of file contents each run in the
//! background. The terminal is asked for its colors (the background at once, the first
//! frame waiting for it at most 3 ms), and the mouse is reported until sim leaves, by any way.
#![forbid(unsafe_code)]
use signal_hook::consts::{SIGHUP, SIGTERM, SIGWINCH};
use sim::color::{self, Input, Syntax};
use sim::io::{self as sio};
use sim::layout::Skin;
use sim::paint::{MOUSE_ON, Painter, leave};
use sim::rank::History;
use sim::shell::{Config, Shell};
use sim::sim::{Env, Sim, SimEvent, start};
use sim_core::core::app::Event;
use sim_core::io::{filter, journal, term};
use std::io::{self, IsTerminal, Write};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use std::{env, fs, thread};

/// How long a quit waits for the terminal to finish answering (VIS-4).
const DRAIN: Duration = Duration::from_millis(100);
/// Set by a quit that is waiting for the terminal: the input thread then asks it and reports.
static QUITTING: AtomicBool = AtomicBool::new(false);
/// CLI-7: what `--help` prints, and an argument sim cannot use (to stderr, exit 2): `help.txt`, prose
/// like the key table, with the version in it.
fn usage() -> String {
    include_str!("../help.txt").replace("{version}", env!("CARGO_PKG_VERSION"))
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
        eprint!("sim: a file name that is not valid UTF-8 cannot be opened\n\n{}", usage());
        return ExitCode::from(2);
    };
    // CLI-7: these two never touch the terminal (no raw mode, no query)
    let flag = |f: &str| args.iter().take_while(|a| *a != "--").any(|a| a == f);
    if flag("--version") {
        println!("sim {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    if flag("--help") {
        print!("{}", usage());
        return ExitCode::SUCCESS;
    }
    let cwd = env::current_dir().unwrap_or_default();
    let here = cwd.clone();
    let Some(mut s) = start(&args, &cwd, |f| std::path::Path::new(f).exists(), |d| d.is_dir())
    else {
        eprint!("{}", usage());
        return ExitCode::from(2);
    };
    s.root = fs::canonicalize(&s.root).unwrap_or(s.root);
    let late = !s.keys.is_empty(); // VIS-4: startup keys may quit; the terminal is asked once they have run
    let size = crossterm::terminal::size();
    let (Ok(size), true) = (size, io::stdout().is_terminal()) else {
        eprintln!("sim: the standard output is not a terminal");
        return ExitCode::FAILURE;
    };
    let (tx, rx) = mpsc::channel();
    // SAFE-6: without a state directory (and unless it is private) there are no
    // snapshots and no recovery copies; the first snapshot that cannot be written says so.
    // STATE-1: one state directory, `sim`; what the core's held is copied into it, once.
    let state = journal::state_dir(env::var_os("XDG_STATE_HOME"), env::var_os("HOME"));
    let moved = state.as_deref().and_then(|d| journal::migrate(d).err());
    let history = state.clone();
    // COLOR-2: without color nothing asks the terminal for any.
    let mono = sio::no_color_here();
    let truecolor = !mono && matches!(env::var("COLORTERM").as_deref(), Ok("truecolor" | "24bit"));
    let (icons, hint) = sio::start_icons(history.as_deref(), |k| env::var(k).ok());
    let env = Env {
        cwd,
        size,
        now: sio::now_ms() / 1000,
        history: history.as_deref().map_or_else(History::default, sio::load_history),
        syn: if mono { Syntax::mono() } else { Syntax::new(truecolor) },
        hook: sio::hook(env::var("SIM_ON_NORMAL").ok(), env::var("SI_ON_NORMAL").ok()),
        plain: sio::plain_by_default(),
        skin: history.as_deref().map_or(Skin::Depth, sio::load_look),
        home: env::var_os("HOME").map(std::path::PathBuf::from),
        icons,
        hint,
        tab_uses: history.as_deref().map_or(0, sio::load_tab_uses),
        chrome: true,
    };
    let (mut sim, mut fx) = Sim::new(s, env);
    if sim.plain {
        filter::forbid_programs(); // PLAIN-1: before anything asks for git, rg or a program
    } else {
        state.as_deref().iter().for_each(|d| sio::use_no_hooks(d)); // GIT-1: before git is run
    }
    if let Some(e) = moved {
        drop(tx.send(SimEvent::Notice(format!("cannot copy the old state directory (si): {e}"))));
    }

    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        leave(&mut io::stdout());
        prev(info);
    }));
    // VIS-4: the background is asked for right after the screen is switched (the core's `enter`), the
    // mouse is reported from here on (PTR-1).
    let entered = term::enter(&mut io::stdout(), !mono && !late);
    let mouse = |out: &mut io::Stdout| out.write_all(MOUSE_ON).and_then(|()| out.flush());
    if entered.and_then(|()| mouse(&mut io::stdout())).is_err() {
        leave(&mut io::stdout());
        eprintln!("sim: cannot set up the terminal");
        return ExitCode::FAILURE;
    }
    let mut asked = Instant::now(); // the query for the background color went out with `enter`
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
    let rg = sio::which_rg(env::var_os("PATH")).filter(|_| !sim.plain);
    let cfg = Config { cwd: here, root: sim.root.clone(), state, history, rg };
    let mut shell = Shell::threads(cfg, tx.clone());
    let (mut painter, tmux) = (Painter::new(io::stdout()), env::var_os("TMUX").is_some());
    let mut first = true;
    // `queried`: whether the terminal was asked for anything (a quit before that leaves nothing to wait for)
    let (mut waiting, mut late) = (!mono, late && !mono);
    let mut queried = !mono && !late;
    loop {
        let out = shell.run(&mut sim, std::mem::take(&mut fx));
        let frame = out.frame;
        for text in out.clipboard {
            drop(painter.write(&term::clipboard(&text, tmux)));
        }
        if let Some(code) = out.quit {
            if queried && sim.asking() {
                // An answer may still be on its way: the input thread is woken (as a
                // resize would) to wait for it, since a late one would be typed into the shell
                QUITTING.store(true, SeqCst);
                drop(signal_hook::low_level::raise(SIGWINCH));
                let _ = drained.recv_timeout(DRAIN);
            }
            leave(&mut io::stdout());
            shell.finish(); // the filters nobody waits for are stopped, the journal has what it was sent
            return ExitCode::from(code as u8);
        }
        if std::mem::take(&mut late) {
            drop(painter.write(term::ASK)); // the keys given at startup did not quit
            (asked, queried) = (Instant::now(), true);
        }
        // VIS-4: the first frame waits at most 3 ms for the terminal's color, which it gets as it is
        // drawn. The wait overlaps the work above. What came meanwhile is the Sim's too.
        if std::mem::take(&mut waiting) {
            let (mut early, until) = (vec![], asked + Duration::from_millis(3));
            // (what is in already is taken even when the 3 ms are over)
            while let Ok(ev) = match until.checked_duration_since(Instant::now()) {
                Some(left) => rx.recv_timeout(left).map_err(drop),
                None => rx.try_recv().map_err(drop),
            } {
                let bg = matches!(ev, SimEvent::Term(Event::Bg(_)));
                early.push(ev);
                if bg {
                    break;
                }
            }
            if !early.is_empty() {
                fx = early.into_iter().flat_map(|ev| shell.update(&mut sim, ev)).collect();
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
            fx.extend(shell.update(&mut sim, ev));
        }
    }
}
