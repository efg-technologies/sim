//! WATCH-1, WATCH-2, WATCH-3: watching the project. What sim does with what the watcher saw (`Touched`),
//! driven through its state; and the watcher itself, on real directories and repositories, with its own
//! thread: what it names, what it leaves out, git's directories (a linked worktree's too), and what it does
//! where the system refuses a watch. The timing tests wait for at most a few seconds.
#![allow(clippy::unwrap_used, clippy::panic)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use sim::io;
use sim::layout::Skin;
use sim::sim::{SimEffect, SimEvent, Touched};
use sim_core::core::app::Event;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

/// A batch that names `paths` (and whether each is a file now).
fn touched(paths: &[(&str, bool)]) -> SimEvent {
    let paths = paths.iter().map(|(p, f)| (PathBuf::from(p), *f)).collect();
    SimEvent::Touched(Touched { paths, ..Touched::default() })
}

/// What an event asked of the shell, by kind, the frame left out.
fn asked(fx: &[SimEffect], root: &Path) -> Vec<String> {
    fx.iter()
        .filter_map(|e| match e {
            SimEffect::Render(_) => None,
            SimEffect::Read(d) => {
                Some(format!("read {}", d.strip_prefix(root).unwrap().display()).trim().into())
            }
            SimEffect::Files => Some("files".into()),
            SimEffect::Status => Some("status".into()),
            SimEffect::Flash(_, ms) => Some(format!("flash {ms}")),
            e => Some(format!("{e:?}")),
        })
        .collect()
}

/// The panel's row that names `name`.
fn row(r: &Rig, name: &str) -> String {
    r.panel().into_iter().find(|l| l.contains(name)).unwrap_or_default()
}

/// A Rig on a repository: `a.txt`, `sub/b.txt` and `other/c.txt` committed, and `x.log`, which git
/// ignores; the panel open with `sub/` open and `other/` closed.
fn project(name: &str, skin: Skin) -> Rig {
    let files =
        ["a.txt=a\n", "sub/b.txt=b\n", "other/c.txt=c\n", ".gitignore=*.log\n", "x.log=noise\n"];
    let mut r = rig_env(name, &files, &[], (80, 16), |e| {
        repo(&e.cwd, &[]);
        e.skin = skin;
        e.syn = sim::color::Syntax::new(true);
    });
    r.send(SimEvent::Term(Event::Bg((0x1e, 0x1e, 0x1e))));
    r.keys("jl"); // other/ is the first row, sub/ the second: open sub/
    assert!(r.panel().iter().any(|l| l.contains("b.txt")), "{:?}", r.panel());
    r
}

/// Opens the file the panel names `name` with Enter.
fn open(r: &mut Rig, name: &str) {
    if !(r.sim.panel.open && r.sim.panel.focus) {
        r.keys("<C-n>");
    }
    r.keys("gg");
    for _ in 0..20 {
        if r.selected().is_some_and(|s| s.contains(name)) {
            r.keys("<CR>");
            return assert_eq!(r.app().path.as_deref().map(|p| p.ends_with(name)), Some(true));
        }
        r.keys("j");
    }
    panic!("no row says {name}: {:?}", r.panel());
}

/// The panel, with the focus.
fn panel(r: &mut Rig) -> Vec<String> {
    if !(r.sim.panel.open && r.sim.panel.focus) {
        r.keys("<C-n>");
    }
    r.panel()
}

#[test]
fn watch_1_touched_asks_for_the_status_and_the_open_directories_only() {
    let mut r = project("w1-asks", Skin::Calm);
    let root = r.sim.root.clone();
    let before = (r.sim.cur, r.sim.panel.cur, r.sim.panel.focus, r.sim.bufs.len());
    // A write to a listed file in an open directory and one in a closed one: their open directory is
    // read again (the closed one is not), git's status is asked, the rows are lit; no list of files (no
    // file came or went), nothing opened.
    let fx = r.sim.update(touched(&[("sub/b.txt", true), ("other/c.txt", true)]));
    assert_eq!(asked(&fx, &root), ["flash 400", "read sub", "status"]);
    // A new file at the top: the root is read, and the list of files asked for.
    let fx = r.sim.update(touched(&[("new.txt", true)]));
    assert_eq!(asked(&fx, &root), ["flash 400", "read", "files", "status"]);
    // git's own files moved (a commit, a checkout): the status alone.
    let git = SimEvent::Touched(Touched { git: true, ..Touched::default() });
    assert_eq!(asked(&r.sim.update(git), &root), ["status"]);
    // More than a batch names: every open directory, the list and the status.
    let many = SimEvent::Touched(Touched { many: true, ..Touched::default() });
    let mut fx = asked(&r.sim.update(many), &root);
    fx.sort();
    assert_eq!(fx, ["files", "read", "read sub", "status"]);
    // Nothing moved: the file shown, the panel's cursor and focus, the open files.
    assert_eq!(before, (r.sim.cur, r.sim.panel.cur, r.sim.panel.focus, r.sim.bufs.len()));
}

