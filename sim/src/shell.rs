//! GUI-4, TREE-5, DISK-1, SAFE-6, PLAIN-1, DIFF-2, DIFF-3
//! The shell: what is done with the Effects `Sim::update` returns, apart from the terminal: read a
//! directory, open, save and watch a file, ask git, highlight, search, snapshot, run a filter. The
//! terminal's loop (`main.rs`) and a window's use this one code, so a window cannot do these things
//! differently. `Shell::threads` runs each job on a thread of its own, as the binary always has;
//! `Shell::inline` runs each at once on the calling thread and queues its answer, so the same input
//! gives the same screen (the offscreen renders and the tests of a window).
//! What is the terminal's or the window's own comes back in `Out`: the last frame, the text for the
//! clipboard, and a quit.
use crate::io::{self as sio, HlJob, Job, MERGE};
use crate::sim::{DiffReq, FLASH_MS, Id, PAUSE_MS, SPIN_MS, Sim, SimEffect, SimEvent};
use sim_core::core::app::{Effect, Event};
use sim_core::core::buffer::hash;
use sim_core::io::file::{self, read};
use sim_core::io::filter;
use sim_core::io::watch::watch;
use std::any::Any;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

/// What the shell needs to know about where it is.
#[derive(Clone, Default)]
pub struct Config {
    /// The working directory: the names sim has for files are relative to it.
    pub cwd: PathBuf,
    /// The root of the tree.
    pub root: PathBuf,
    /// The journal's directory (SAFE-6); none: nothing is journaled, no recovery.
    pub state: Option<PathBuf>,
    /// Where the history and the choices are kept (STATE-1); none: nothing is remembered.
    pub history: Option<PathBuf>,
    /// The `rg` to search with, if any (GREP-8).
    pub rg: Option<PathBuf>,
}

/// What running the Effects left for the terminal or the window to do.
#[derive(Default)]
pub struct Out {
    /// The last screen sim drew.
    pub frame: Option<crate::paint::Screen>,
    /// Text for the system clipboard, in order.
    pub clipboard: Vec<String>,
    /// sim is to end with this code; the Effects after the quit were not run.
    pub quit: Option<i32>,
}

/// A watcher, kept alive by holding it, and the switch that silences it (DISK-1).
type Held = (Box<dyn Any>, Arc<AtomicBool>);

/// The threads, one set for all the files.
struct Threads {
    tx: Sender<SimEvent>,
    saves: Sender<sio::Save>,
    jobs: Sender<Job>,
    finished: Receiver<()>,
    highlights: Sender<HlJob>,
    diffs: Sender<DiffReq>,
    greps: sio::Greps,
    files: Sender<()>,
    git: Sender<()>,
    numbers: Sender<Vec<PathBuf>>,
    reads: sio::Reads,
    watchers: HashMap<Id, Held>,
    pgids: HashMap<Id, u32>,
}

enum Work {
    Threads(Box<Threads>),
    Inline(sio::Highlighter),
}

pub struct Shell {
    cfg: Config,
    /// Answers that came back at once: given to sim before anything else.
    local: Vec<SimEvent>,
    work: Work,
}

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

impl Shell {
    /// The shell of the binary: each job on a thread, its answer sent to `tx`.
    pub fn threads(cfg: Config, tx: Sender<SimEvent>) -> Shell {
        let (jobs, finished) =
            sio::journal_thread(cfg.state.clone(), cfg.history.clone(), tx.clone());
        let (r1, r2) = (cfg.root.clone(), cfg.root.clone());
        let t = Threads {
            saves: sio::save_thread(cfg.state.clone(), tx.clone()),
            highlights: sio::highlight_thread(tx.clone()),
            diffs: sio::diff_thread(tx.clone(), cfg.cwd.clone()), // UI-5: off the main thread
            greps: sio::Greps::new(tx.clone(), cfg.rg.clone()),
            files: sio::worker(tx.clone(), move || SimEvent::Files(sio::files(&r1))),
            git: sio::worker(tx.clone(), move || SimEvent::Status(sio::status(&r2))),
            numbers: sio::changes_thread(tx.clone(), cfg.root.clone()), // DIFF-2
            reads: sio::Reads::default(),
            watchers: HashMap::new(),
            pgids: HashMap::new(),
            tx,
            jobs,
            finished,
        };
        Shell { cfg, local: vec![], work: Work::Threads(Box::new(t)) }
    }

