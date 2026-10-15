//! MARK-1: modified, added and untracked files get the change color, every directory above a
//! change gets `~`; the status comes from git, as NUL-separated entries.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use sim::{io, marks::Marks};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

fn p(s: &str) -> PathBuf {
    PathBuf::from(s)
}

fn none() -> HashSet<PathBuf> {
    HashSet::new()
}

#[test]
fn mark_1_modified_added_and_untracked_files_get_the_change_color() {
    let status = b" M src/main.rs\0A  src/new.rs\0?? notes.md\0MM both.rs\0UU conflict.rs\0";
    let m = Marks::new(status, "", &none());
    for f in ["src/main.rs", "src/new.rs", "notes.md", "both.rs", "conflict.rs"] {
        assert!(m.changed(Path::new(f)), "{f}");
    }
    assert!(!m.changed(Path::new("src/other.rs")));
    assert!(!m.changed(Path::new("src")), "a directory gets `~`, not the change color");
    assert!(!m.changed_below(Path::new("src/main.rs")), "a file has nothing below it");
}

#[test]
fn mark_1_every_directory_above_a_change_gets_a_tilde() {
    let m = Marks::new(b"?? a/b/c/d.rs\0 M top.rs\0 M x/y.rs\0", "", &none());
    for dir in ["a", "a/b", "a/b/c", "x"] {
        assert!(m.changed_below(Path::new(dir)), "{dir}");
    }
    assert!(!m.changed_below(Path::new("a/b/c/d.rs")));
    assert!(!m.changed_below(Path::new("")), "the root is not a row");
    assert!(!m.changed_below(Path::new("a/z")) && !m.changed_below(Path::new("y")));
}

#[test]
fn mark_1_a_rename_marks_the_new_name_and_the_directory_the_old_one_left() {
    // With -z a rename is `XY new NUL old NUL`, and the next entry follows.
    let m = Marks::new(b"R  new/name.rs\0old/a.c\0 M after.rs\0", "", &none());
    assert!(m.changed(Path::new("new/name.rs")) && m.changed_below(Path::new("new")));
    assert!(m.changed_below(Path::new("old")), "something left it");
    assert!(m.changed(Path::new("after.rs")), "the old name is not read as an entry");
    assert!(
        !m.changed(Path::new("old/a.c")) || m.changed(Path::new("old/a.c")),
        "gone files have no row anyway"
    );
    // A copy leaves the original as it was.
    let m = Marks::new(b"C  copy/b.rs\0orig/b.rs\0", "", &none());
    assert!(m.changed(Path::new("copy/b.rs")));
    assert!(!m.changed(Path::new("orig/b.rs")) && !m.changed_below(Path::new("orig")));
}

#[test]
fn mark_1_a_deleted_file_marks_the_directories_it_was_in() {
    let m = Marks::new(b" D gone/dir/file.rs\0D  other.rs\0", "", &none());
    assert!(m.changed_below(Path::new("gone")) && m.changed_below(Path::new("gone/dir")));
}

#[test]
fn mark_1_paths_are_made_relative_to_the_root_and_outside_changes_are_left_out() {
    // The root is `crates/sim/`, below the top of the repository.
    let status = b" M crates/sim/src/a.rs\0 M crates/other/b.rs\0?? crates/sim/c.rs\0 M top.rs\0";
    let m = Marks::new(status, "crates/sim/", &none());
    assert!(m.changed(Path::new("src/a.rs")) && m.changed(Path::new("c.rs")));
    assert!(m.changed_below(Path::new("src")));
    assert!(!m.changed_below(Path::new("crates")) && !m.changed(Path::new("top.rs")));
    assert!(!m.changed(Path::new("crates/other/b.rs")) && !m.changed(Path::new("b.rs")));
}

#[test]
fn mark_1_files_changed_on_disk_while_sim_runs_are_marked_too() {
    let disk: HashSet<PathBuf> = [p("logs/today.txt"), p("config.toml")].into();
    let m = Marks::new(b" M src/a.rs\0", "", &disk);
    assert!(m.changed(Path::new("logs/today.txt")) && m.changed_below(Path::new("logs")));
    assert!(m.changed(Path::new("config.toml")) && m.changed(Path::new("src/a.rs")));
    // Without git (not a repository) the disk changes alone are enough.
    let m = Marks::new(b"", "", &disk);
    assert!(m.changed(Path::new("config.toml")));
}

