//! TREE-5, FIND-3, BUF-1: reading a directory, listing the files of a repository, and the
//! threads that keep snapshots and save with si's private state directory (SAFE-6).
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use si::core::app::{Effect, Event};
use si::io::journal::{self, Snapshot};
use sim::io;
use sim::sim::SimEvent;
use sim::tree::Entry;
use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
use std::path::Path;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

fn sorted(mut v: Vec<Entry>) -> Vec<(String, bool)> {
    v.sort_by(|a, b| a.name.cmp(&b.name));
    v.into_iter().map(|e| (e.name, e.is_dir)).collect()
}

fn entry(name: &str, is_dir: bool) -> (String, bool) {
    (name.to_string(), is_dir)
}

#[test]
fn tree_5_read_dir_gives_names_and_kinds_dotfiles_and_git_included() {
    let d = temp_dir("io-readdir");
    make(&d.0, &["a.txt", ".hidden", "sub/inner.rs", "empty/", ".git/HEAD", "日本語.md"]);
    let got = sorted(io::read_dir(&d.0).unwrap());
    let want = [".git", ".hidden", "a.txt", "empty", "sub", "日本語.md"]
        .map(|n| entry(n, matches!(n, ".git" | "empty" | "sub")));
    assert_eq!(got, want, "hiding `.git` is the tree's job");
    assert_eq!(sorted(io::read_dir(&d.0.join("sub")).unwrap()), [entry("inner.rs", false)]);
    assert_eq!(io::read_dir(&d.0.join("empty")).unwrap().len(), 0);
}

#[test]
fn tree_5_read_dir_says_why_it_cannot_read() {
    let d = temp_dir("io-readdir-err");
    make(&d.0, &["file"]);
    let err = io::read_dir(&d.0.join("missing")).unwrap_err();
    assert!(err.starts_with("cannot read ") && err.contains("missing"), "{err}");
    assert!(io::read_dir(&d.0.join("file")).unwrap_err().starts_with("cannot read "));
}

#[test]
fn tree_5_read_dir_follows_a_link_to_a_directory_but_never_into_a_loop() {
    let d = temp_dir("io-readdir-links");
    make(&d.0, &["real/inner.txt", "other/x"]);
    symlink(d.0.join("real"), d.0.join("link_to_dir")).unwrap();
    symlink("real/inner.txt", d.0.join("link_to_file")).unwrap();
    symlink("nowhere", d.0.join("dangling")).unwrap();
    symlink(".", d.0.join("real/up_self")).unwrap();
    symlink("..", d.0.join("real/up_parent")).unwrap();
    symlink("../other", d.0.join("real/sideways")).unwrap();
    let top = sorted(io::read_dir(&d.0).unwrap());
    let kind = |n: &str| top.iter().find(|e| e.0 == n).unwrap().1;
    assert!(kind("link_to_dir") && kind("real") && !kind("link_to_file") && !kind("dangling"));
    // Inside `real`: a link to itself and a link to its parent would loop; a link to a sibling would not.
    let inner = sorted(io::read_dir(&d.0.join("real")).unwrap());
    let kind = |n: &str| inner.iter().find(|e| e.0 == n).unwrap().1;
    assert!(!kind("up_self") && !kind("up_parent"), "shown as files, never expanded");
    assert!(kind("sideways"));
    // Reading through the link is a read of the same directory, and still sees the loops.
    let via_link = sorted(io::read_dir(&d.0.join("link_to_dir")).unwrap());
    assert_eq!(via_link, inner);
}

#[test]
fn tree_5_a_big_directory_is_read_whole() {
    let d = temp_dir("io-readdir-big");
    for i in 0..3000 {
        std::fs::write(d.0.join(format!("f{i}")), "").unwrap();
    }
    assert_eq!(io::read_dir(&d.0).unwrap().len(), 3000);
}

fn files(dir: &std::path::Path) -> Vec<String> {
    let bytes = io::git_files(dir).unwrap();
    let mut v: Vec<String> = bytes
        .split(|&b| b == 0)
        .filter(|f| !f.is_empty())
        .map(|f| String::from_utf8_lossy(f).into_owned())
        .collect();
    v.sort();
    v
}

