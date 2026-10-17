//! TREE-1, BUF-1, BUF-3, FIND-3, MARK-1, MARK-2, SYN-1, SYN-3, GREP-5, PTR-1, SPEED-1, SPEED-2, UI-5, UI-13, PICK-5, PLAIN-1, STATE-1, COLOR-2, ICON-1, VIS-4, PERF-1, PERF-5, CLI-7
//! The event loop: several core Apps, the panel, the threads, running Effects: one save thread and
//! one journal thread for all of them, a watcher and a filter for each, and every job and answer
//! tagged with the id of its file. The file list, git's
//! status, the history, the highlighter and the searches of file contents each run in the
//! background. The terminal is asked for its colors (the background at once, the first
//! frame waiting for it at most 3 ms), and the mouse is reported until sim leaves, by any way.
#![forbid(unsafe_code)]
use signal_hook::consts::{SIGHUP, SIGTERM, SIGWINCH};
use sim::color::{self, Input, Syntax};
use sim::io::{self as sio, HlJob, Job, MERGE};
use sim::layout::Skin;
use sim::paint::{MOUSE_ON, Painter, leave};
use sim::rank::History;
use sim::sim::{Env, Id, Sim, SimEffect, SimEvent, start};
use sim_core::core::app::{Effect, Event};
use sim_core::core::buffer::hash;
use sim_core::io::file::{self, read};
use sim_core::io::watch::watch;
use sim_core::io::{filter, journal, term};
use std::any::Any;
use std::collections::HashMap;
use std::io::{self, IsTerminal, Write};
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
use std::sync::mpsc::{self, Sender};
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

/// A watcher, kept alive by holding it, and the switch that silences it (DISK-1).
type Held = (Box<dyn Any>, Arc<AtomicBool>);

/// Watches `path` for file `id` (DISK-1), as the core's main does for its one file. The deadline of each
/// change is stamped when the watcher reads the file, since the core reads no clock (DISK-5), and
/// its hash is made there too, so that no key waits for it (P3). With `first`, the watcher's thread
/// also reads the file once at the start, ordered before the debounced reads.
fn watching(tx: &Sender<SimEvent>, id: Id, path: &str, first: bool) -> Option<(Id, Held)> {
    let (t, live) = (tx.clone(), Arc::new(AtomicBool::new(true)));
    let on_ = live.clone();
    let on = move |seq: u64, bytes: Option<Vec<u8>>| {
        let hash = bytes.as_deref().map(hash);
        let ev = Event::Disk { bytes, hash, deadline: Instant::now() + MERGE, seq };
        on_.load(SeqCst).then(|| drop(t.send(SimEvent::App(id, ev))));
    };
    watch(&file::resolve(path), on, first)
        .inspect_err(|e| drop(tx.send(SimEvent::Notice(format!("cannot watch {path}: {e}")))))
        .ok()
        .map(|w| (id, (Box::new(w) as Box<dyn Any>, live)))
}