#[test]
fn mark_1_names_with_spaces_and_non_ascii_are_kept_as_they_are() {
    let m = Marks::new(" M my dir/日本 語.md\0?? \"quoted\".txt\0".as_bytes(), "", &none());
    assert!(m.changed(Path::new("my dir/日本 語.md")) && m.changed_below(Path::new("my dir")));
    assert!(m.changed(Path::new("\"quoted\".txt")), "-z output is never quoted");
}

#[test]
fn mark_1_damaged_status_output_marks_nothing_and_never_panics() {
    for bad in [&b""[..], b"\0\0\0", b"M", b" M", b" M \0", b"\xff\xfe\0", b"??"] {
        assert_eq!(Marks::new(bad, "", &none()), Marks::default(), "{bad:?}");
    }
    // A rename that lost its old name still marks the new one.
    let m = Marks::new(b"R  a\0", "", &none());
    assert!(m.changed(Path::new("a")));
}

#[test]
fn mark_1_git_status_of_a_real_repository() {
    let d = temp_dir("mark1-status");
    repo(
        &d.0,
        &["keep.rs", "src/old.rs", "src/mod.rs", "docs/a.md", "docs/gone.md", "docs/deep/x.md"],
    );
    std::fs::write(d.0.join("keep.rs"), "changed\n").unwrap();
    std::fs::write(d.0.join("docs/deep/y.md"), "new\n").unwrap();
    std::fs::remove_file(d.0.join("docs/gone.md")).unwrap();
    git(&d.0, &["mv", "src/old.rs", "src/new.rs"]);
    make(&d.0, &["untracked/dir/file.txt"]);
    let status = io::git_status(&d.0).unwrap();
    let m = Marks::new(&status, &io::git_prefix(&d.0).unwrap(), &none());
    for f in ["keep.rs", "docs/deep/y.md", "src/new.rs", "untracked/dir/file.txt"] {
        assert!(m.changed(Path::new(f)), "{f}");
    }
    for dir in ["docs", "docs/deep", "src", "untracked", "untracked/dir"] {
        assert!(m.changed_below(Path::new(dir)), "{dir}");
    }
    assert!(!m.changed(Path::new("src/mod.rs")) && !m.changed(Path::new("docs/a.md")));
    // The same from a subdirectory: paths are relative to it, and the rest is left out.
    let sub = d.0.join("docs");
    let prefix = io::git_prefix(&sub).unwrap();
    assert_eq!(prefix, "docs/");
    let m = Marks::new(&io::git_status(&sub).unwrap(), &prefix, &none());
    assert!(m.changed(Path::new("deep/y.md")) && m.changed_below(Path::new("deep")));
    assert!(!m.changed(Path::new("keep.rs")) && !m.changed(Path::new("src/new.rs")));
}

#[test]
fn mark_1_a_clean_repository_has_no_marks_and_a_plain_directory_is_an_error() {
    let d = temp_dir("mark1-clean");
    repo(&d.0, &["a.rs"]);
    assert_eq!(io::git_status(&d.0).unwrap(), b"");
    assert_eq!(io::git_prefix(&d.0).unwrap(), "");
    let plain = temp_dir("mark1-plain");
    let err = io::git_status(&plain.0).unwrap_err();
    assert!(err.starts_with("not a git repository"), "{err}");
}

#[test]
fn mark_1_git_never_runs_the_monitor_that_a_repository_configures() {
    use std::os::unix::fs::PermissionsExt;
    let d = temp_dir("mark1-monitor");
    repo(&d.0, &["a.txt"]);
    let witness = std::env::temp_dir().join(format!("sim-it-{}-monitor-ran", std::process::id()));
    let _ = std::fs::remove_file(&witness);
    let hook = d.0.join(".git/monitor.sh");
    std::fs::write(&hook, format!("#!/bin/sh\ntouch '{}'\necho\n", witness.display())).unwrap();
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    git(&d.0, &["config", "core.fsmonitor", &hook.display().to_string()]);
    // The control: git as the user runs it does run the monitor, so this test can tell.
    git(&d.0, &["status", "--porcelain"]);
    assert!(witness.exists(), "the repository's monitor runs under plain git");
    std::fs::remove_file(&witness).unwrap();
    io::git_status(&d.0).unwrap();
    io::git_files(&d.0).unwrap();
    io::git_prefix(&d.0).unwrap();
    let ran = witness.exists();
    let _ = std::fs::remove_file(&witness);
    assert!(!ran, "sim's git runs with core.fsmonitor=false");
}