#[test]
fn find_3_git_files_lists_tracked_and_untracked_files_but_not_ignored_ones() {
    let d = temp_dir("io-gitfiles");
    repo(&d.0, &["src/main.rs", "README.md", ".gitignore", "target/debug/big.o"]);
    std::fs::write(d.0.join(".gitignore"), "target/\n*.log\n").unwrap();
    git(&d.0, &["rm", "-rq", "--cached", "target"]);
    make(&d.0, &["notes/new file.txt", "build.log", "src/日本.rs"]);
    assert_eq!(
        files(&d.0),
        [".gitignore", "README.md", "notes/new file.txt", "src/main.rs", "src/日本.rs"]
    );
}

#[test]
fn find_3_git_files_are_relative_to_the_directory_asked() {
    let d = temp_dir("io-gitfiles-sub");
    repo(&d.0, &["top.rs", "crates/sim/src/lib.rs", "crates/sim/Cargo.toml", "crates/other/x.rs"]);
    assert_eq!(files(&d.0.join("crates/sim")), ["Cargo.toml", "src/lib.rs"]);
}

#[test]
fn find_3_not_a_repository_is_a_clear_error_and_so_is_a_missing_directory() {
    let d = temp_dir("io-notrepo");
    let err = io::git_files(&d.0).unwrap_err();
    assert!(err.starts_with("not a git repository: ") && err.contains("io-notrepo"), "{err}");
    let err = io::git_files(&d.0.join("missing")).unwrap_err();
    assert!(err.starts_with("cannot run git in "), "{err}");
}

// ---- BUF-1: the state directory is si's (SAFE-6) ----

const WAIT: Duration = Duration::from_secs(5);

/// The journal key of a file: its canonical path.
fn key_of(p: &Path) -> String {
    si::io::file::resolve(&p.to_string_lossy()).to_string_lossy().into_owned()
}

fn mode(p: &Path, m: u32) {
    std::fs::set_permissions(p, std::fs::Permissions::from_mode(m)).unwrap();
}

/// A `Snapshot` effect of the file `name` holding `text`.
fn snapshot(name: &str, text: &str) -> io::Job {
    let rope = ropey::Rope::from_str;
    let (base, text, path) = (rope(""), rope(text), Some(name.to_string()));
    io::Job::App(1, Effect::Snapshot { path, base, text })
}

/// Runs the journal thread on `state` over `jobs` to its end; what it told the user.
fn journal(state: Option<&Path>, jobs: Vec<io::Job>) -> Vec<String> {
    let (tx, rx) = mpsc::channel();
    let (send, finished) = io::journal_thread(state.map(Path::to_path_buf), None, tx);
    jobs.into_iter().for_each(|j| send.send(j).unwrap());
    drop(send);
    assert_eq!(finished.recv_timeout(WAIT), Err(RecvTimeoutError::Disconnected));
    let notices =
        rx.try_iter().filter_map(|e| if let SimEvent::Notice(m) = e { Some(m) } else { None });
    notices.collect()
}

#[test]
fn buf_1_the_snapshot_directory_is_si_s_and_it_is_made_private() {
    let d = temp_dir("snap-ok");
    let state = journal::state_dir(Some(d.0.clone().into()), None).unwrap();
    assert_eq!(state, d.0.join("si"), "the directory of si, so that si and sim recover each other");
    assert_eq!(io::state_dir(Some(d.0.clone().into()), None), Some(d.0.join("sim")), "the history");
    assert!(journal(Some(&state), vec![snapshot("a.txt", "x")]).is_empty());
    assert_eq!(std::fs::metadata(&state).unwrap().mode() & 0o777, 0o700, "made 0700");
    let key = key_of(Path::new("a.txt"));
    let file = journal::file(&state, &key, std::process::id());
    assert_eq!(std::fs::metadata(&file).unwrap().mode() & 0o777, 0o600);
}