#[test]
fn watch_1_what_git_ignores_asks_git_nothing_and_is_no_news() {
    let mut r = project("w1-ignored", Skin::Calm);
    let root = r.sim.root.clone();
    let fx = r.sim.update(touched(&[("x.log", true)]));
    assert_eq!(asked(&fx, &root), ["read"], "its directory is read again, and that is all");
    assert!(!row(&r, "x.log").contains('↯'), "{:?}", r.panel());
}

#[test]
fn watch_2_a_file_is_unseen_until_it_has_been_looked_at() {
    let mut r = project("w2-unseen", Skin::Calm);
    r.send(touched(&[("sub/b.txt", true)]));
    assert_eq!(row(&r, "b.txt").trim(), "b.txt ↯", "COLOR-3: an arrow, which needs no color");
    r.send(touched(&[("sub/b.txt", true)]));
    assert!(row(&r, "b.txt").contains('↯'), "it stays until it is looked at");
    // Shown, with no mark of DISK-6 in it: it has been looked at.
    open(&mut r, "b.txt");
    assert!(!row(&r, "b.txt").contains('↯'));
    assert!(!panel(&mut r).iter().any(|l| l.contains('↯')), "{:?}", r.panel());
    // An open file that is not shown: what another process wrote and the file marks is unseen until it
    // is shown and no mark is left in it (`:seen` clears them).
    let b = r.sim.bufs[0].id;
    open(&mut r, "a.txt");
    r.send(SimEvent::App(b, disk(b"b\nmore\n")));
    assert!(row(&r, "b.txt").contains('↯'), "{:?}", r.panel());
    r.keys("<Esc><Tab>");
    assert_eq!(r.app().path.as_deref(), Some("sub/b.txt"));
    assert!(panel(&mut r).iter().any(|l| l.contains("b.txt ↯")), "shown, but its mark is left");
    r.keys("<Esc>:seen\n");
    assert!(!panel(&mut r).iter().any(|l| l.contains('↯')), "{:?}", r.panel());
}

#[test]
fn watch_2_sim_s_own_saves_make_no_mark() {
    let mut r = project("w2-own", Skin::Calm);
    open(&mut r, "a.txt");
    r.keys("x:w\n");
    // The watcher tells of the save, as it tells of any write: it is sim's own, and no news.
    r.send(touched(&[("a.txt", true)]));
    // And `:w` to a name no file has open is sim's own too.
    r.keys(":w copy.txt\n");
    r.send(touched(&[("copy.txt", true)]));
    assert!(panel(&mut r).iter().all(|l| !l.contains('↯')), "{:?}", r.panel());
    assert!(row(&r, "copy.txt").contains("copy.txt"), "{:?}", r.panel());
    // What another process writes to that file after it is news again.
    r.send(touched(&[("copy.txt", true)]));
    assert!(row(&r, "copy.txt").contains('↯'), "{:?}", r.panel());
}

#[test]
fn watch_2_the_marks_take_a_written_file_as_they_take_the_unseen_ones_they_are_made_with() {
    use sim::marks::Marks;
    use std::collections::HashSet;
    let status = b" M a.rs\0?? new/b.rs\0";
    let unseen: HashSet<PathBuf> = [PathBuf::from("deep/er/c.rs"), PathBuf::from("a.rs")].into();
    let mut m = Marks::new(status, "", &HashSet::new());
    for p in &unseen {
        m.wrote(p);
    }
    assert_eq!(m, Marks::new(status, "", &unseen));
    assert!(m.unseen(Path::new("deep/er/c.rs")) && m.changed_below(Path::new("deep")));
    assert!(!m.unseen(Path::new("new/b.rs")), "git's change is not a write from outside");
    // the changes view lists git's: an unseen file git has no entry for is no change from HEAD
    let mut listed: Vec<_> = m.differing().cloned().collect();
    listed.sort();
    assert_eq!(listed, [PathBuf::from("a.rs"), PathBuf::from("new/b.rs")]);
}