    /// The shell of a window's tests and offscreen renders: every job runs at once, here. Nothing is
    /// watched, journaled or remembered; a spinner stays on its frame, a pause is over at once.
    pub fn inline(cfg: Config) -> Shell {
        Shell { cfg, local: vec![], work: Work::Inline(sio::Highlighter::default()) }
    }

    /// The name of a file as the job that reads or writes it is to take it: as sim has it (relative to
    /// the working directory, which is the process's) on threads, made absolute when inline, where the
    /// process is somewhere else.
    fn at(&self, name: &str) -> String {
        match self.work {
            Work::Threads(_) => name.to_string(),
            Work::Inline(_) => self.cfg.cwd.join(name).to_string_lossy().into_owned(),
        }
    }

    /// An event for sim, which first notes that a filter has ended (its process group is gone: the id
    /// could be somebody else's), and what sim is to do about it.
    pub fn update(&mut self, sim: &mut Sim, ev: SimEvent) -> Vec<SimEffect> {
        if let (Work::Threads(t), SimEvent::App(id, Event::FilterDone { .. })) =
            (&mut self.work, &ev)
        {
            t.pgids.remove(id);
        }
        sim.update(ev)
    }

    /// An event and what it asks for, run.
    pub fn send(&mut self, sim: &mut Sim, ev: SimEvent) -> Out {
        let fx = self.update(sim, ev);
        self.run(sim, fx)
    }

    /// The file changed outside sim, as its watcher would tell (an inline shell has none): its bytes now.
    pub fn disk_changed(&mut self, sim: &mut Sim, id: Id, name: &str) -> Out {
        let bytes = std::fs::read(self.at(name)).ok();
        let h = bytes.as_deref().map(hash);
        let ev = Event::Disk { bytes, hash: h, deadline: Instant::now() + MERGE, seq: file::seq() };
        self.send(sim, SimEvent::App(id, ev))
    }

    /// Runs the Effects, and what they answer at once (and what that asks for) until nothing is left.
    pub fn run(&mut self, sim: &mut Sim, mut fx: Vec<SimEffect>) -> Out {
        let mut out = Out::default();
        loop {
            for e in fx.drain(..) {
                if self.effect(e, &mut out) {
                    return out;
                }
            }
            if self.local.is_empty() {
                return out;
            }
            fx = self.local.drain(..).flat_map(|ev| sim.update(ev)).collect();
        }
    }

    /// The end: the filters nobody waits for are stopped, and the journal has what it was sent (it
    /// ends once its queue is done: waited for, 2 s at most).
    pub fn finish(&mut self) {
        if let Work::Threads(t) = &mut self.work {
            t.pgids.values().for_each(|&g| filter::cancel(g));
            let (jobs, _) = mpsc::channel();
            drop(std::mem::replace(&mut t.jobs, jobs));
            let _ = t.finished.recv_timeout(Duration::from_secs(2));
        }
    }

    /// An answer after a while (the yank's flash, the spinner's frame, the pause on a prefix): a thread
    /// sleeps, or, inline, the pause is over and the flash gone at once and the spinner stays.
    fn later(&mut self, wait: Duration, ev: SimEvent) {
        match &self.work {
            Work::Threads(t) => {
                let t = t.tx.clone();
                thread::spawn(move || {
                    thread::sleep(wait);
                    drop(t.send(ev));
                });
            }
            Work::Inline(_) if !matches!(ev, SimEvent::Spin(_)) => self.local.push(ev),
            Work::Inline(_) => {}
        }
    }