#[test]
fn buf_1_without_a_state_directory_there_are_no_snapshots_and_never_a_shared_fallback() {
    // si's rule, which sim calls: no absolute $XDG_STATE_HOME or $HOME, no directory.
    for (xdg, home) in [(None, None), (Some("rel/state"), Some("rel/home")), (Some(""), Some(""))] {
        assert_eq!(journal::state_dir(xdg.map(Into::into), home.map(Into::into)), None);
        assert_eq!(io::state_dir(xdg.map(Into::into), home.map(Into::into)), None);
    }
    // The journal thread tells it once, however many snapshots it is asked for, and writes nothing.
    let told = journal(None, vec![snapshot("a.txt", "x"), snapshot("a.txt", "y")]);
    assert_eq!(told.len(), 1, "{told:?}");
    assert!(told[0].starts_with("cannot write snapshots: no state directory"), "{told:?}");
    // A save that needs a recovery copy (SAFE-2: a file with two hard links) fails instead of
    // writing one anywhere; a save that does not need one works.
    let d = temp_dir("snap-off");
    let (tx, rx) = mpsc::channel();
    let saves = io::save_thread(None, tx);
    let (plain, linked) = (d.0.join("plain.txt"), d.0.join("linked.txt"));
    std::fs::write(&plain, "old\n").unwrap();
    std::fs::write(&linked, "old\n").unwrap();
    std::fs::hard_link(&linked, d.0.join("other.txt")).unwrap();
    for (id, f) in [(1, &plain), (2, &linked)] {
        saves.send((id, f.to_string_lossy().into_owned(), b"new\n".to_vec(), None)).unwrap();
    }
    let answers: Vec<_> = (0..2).map(|_| rx.recv_timeout(WAIT).unwrap()).collect();
    match &answers[..] {
        [
            SimEvent::App(1, si::core::app::Event::Saved(Ok(_), _)),
            SimEvent::App(2, si::core::app::Event::Saved(Err(e), _)),
        ] => {
            assert!(e.contains("no recovery copy possible"), "{e}");
        }
        a => panic!("{a:?}"),
    }
    assert_eq!(std::fs::read_to_string(&plain).unwrap(), "new\n");
    assert_eq!(std::fs::read_to_string(&linked).unwrap(), "old\n", "nothing was written");
}

#[test]
fn buf_1_a_directory_that_others_can_enter_or_that_is_a_link_is_never_used() {
    let d = temp_dir("snap-open");
    let si = d.0.join("si");
    std::fs::create_dir_all(&si).unwrap();
    // A snapshot of a.txt, made while the directory was private.
    let key = key_of(&d.0.join("a.txt"));
    let dead = u32::MAX / 2; // a snapshot of a process that still runs is not offered
    let snap = Snapshot { path: key, pid: dead, time_ms: 1, base: vec![], text: b"t".to_vec() };
    mode(&si, 0o700);
    journal::write(&si, &snap).unwrap();
    let found = |state: &Path| match io::open(Some(state), 0, "a.txt", &d.0.join("a.txt")) {
        SimEvent::Opened(o) => o.found.is_some(),
        e => panic!("{e:?}"),
    };
    assert!(found(&si), "private: offered");
    for m in [0o755, 0o710, 0o701, 0o777] {
        mode(&si, m);
        assert!(!found(&si), "{m:o}: not looked at");
        let told = journal(Some(&si), vec![snapshot("b.txt", "x")]);
        assert!(told.len() == 1 && told[0].contains("is not private"), "{m:o}: {told:?}");
        let names = std::fs::read_dir(&si).unwrap().count();
        assert_eq!(names, 1, "{m:o}: nothing was written there");
        // A save that needs a recovery copy (two hard links) fails rather than put one there.
        let (tx, rx) = mpsc::channel();
        let saves = io::save_thread(Some(si.clone()), tx);
        let (f, other) = (d.0.join("linked.txt"), d.0.join("other.txt"));
        let _ = std::fs::remove_file(&other);
        std::fs::write(&f, "old\n").unwrap();
        std::fs::hard_link(&f, &other).unwrap();
        saves.send((1, f.to_string_lossy().into_owned(), b"new\n".to_vec(), None)).unwrap();
        match rx.recv_timeout(WAIT).unwrap() {
            SimEvent::App(1, si::core::app::Event::Saved(Err(e), _)) => {
                assert!(e.contains("no recovery copy possible"), "{m:o}: {e}");
            }
            e => panic!("{m:o}: {e:?}"),
        }
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "old\n");
        assert_eq!(std::fs::read_dir(&si).unwrap().count(), 1, "{m:o}: still nothing there");
    }
    // A link to a private directory is still a link, not a real directory.
    mode(&si, 0o700);
    let link = d.0.join("link");
    symlink(&si, &link).unwrap();
    assert!(!found(&link), "a link is not looked through");
    let told = journal(Some(&link), vec![snapshot("b.txt", "x")]);
    assert!(told.len() == 1 && told[0].contains("is not private"), "{told:?}");
}