/// The paint of the first cell of the row that names `name` in the panel.
fn first_paint(r: &Rig, name: &str) -> Option<sim::paint::Paint> {
    let y = r.panel().iter().position(|l| l.contains(name))?;
    let x = r.panel()[y].find(name).map(|b| r.panel()[y][..b].chars().count() as u16)?;
    r.shot.paints[y].iter().find(|(c, _)| c.contains(&x)).map(|p| p.1)
}

#[test]
fn watch_2_the_row_is_lit_for_a_moment_and_a_closed_directory_s_row_for_what_it_hides() {
    let mut r = project("w2-lit", Skin::Flat);
    let green = sim::color::Tones::new(&sim_core::core::color::palette((0x1e, 0x1e, 0x1e)));
    let solid = Some(green.add_solid().0);
    let fx = r.sim.update(touched(&[("sub/b.txt", true), ("other/c.txt", true)]));
    let n = fx.iter().find_map(|e| if let SimEffect::Flash(n, _) = e { Some(*n) } else { None });
    r.send(SimEvent::Notice(String::new())); // (a frame)
    assert_eq!(first_paint(&r, "b.txt").and_then(|p| p.bg), solid, "the row of the file");
    assert_eq!(first_paint(&r, "other/").and_then(|p| p.bg), solid, "the closed directory");
    assert_ne!(
        first_paint(&r, "sub/").and_then(|p| p.bg),
        solid,
        "an open one has its own row lit"
    );
    r.send(SimEvent::Unflash(n.unwrap()));
    assert_ne!(first_paint(&r, "b.txt").and_then(|p| p.bg), solid, "and then it is not");
    assert_ne!(first_paint(&r, "other/").and_then(|p| p.bg), solid);
    // In calm nothing is lit (UI-19's flash is flat's and depth's); the arrow says it.
    let mut c = project("w2-lit-calm", Skin::Calm);
    c.send(touched(&[("sub/b.txt", true)]));
    assert!(c.shot.paints.iter().all(Vec::is_empty));
    assert!(row(&c, "b.txt").contains('↯'));
}

// ---- the watcher itself ----

/// A repository in a directory of its own: `a.txt`, `sub/b.txt`, `.gitignore` (ignoring `target/` and
/// `.state/`), `target/junk.rs` (ignored), committed.
fn repository(name: &str) -> TempDir {
    let d = temp_dir(name);
    make(&d.0, &["a.txt", "sub/b.txt", "target/junk.rs"]);
    fs::write(d.0.join(".gitignore"), "target/\n.state/\n").unwrap();
    repo(&d.0, &[]);
    d
}

/// The watcher of `root`, told the list of files, and the channel it answers on. What it says before the
/// first write (nothing, or the batch of its start) is let go.
fn watching(root: &Path, state: Option<PathBuf>, poll: Duration) -> Receiver<SimEvent> {
    let (tx, rx) = mpsc::channel();
    let p = io::project(tx, fs::canonicalize(root).unwrap(), state, poll);
    p.files(&io::files(root));
    std::thread::sleep(Duration::from_millis(300)); // (the watches are made)
    while rx.try_recv().is_ok() {}
    std::mem::forget(p); // (the thread keeps its own handle; the test's ends with it)
    rx
}

/// The batches that come within `wait`.
fn batches(rx: &Receiver<SimEvent>, wait: Duration) -> Vec<Touched> {
    let until = Instant::now() + wait;
    let mut out = vec![];
    while let Some(left) = until.checked_duration_since(Instant::now()) {
        match rx.recv_timeout(left) {
            Ok(SimEvent::Touched(t)) => out.push(t),
            Ok(_) => {}
            Err(_) => break,
        }
    }
    out
}

fn named(b: &[Touched]) -> Vec<(String, bool)> {
    let mut all: Vec<(String, bool)> =
        b.iter().flat_map(|t| &t.paths).map(|(p, f)| (p.display().to_string(), *f)).collect();
    all.sort();
    all.dedup();
    all
}

#[test]
fn watch_1_the_watcher_names_a_write_a_new_file_and_a_deletion_below_the_root_within_a_second() {
    let d = repository("w1-watch");
    let rx = watching(&d.0, None, io::POLL);
    let t = Instant::now();
    fs::write(d.0.join("sub/b.txt"), "written\n").unwrap();
    let first = batches(&rx, Duration::from_millis(400));
    assert!(t.elapsed() < Duration::from_secs(1));
    assert_eq!(named(&first), [("sub/b.txt".into(), true)]);
    fs::write(d.0.join("new.txt"), "new\n").unwrap();
    fs::remove_file(d.0.join("a.txt")).unwrap();
    let next = batches(&rx, Duration::from_millis(400));
    assert_eq!(named(&next), [("a.txt".into(), false), ("new.txt".into(), true)]);
    assert!(next.iter().all(|t| !t.many && !t.git));
}