    /// One Effect. Whether sim is to quit (nothing after it is run).
    fn effect(&mut self, e: SimEffect, out: &mut Out) -> bool {
        let (cwd, root) = (self.cfg.cwd.clone(), self.cfg.root.clone());
        match (e, &mut self.work) {
            (SimEffect::Render(f), _) => out.frame = Some(f),
            (SimEffect::Read(dir), Work::Threads(t)) => {
                let (tx, reads, ticket) = (t.tx.clone(), t.reads.clone(), t.reads.start());
                thread::spawn(move || {
                    let r = sio::read_dir(&dir);
                    let to = dir.clone();
                    reads.answer(&to, ticket, || drop(tx.send(SimEvent::Listing(dir, r))));
                });
            }
            (SimEffect::Read(dir), Work::Inline(_)) => {
                let r = sio::read_dir(&dir);
                self.local.push(SimEvent::Listing(dir, r));
            }
            (SimEffect::Files, Work::Threads(t)) => drop(t.files.send(())),
            (SimEffect::Files, Work::Inline(_)) => {
                self.local.push(SimEvent::Files(sio::files(&root)))
            }
            (SimEffect::Status, Work::Threads(t)) => drop(t.git.send(())),
            (SimEffect::Status, Work::Inline(_)) => {
                self.local.push(SimEvent::Status(sio::status(&root)))
            }
            (SimEffect::Changes(untracked), Work::Threads(t)) => drop(t.numbers.send(untracked)),
            (SimEffect::Changes(untracked), Work::Inline(_)) => {
                self.local.push(SimEvent::Changes(sio::changes(&root, &untracked)))
            }
            (SimEffect::Show(path), Work::Threads(t)) => sio::show_thread(t.tx.clone(), root, path),
            (SimEffect::Show(path), Work::Inline(_)) => {
                let blob = sio::show_head(&root, &path);
                self.local.push(SimEvent::Shown(path, blob));
            }
            (SimEffect::Flash(n), _) => {
                self.later(Duration::from_millis(FLASH_MS), SimEvent::Unflash(n))
            }
            // PICK-5: a frame of the spinner; the state asks again while the search runs.
            (SimEffect::Spin(n), _) => {
                self.later(Duration::from_millis(SPIN_MS), SimEvent::Spin(n))
            }
            // KEY-3: a pause on a prefix: the thread's only job is to say when it is over.
            (SimEffect::Pause(n), _) => {
                self.later(Duration::from_millis(PAUSE_MS), SimEvent::Pause(n))
            }
            (SimEffect::Diff(req), Work::Threads(t)) => drop(t.diffs.send(req)),
            (SimEffect::Diff(req), Work::Inline(_)) => {
                let answer = sio::diff(&cwd, &req);
                self.local.push(SimEvent::Diff(req.id, answer, req.lines));
            }
            (SimEffect::History(h), Work::Threads(t)) => drop(t.jobs.send(Job::History(h))),
            (SimEffect::Look(k), Work::Threads(t)) => drop(t.jobs.send(Job::Look(k))),
            (SimEffect::Icons(on), Work::Threads(t)) => drop(t.jobs.send(Job::Icons(on))),
            (SimEffect::TabUses(n), Work::Threads(t)) => drop(t.jobs.send(Job::TabUses(n))),
            (
                SimEffect::History(_)
                | SimEffect::Look(_)
                | SimEffect::Icons(_)
                | SimEffect::TabUses(_),
                Work::Inline(_),
            ) => {}
            (SimEffect::Highlight(req), Work::Threads(t)) => {
                drop(t.highlights.send(HlJob::Run(req)))
            }
            (SimEffect::Highlight(req), Work::Inline(hl)) => self.local.push(hl.run(req)),
            (SimEffect::Grep(req), Work::Threads(t)) => t.greps.ask(req),
            (SimEffect::Grep(req), Work::Inline(_)) => {
                let mut batches = vec![];
                let rg = self.cfg.rg.clone();
                sio::grep(&req, rg.as_deref(), &|| true, &mut |b| batches.push(SimEvent::Grep(b)));
                self.local.extend(batches);
            }
            (SimEffect::Preview(path), Work::Threads(t)) => {
                let tx = t.tx.clone();
                thread::spawn(move || drop(tx.send(sio::preview(&root, &path))));
            }
            (SimEffect::Preview(path), Work::Inline(_)) => {
                self.local.push(sio::preview(&root, &path))
            }
            (SimEffect::Open(id, path), w) => {
                if let Work::Threads(t) = w {
                    // Watching starts before the file is read, so that no change slips in between.
                    t.watchers
                        .extend(watching(&t.tx, id, &path, false).filter(|_| !path.is_empty()));
                }
                let at = self.at(&path);
                let found = sio::open(self.cfg.state.as_deref(), id, &path, Path::new(&at));
                self.local.push(found);
            }
            (SimEffect::Close(id), Work::Threads(t)) => {
                drop(t.watchers.remove(&id));
                drop(t.highlights.send(HlJob::Forget(id)));
                t.pgids.remove(&id).into_iter().for_each(filter::cancel); // nobody waits for it
            }
            (SimEffect::Close(_), Work::Inline(_)) => {}
            (SimEffect::Hold(id, key, pid), Work::Threads(t)) => {
                drop(t.jobs.send(Job::Hold(id, key, pid)))
            }
            (SimEffect::Hold(..), Work::Inline(_)) => {}
            (SimEffect::App(id, e), _) => self.app_effect(id, e, out),
            (SimEffect::Quit(code), _) => {
                out.quit = Some(code);
                return true;
            }
        }
        false
    }

