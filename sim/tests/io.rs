//! TREE-5, FIND-3, BUF-1: reading a directory, listing the files of a repository, and the
//! private directory that snapshots go to.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use sim::io;
use sim::tree::Entry;
use std::ffi::OsString;
use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
use std::path::Path;

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

// ---- BUF-1: the directory of snapshots (SAFE-6) ----

fn os(s: &str) -> Option<OsString> {
    Some(OsString::from(s))
}

fn mode(p: &Path, m: u32) {
    std::fs::set_permissions(p, std::fs::Permissions::from_mode(m)).unwrap();
}

#[test]
fn buf_1_the_snapshot_directory_is_si_s_and_it_is_made_private() {
    let d = temp_dir("snap-ok");
    let got = io::snapshot_dir(Some(d.0.clone().into()), os("/home/nobody")).unwrap();
    assert_eq!(got, d.0.join("si"));
    let m = std::fs::metadata(&got).unwrap();
    assert_eq!(m.mode() & 0o777, 0o700);
    assert_eq!(io::private(&got).unwrap(), m.uid(), "the uid of the probe is the owner's");
    assert!(std::fs::read_dir(&got).unwrap().next().is_none(), "the probe is gone");
    // From $HOME when there is no $XDG_STATE_HOME; the same directory the second time.
    let home = temp_dir("snap-home");
    let got = io::snapshot_dir(None, Some(home.0.clone().into())).unwrap();
    assert_eq!(got, home.0.join(".local/state/si"));
    assert_eq!(io::snapshot_dir(None, Some(home.0.clone().into())).unwrap(), got);
}

#[test]
fn buf_1_without_a_state_directory_there_are_no_snapshots_and_never_a_shared_fallback() {
    for (xdg, home) in
        [(None, None), (os("rel/state"), os("rel/home")), (os(""), os("")), (os("rel"), None)]
    {
        let err = io::snapshot_dir(xdg.clone(), home.clone()).unwrap_err();
        assert!(err.contains("neither $XDG_STATE_HOME nor $HOME"), "{xdg:?} {home:?}: {err}");
    }
    // A relative $XDG_STATE_HOME is ignored (the XDG rule), and $HOME is used.
    let home = temp_dir("snap-rel");
    let got = io::snapshot_dir(os("relative"), Some(home.0.clone().into())).unwrap();
    assert!(got.starts_with(&home.0));
    assert!(!std::env::temp_dir().join("si").starts_with(&got));
}

#[test]
fn buf_1_a_directory_that_others_can_enter_or_that_is_a_link_is_refused() {
    let d = temp_dir("snap-open");
    let si = d.0.join("si");
    std::fs::create_dir_all(&si).unwrap();
    for m in [0o755, 0o710, 0o701, 0o777] {
        mode(&si, m);
        let err = io::snapshot_dir(Some(d.0.clone().into()), None).unwrap_err();
        assert!(err.contains("is not private"), "{m:o}: {err}");
    }
    mode(&si, 0o700);
    assert!(io::snapshot_dir(Some(d.0.clone().into()), None).is_ok());
    // A symlink to a directory that is private is still a link: not a real directory.
    let d = temp_dir("snap-link");
    let real = d.0.join("real");
    std::fs::create_dir_all(&real).unwrap();
    mode(&real, 0o700);
    symlink(&real, d.0.join("si")).unwrap();
    let err = io::snapshot_dir(Some(d.0.clone().into()), None).unwrap_err();
    assert!(err.contains("is not private"), "{err}");
    // And a plain file in the way.
    let d = temp_dir("snap-file");
    make(&d.0, &["si"]);
    assert!(io::snapshot_dir(Some(d.0.clone().into()), None).is_err());
}

#[test]
fn buf_1_a_snapshot_file_of_someone_else_is_never_read() {
    let d = temp_dir("snap-foreign");
    let state = io::snapshot_dir(Some(d.0.clone().into()), None).unwrap();
    let uid = std::fs::metadata(&state).unwrap().uid();
    let key = "/p/a.txt";
    assert!(!io::foreign(&state, key, uid), "nothing there");
    let snap = si::io::journal::Snapshot {
        path: key.into(),
        pid: 1,
        time_ms: 1,
        base: b"b".to_vec(),
        text: b"t".to_vec(),
    };
    si::io::journal::write(&state, &snap).unwrap();
    assert!(!io::foreign(&state, key, uid), "ours");
    assert!(io::foreign(&state, key, uid + 1), "had it been someone else's");
    assert!(!io::foreign(&state, "/p/other.txt", uid + 1), "other files have their own");
}

#[test]
fn buf_1_with_snapshots_off_saves_still_work_and_nothing_is_written_anywhere_shared() {
    let d = temp_dir("snap-off");
    let (tx, rx) = std::sync::mpsc::channel();
    let saves = io::save_thread(None, tx.clone());
    let (jobs, finished) = io::journal_thread(None, None, tx);
    let target = d.0.join("f.txt");
    std::fs::write(&target, "old\n").unwrap();
    saves.send((1, target.to_string_lossy().into_owned(), b"new\n".to_vec(), None)).unwrap();
    match rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap() {
        sim::sim::SimEvent::App(1, si::core::app::Event::Saved(Ok(_))) => {}
        e => panic!("{e:?}"),
    }
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "new\n");
    // The journal thread takes its jobs and does nothing with them.
    jobs.send(io::Job::Snap(1, si::core::app::Effect::Unsnapshot)).unwrap();
    drop(jobs);
    assert!(matches!(
        finished.recv_timeout(std::time::Duration::from_secs(5)),
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected)
    ));
    assert!(!Path::new(io::DEAD_STATE).exists());
    assert!(
        std::fs::create_dir_all(io::DEAD_STATE).is_err(),
        "recovery copies cannot be made there"
    );
}