#[test]
fn watch_1_a_new_directory_is_watched_once_the_list_has_it() {
    let d = repository("w1-newdir");
    let (tx, rx) = mpsc::channel();
    let p = io::project(tx, fs::canonicalize(&d.0).unwrap(), None, io::POLL);
    p.files(&io::files(&d.0));
    std::thread::sleep(Duration::from_millis(300));
    while rx.try_recv().is_ok() {}
    fs::create_dir(d.0.join("fresh")).unwrap();
    fs::write(d.0.join("fresh/one.txt"), "1\n").unwrap();
    assert_eq!(named(&batches(&rx, Duration::from_millis(400))), [("fresh".into(), false)]);
    // sim asks for the list, which has the new file: its directory is watched, and told once more
    p.files(&io::files(&d.0));
    assert_eq!(named(&batches(&rx, Duration::from_millis(400))), [("fresh".into(), false)]);
    fs::write(d.0.join("fresh/one.txt"), "2\n").unwrap();
    assert_eq!(named(&batches(&rx, Duration::from_millis(400))), [("fresh/one.txt".into(), true)]);
}

#[test]
fn watch_1_a_commit_made_outside_moves_git_s_flag() {
    let d = repository("w1-commit");
    let rx = watching(&d.0, None, io::POLL);
    fs::write(d.0.join("a.txt"), "changed\n").unwrap();
    git(&d.0, &["commit", "-q", "-am", "outside"]);
    let b = batches(&rx, Duration::from_millis(600));
    assert!(b.iter().any(|t| t.git), "{b:?}");
}

#[test]
fn watch_1_in_a_linked_worktree_git_s_directories_are_its_own_and_the_common_one() {
    let d = repository("w1-wt");
    let wt = temp_dir("w1-wt-linked");
    fs::remove_dir(&wt.0).unwrap();
    git(&d.0, &["worktree", "add", "-q", &wt.0.to_string_lossy(), "-b", "side"]);
    let common = fs::canonicalize(d.0.join(".git")).unwrap();
    let dirs = io::git_dirs(&fs::canonicalize(&wt.0).unwrap());
    assert_eq!(
        dirs,
        [
            common.join("worktrees").join(wt.0.file_name().unwrap()),
            common.clone(),
            common.join("refs/heads"),
            common.join("hooks")
        ]
    );
    let rx = watching(&wt.0, None, io::POLL);
    fs::write(wt.0.join("a.txt"), "in the worktree\n").unwrap();
    assert_eq!(named(&batches(&rx, Duration::from_millis(400))), [("a.txt".into(), true)]);
    git(&wt.0, &["commit", "-q", "-am", "in the worktree"]);
    let b = batches(&rx, Duration::from_millis(600));
    assert!(b.iter().any(|t| t.git), "{b:?}");
}

#[test]
fn watch_3_its_own_temporary_files_the_state_directory_and_ignored_directories_make_no_event() {
    let d = repository("w3-quiet");
    let state = d.0.join(".state");
    fs::create_dir(&state).unwrap();
    fs::write(state.join("history"), "x\n").unwrap();
    let rx = watching(&d.0, Some(fs::canonicalize(&state).unwrap()), io::POLL);
    fs::write(d.0.join(".a.txt.sim-1234"), "a save's temporary file\n").unwrap();
    fs::rename(d.0.join(".a.txt.sim-1234"), d.0.join(".a.txt.sim-5678")).unwrap();
    for i in 0..50 {
        fs::write(d.0.join(format!("target/{i}.o")), "built\n").unwrap();
    }
    fs::write(state.join("history"), "y\n").unwrap();
    fs::write(state.join("positions"), "z\n").unwrap();
    assert_eq!(named(&batches(&rx, Duration::from_millis(500))), Vec::<(String, bool)>::new());
    fs::write(d.0.join("a.txt"), "news\n").unwrap();
    assert_eq!(named(&batches(&rx, Duration::from_millis(400))), [("a.txt".into(), true)]);
}