    /// An Effect of one file's App.
    fn app_effect(&mut self, id: Id, e: Effect, out: &mut Out) {
        let state = self.cfg.state.clone();
        match (e, &mut self.work) {
            (Effect::Write { path, bytes, expect_hash }, Work::Threads(t)) => {
                drop(t.saves.send((id, path, bytes, expect_hash)))
            }
            (Effect::Write { path, bytes, expect_hash }, Work::Inline(_)) => {
                let at = self.at(&path);
                let done = file::save(&at, &bytes, expect_hash, state.as_deref());
                let (seq, soon) = (file::seq(), Instant::now() + MERGE);
                let ev = match done {
                    Ok(file::Saved::Done(h, w)) => Event::Saved(Ok((h, w)), seq),
                    Ok(file::Saved::Conflict(d)) => Event::SaveConflict(d, soon, seq),
                    Err(e) => Event::Saved(Err(e), seq),
                };
                self.local.push(SimEvent::App(id, ev));
            }
            (Effect::Open(path), w) => {
                // The watcher of the file being left goes quiet before the read's number is taken:
                // what it says later began earlier, and App drops it (DISK-1).
                if let Work::Threads(t) = w {
                    t.watchers.get(&id).iter().for_each(|(_, live)| live.store(false, SeqCst));
                }
                let seq = file::seq(); // taken before the read starts (DISK-1)
                let opened = Event::Opened(path.clone(), read(&self.at(&path)), seq);
                self.local.push(SimEvent::App(id, opened));
                if let Work::Threads(t) = &self.work {
                    drop(t.jobs.send(Job::App(id, Effect::Open(path))));
                }
            }
            (Effect::Watch(path), Work::Threads(t)) => {
                drop(t.watchers.remove(&id)); // the old watcher's thread ends before the new one starts
                // Its first read is ordered after everything sent so far (see the core's main).
                t.watchers.extend(watching(&t.tx, id, &path, true));
            }
            (Effect::Clipboard(text), _) => out.clipboard.push(text),
            (
                e @ (Effect::Snapshot { .. }
                | Effect::Unsnapshot
                | Effect::Remember { .. }
                | Effect::Keep { .. }),
                Work::Threads(t),
            ) => drop(t.jobs.send(Job::App(id, e))),
            (Effect::Spawn { cmd, input }, Work::Threads(t)) => {
                let tx = t.tx.clone();
                let done = {
                    let tx = tx.clone();
                    move |o: filter::Output| filter_done(&tx, id, o.status, o.stdout, o.stderr)
                };
                let started = filter::spawn(&cmd, input, done).inspect_err(|e| {
                    let why = format!("cannot run: {e}").into_bytes();
                    filter_done(&tx, id, Some(127), vec![], why)
                });
                t.pgids.extend(started.ok().map(|g| (id, g)));
            }
            (Effect::Spawn { cmd, input }, Work::Inline(_)) => {
                // run to its end here: an inline shell has no loop to wait in
                let (tx, rx) = mpsc::channel();
                let started = filter::spawn(&cmd, input, move |o| drop(tx.send(o)));
                let (status, bytes, err) = match started
                    .map(|_| rx.recv_timeout(Duration::from_secs(5)))
                {
                    Ok(Ok(o)) => (o.status, o.stdout, o.stderr),
                    Ok(Err(_)) => (Some(124), vec![], b"the filter did not end in 5 s".to_vec()),
                    Err(e) => (Some(127), vec![], format!("cannot run: {e}").into_bytes()),
                };
                let ev = Event::FilterDone(status, bytes, err, Instant::now() + MERGE);
                self.local.push(SimEvent::App(id, ev));
            }
            // `kill` waits for a process: not on this thread.
            (Effect::Kill, Work::Threads(t)) => {
                let g = t.pgids.get(&id).copied();
                drop(g.map(|g| thread::spawn(move || filter::cancel(g))))
            }
            (Effect::Hook(cmd), Work::Threads(_)) => drop(filter::spawn(&cmd, vec![], |_| {})),
            _ => {} // a watch, a snapshot, a kill, a hook with no threads to do them; a render or a quit, which are not here
        }
    }
}