/// Tells file `id`'s App that its filter ended (FILT-2). The diff of its result gives up after
/// `MERGE` (DISK-5).
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
    let (jobs, finished) = sio::journal_thread(state.clone(), history, tx.clone());
    let saves = sio::save_thread(state.clone(), tx.clone());
    let highlights = sio::highlight_thread(tx.clone());
    let diffs = sio::diff_thread(tx.clone(), here); // UI-5: the diffs against HEAD, off the main thread
    let rg = sio::which_rg(env::var_os("PATH")).filter(|_| !sim.plain);
    let greps = sio::Greps::new(tx.clone(), rg);
    let (r1, r2) = (sim.root.clone(), sim.root.clone());
    let files = sio::worker(tx.clone(), move || SimEvent::Files(sio::files(&r1)));
    let git = sio::worker(tx.clone(), move || SimEvent::Status(sio::status(&r2)));
    let reads = sio::Reads::default();
    let (mut painter, tmux) = (Painter::new(io::stdout()), env::var_os("TMUX").is_some());
    let (mut watchers, mut local, mut first) = (HashMap::<Id, Held>::new(), vec![], true);
    // `queried`: whether the terminal was asked for anything (a quit before that leaves nothing to wait for)
    let (mut pgids, mut waiting, mut late) = (HashMap::<Id, u32>::new(), !mono, late && !mono);
    let mut queried = !mono && !late;
    loop {
        let mut frame = None;
        loop {
            for e in fx.drain(..) {
                match e {
                    SimEffect::Render(f) => frame = Some(f),
                    SimEffect::Read(dir) => {
                        let (t, reads, ticket) = (tx.clone(), reads.clone(), reads.start());
                        thread::spawn(move || {
                            let r = sio::read_dir(&dir);
                            let to = dir.clone();
                            reads.answer(&to, ticket, || drop(t.send(SimEvent::Listing(dir, r))));
                        });
                    }
                    SimEffect::Files => drop(files.send(())),
                    SimEffect::Flash(n) => {
                        // UI-13: a yank is lit for a moment: the thread's only job is to say when it is over.
                        let t = tx.clone();
                        thread::spawn(move || {
                            thread::sleep(Duration::from_millis(sim::sim::FLASH_MS));
                            drop(t.send(SimEvent::Unflash(n)));
                        });
                    }
                    SimEffect::Spin(n) => {
                        // PICK-5: a frame of the spinner; the state asks again while the search runs.
                        let t = tx.clone();
                        thread::spawn(move || {
                            thread::sleep(Duration::from_millis(sim::sim::SPIN_MS));
                            drop(t.send(SimEvent::Spin(n)));
                        });
                    }
                    SimEffect::Diff(req) => drop(diffs.send(req)),
                    SimEffect::Status => drop(git.send(())),
                    SimEffect::History(h) => drop(jobs.send(Job::History(h))),
                    SimEffect::Look(k) => drop(jobs.send(Job::Look(k))),
                    SimEffect::Icons(on) => drop(jobs.send(Job::Icons(on))),
                    SimEffect::TabUses(n) => drop(jobs.send(Job::TabUses(n))),
                    SimEffect::Pause(n) => {
                        // KEY-3: a pause on a prefix: the thread's only job is to say when it is over.
                        let t = tx.clone();
                        thread::spawn(move || {
                            thread::sleep(Duration::from_millis(sim::sim::PAUSE_MS));
                            drop(t.send(SimEvent::Pause(n)));
                        });
                    }
                    SimEffect::Highlight(req) => drop(highlights.send(HlJob::Run(req))),
                    SimEffect::Grep(req) => greps.ask(req),
                    SimEffect::Preview(path) => {
                        let (t, root) = (tx.clone(), sim.root.clone());
                        thread::spawn(move || drop(t.send(sio::preview(&root, &path))));
                    }
                    SimEffect::Open(id, path) => {
                        // Watching starts before the file is read, so that no change slips in between.
                        watchers
                            .extend(watching(&tx, id, &path, false).filter(|_| !path.is_empty()));
                        let at = std::path::Path::new(&path);
                        local.push(sio::open(state.as_deref(), id, &path, at));
                    }
                    SimEffect::Close(id) => {
                        drop(watchers.remove(&id));
                        drop(highlights.send(HlJob::Forget(id)));
                        pgids.remove(&id).into_iter().for_each(filter::cancel); // nobody waits for it
                    }
                    SimEffect::Hold(id, key, pid) => drop(jobs.send(Job::Hold(id, key, pid))),
                    SimEffect::App(id, e) => match e {
                        Effect::Write { path, bytes, expect_hash } => {
                            drop(saves.send((id, path, bytes, expect_hash)))
                        }
                        Effect::Open(path) => {
                            // The watcher of the file being left goes quiet before the read's number
                            // is taken: what it says later began earlier, and App drops it (DISK-1).
                            watchers
                                .get(&id)
                                .iter()
                                .for_each(|(_, live)| live.store(false, SeqCst));
                            let seq = file::seq(); // taken before the read starts (DISK-1)
                            let opened = Event::Opened(path.clone(), read(&path), seq);
                            local.push(SimEvent::App(id, opened));
                            drop(jobs.send(Job::App(id, Effect::Open(path))));
                        }
                        Effect::Watch(path) => {
                            drop(watchers.remove(&id)); // the old watcher's thread ends before the new one starts
                            // Its first read is ordered after everything sent so far (see the core's main).
                            watchers.extend(watching(&tx, id, &path, true));
                        }
                        Effect::Clipboard(text) => {
                            drop(painter.write(&term::clipboard(&text, tmux)))
                        }
                        Effect::Snapshot { .. }
                        | Effect::Unsnapshot
                        | Effect::Remember { .. }
                        | Effect::Keep { .. } => drop(jobs.send(Job::App(id, e))),
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
                        if queried && sim.asking() {
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