#[test]
fn watch_3_an_idle_project_asks_git_nothing() {
    let d = repository("w3-idle");
    fs::write(d.0.join("a.txt"), "changed, for a diff\n").unwrap();
    let rx = watching(&d.0, None, io::POLL);
    // sim's own git (the status, the list, the numbers, the diff, HEAD's version) writes nothing that
    // the watcher sees: it runs with `--no-optional-locks` (MARK-1)
    let root = fs::canonicalize(&d.0).unwrap();
    for _ in 0..3 {
        io::status(&root).unwrap();
        io::files(&root);
        io::changes(&root, &[]).unwrap();
        io::git_diff(&root, "a.txt").unwrap();
        io::show_head(&root, "a.txt").unwrap();
    }
    assert!(
        batches(&rx, Duration::from_millis(1500)).is_empty(),
        "nothing came, so sim asks nothing"
    );
}

#[test]
fn watch_3_where_a_watch_is_refused_the_project_is_looked_at_every_interval_and_that_is_said_once()
{
    let d = repository("w3-refused");
    let locked = d.0.join("locked");
    fs::create_dir(&locked).unwrap();
    fs::write(locked.join("x.txt"), "x\n").unwrap();
    git(&d.0, &["add", "-A"]);
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    if fs::read_dir(&locked).is_ok() {
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
        return eprintln!("skipped: this user reads a directory of mode 000 (root)");
    }
    let (tx, rx) = mpsc::channel();
    let p = io::project(tx, fs::canonicalize(&d.0).unwrap(), None, Duration::from_millis(300));
    p.files(&["a.txt".into(), "locked/x.txt".into()]);
    let mut notices = vec![];
    let mut polls = 0;
    let until = Instant::now() + Duration::from_millis(1100);
    while let Some(left) = until.checked_duration_since(Instant::now()) {
        match rx.recv_timeout(left) {
            Ok(SimEvent::Notice(n)) => notices.push(n),
            Ok(SimEvent::Touched(t)) if t.many => polls += 1,
            Ok(_) => {}
            Err(_) => break,
        }
    }
    p.files(&["a.txt".into(), "locked/x.txt".into()]); // (refused again: said no more)
    std::thread::sleep(Duration::from_millis(100));
    while let Ok(ev) = rx.try_recv() {
        if let SimEvent::Notice(n) = ev {
            notices.push(n);
        }
    }
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(notices.len(), 1, "{notices:?}");
    assert!(
        notices[0].starts_with("the project is looked at every 300ms: cannot watch it"),
        "{notices:?}"
    );
    assert!((2..=4).contains(&polls), "every 300 ms: {polls}");
    // The rest of the project is still watched.
    fs::write(d.0.join("a.txt"), "news\n").unwrap();
    let b = batches(&rx, Duration::from_millis(400));
    assert!(named(&b).contains(&("a.txt".into(), true)), "{b:?}");
}