#[test]
fn buf_1_the_save_thread_numbers_each_answer_when_its_save_is_done() {
    // PROP-1: a read of the disk takes its number as it starts, a save's answer when the save is
    // done, from one counter for every thread; a read that began before the answer is older.
    let d = temp_dir("save-seq");
    let (tx, rx) = mpsc::channel();
    let saves = io::save_thread(Some(d.0.join("state")), tx);
    let f = d.0.join("f.txt");
    std::fs::write(&f, "old\n").unwrap();
    let began = si::io::file::seq(); // a watcher's read that starts before the save
    saves.send((1, f.to_string_lossy().into_owned(), b"new\n".to_vec(), None)).unwrap();
    saves.send((2, f.to_string_lossy().into_owned(), b"newer\n".to_vec(), Some(0))).unwrap(); // refused
    let seq = |e: SimEvent| match e {
        SimEvent::App(_, Event::Saved(_, n) | Event::SaveConflict(_, _, n)) => n,
        e => panic!("{e:?}"),
    };
    let (first, second) =
        (seq(rx.recv_timeout(WAIT).unwrap()), seq(rx.recv_timeout(WAIT).unwrap()));
    assert!(began < first && first < second, "{began} < {first} < {second}");
    assert!(second < si::io::file::seq(), "and the counter goes on from there");
}

#[test]
fn buf_1_the_journal_thread_remembers_where_a_file_was_closed_and_answers_a_reload_with_it() {
    // CLI-6: `Remember` saves the place (reading the others' again first), `Open` looks it up and
    // the answer is tagged with the id of the file that asked.
    let d = temp_dir("places");
    let state = journal::state_dir(Some(d.0.clone().into()), None).unwrap();
    std::fs::create_dir_all(&state).unwrap();
    mode(&state, 0o700);
    let [a, b, c] = ["a.txt", "b.txt", "c.txt"].map(|n| d.0.join(n).to_string_lossy().into_owned());
    let (tx, rx) = mpsc::channel();
    let (send, finished) = io::journal_thread(Some(state.clone()), None, tx);
    let remember =
        |p: &str, line, col| io::Job::App(1, Effect::Remember { path: p.into(), line, col });
    send.send(remember(&a, 4, 2)).unwrap();
    send.send(remember(&b, 0, 7)).unwrap();
    send.send(io::Job::App(5, Effect::Open(a.clone()))).unwrap();
    send.send(io::Job::App(6, Effect::Open(c))).unwrap(); // never closed: no answer
    drop(send);
    assert_eq!(finished.recv_timeout(WAIT), Err(RecvTimeoutError::Disconnected));
    assert_eq!(journal::places(&state).len(), 2);
    assert_eq!(journal::place(&state, &key_of(Path::new(&a))), Some((4, 2)));
    let answers: Vec<_> = rx.try_iter().collect();
    match &answers[..] {
        [SimEvent::App(5, Event::Place(p, 4, 2))] => assert_eq!(p, &a),
        a => panic!("{a:?}"),
    }
    // The first open of a file finds its place too, and brings it with the file (`Opened`).
    match io::open(Some(&state), 3, "a.txt", Path::new(&a)) {
        SimEvent::Opened(o) => assert_eq!(o.place, Some((4, 2))),
        e => panic!("{e:?}"),
    }
}