#[test]
fn watch_3_git_is_asked_at_most_once_at_a_time_and_no_more_often_than_every_200_ms() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
    assert_eq!(io::pace(Duration::from_millis(10)), Duration::from_millis(200));
    assert_eq!(io::pace(Duration::from_millis(100)), Duration::from_millis(400));
    let (runs, (tx, rx)) = (Arc::new(AtomicUsize::new(0)), mpsc::channel());
    let counter = runs.clone();
    let ask = io::worker(tx, move || {
        counter.fetch_add(1, SeqCst);
        std::thread::sleep(Duration::from_millis(10)); // (a status of a small repository)
        SimEvent::Status(Err("not a git repository: x".into()))
    });
    // A storm: 10,000 requests in a second.
    let t = Instant::now();
    for i in 0..10_000u32 {
        ask.send(()).unwrap();
        if i % 100 == 0 {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    let spent = t.elapsed();
    std::thread::sleep(Duration::from_millis(250));
    let n = runs.load(SeqCst);
    let most = (spent.as_millis() / 200 + 2) as usize;
    assert!(n <= most, "{n} runs in {spent:?}: at most {most}");
    assert!(n >= 2, "and it ran");
    drop(rx);
}

/// WATCH-3: a storm: 10,000 files written in ten directories in about a second, through the real
/// watcher; its batches go to sim (timed: they are events on the main thread, as a key is), and each
/// status sim asks for goes to the paced worker that runs git's status of this repository for real.
/// git runs a bounded number of times, and a batch costs the main thread its own paths, never the whole
/// status (SPEED-1 has the keys' times with batches between them, its kind `touched`).
///
///   cargo test --release -p sim-editor --test watch -- --ignored bench --nocapture
#[test]
#[ignore]
fn watch_3_bench_a_storm_of_events_asks_git_a_bounded_number_of_times() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
    release_only();
    let d = repository("w3-storm");
    for i in 0..10 {
        make(&d.0, &[&format!("d{i}/seed.txt")]);
    }
    git(&d.0, &["add", "-A"]);
    git(&d.0, &["commit", "-q", "-m", "seeds"]);
    let root = fs::canonicalize(&d.0).unwrap();
    let (tx, rx) = mpsc::channel();
    let p = io::project(tx.clone(), root.clone(), None, io::POLL);
    p.files(&io::files(&root));
    std::thread::sleep(Duration::from_millis(300));
    while rx.try_recv().is_ok() {}
    let (runs, counter, at) =
        (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)), root.clone());
    let n = counter.clone();
    let status = io::worker(tx, move || {
        n.fetch_add(1, SeqCst);
        SimEvent::Status(io::status(&at))
    });
    let s = sim::sim::start(&[], &root, |_| false, |d| d.is_dir()).unwrap();
    let env = sim::sim::Env { cwd: root.clone(), size: (120, 40), now: NOW, ..Default::default() };
    let (mut sim, _) = sim::sim::Sim::new(s, env);
    sim.update(SimEvent::Files(io::files(&root)));
    let t0 = Instant::now();
    let w = root.clone();
    let writer = std::thread::spawn(move || {
        for i in 0..10_000 {
            fs::write(w.join(format!("d{}/f{i}.txt", i % 10)), "x\n").unwrap();
            if i % 100 == 99 {
                std::thread::sleep(Duration::from_millis(10)); // (ten thousand in a second)
            }
        }
    });
    let (mut times, mut answers, mut named, mut many, mut batches) = (vec![], vec![], 0, 0, 0);
    let mut done = None;
    while done.is_none_or(|t: Instant| t.elapsed() < Duration::from_millis(1500)) {
        if done.is_none() && writer.is_finished() {
            done = Some(Instant::now());
        }
        let Ok(ev) = rx.recv_timeout(Duration::from_millis(50)) else { continue };
        let touch = matches!(ev, SimEvent::Touched(_));
        if let SimEvent::Touched(t) = &ev {
            (batches, named, many) =
                (batches + 1, named + t.paths.len(), many + usize::from(t.many));
        }
        let t = Instant::now();
        let fx = sim.update(ev);
        if touch {
            times.push(t.elapsed())
        } else {
            answers.push(t.elapsed())
        }
        fx.iter().filter(|e| matches!(e, SimEffect::Status)).for_each(|_| status.send(()).unwrap());
    }
    let spent = done.unwrap() - t0;
    runs.store(counter.load(SeqCst), SeqCst);
    times.sort();
    answers.sort();
    let window = t0.elapsed();
    let ran = runs.load(SeqCst);
    eprintln!(
        "WATCH-3 10,000 writes in {spent:?}; {batches} batches ({many} of many), {named} paths named; git's status ran {ran} times in {window:?}; a batch in sim: p50 {:?} p99 {:?} max {:?}; a status answer: max {:?}",
        percentile(&times, 50),
        percentile(&times, 99),
        times[times.len() - 1],
        answers.last().copied().unwrap_or_default()
    );
    assert!(batches as u128 <= window.as_millis() / 100 + 2, "{batches} batches");
    assert!(ran as u128 <= window.as_millis() / 200 + 2, "git ran {ran} times");
    // A batch of a thousand paths is a few hundred microseconds alone and about a millisecond in the
    // storm, with git and the writer on the other cores; a status read again for every batch was 8 ms.
    assert!(
        times[times.len() - 1] < Duration::from_millis(5),
        "a batch is cheap on the main thread"
    );
}

#[test]
fn watch_2_a_panel_too_narrow_for_a_deep_row_and_its_arrow_cuts_them_at_its_edge() {
    // (found by the model: a file one folder down in a panel of 5 columns, with its arrow, took 6)
    let mut r = rig_sized("w2-narrow", &["sub/f3=x\n", "a=y\n"], &[], (16, 6));
    r.keys("l");
    r.send(touched(&[("sub/f3", true)]));
    let (pw, _) = r.sim.widths();
    assert_eq!(pw, 5);
    for row in &r.screen.rows {
        assert!(width(&row.text) <= 16, "{:?} is wider than the screen", row.text);
    }
    assert!(r.panel().iter().all(|l| width(l) <= usize::from(pw)), "{:?}", r.panel());
}
