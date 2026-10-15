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

// ---- the guard: git is not run where the repository's own config could run commands ----

/// Configs that define a command git would run, or could bring one in.
const RUNS: [&str; 15] = [
    "[filter \"lfs\"]\n\tclean = git-lfs clean -- %f\n",
    "[filter \"x\"]\n",
    "[FILTER \"x\"]\n\tSmudge = cat\n",
    "[filter.x]\n\tprocess = cat\n",
    "[core]\n\tfsmonitor = /tmp/monitor\n",
    "[core]\n\tFsMonitor=true\n",
    "[core] fsmonitor = true\n",
    "[core]\n\tbare = false\n\tfsmonitor = false\n",
    "[core]\n\thooksPath = /tmp/hooks\n",
    "[Core]\n\thookspath\n",
    "[include]\n\tpath = ../other\n",
    "[includeIf \"gitdir:~/work/\"]\n\tpath = work\n",
    "\u{feff}[core]\n\tfsmonitor = true\n",
    "[user]\r\n\tname = a\r\n[core]\r\n\tfsmonitor = true\r\n",
    "[alias]\n\tx = y \\\n[filter]\n",
];

/// Configs that cannot run anything in `status` or `ls-files`.
const SAFE: [&str; 5] = [
    "",
    "[core]\n\trepositoryformatversion = 0\n\tfilemode = true\n\tbare = false\n\tlogallrefupdates = true\n\
     [remote \"origin\"]\n\turl = https://example.com/filter.git\n\tfetch = +refs/heads/*:refs/remotes/origin/*\n\
     [branch \"main\"]\n\tremote = origin\n[user]\n\tname = filter\n\temail = include@example.com\n",
    "# [filter \"x\"]\n; fsmonitor = true\n[core]\n\t# hooksPath = x\n\tautocrlf = true\n",
    "[remote \"origin\"]\n\turl = x\n\tfsmonitor = not-core\n\thookspath = nor-this\n",
    "[alias]\n\tst = status\n[diff]\n\talgorithm = patience\n[core.fsmonitor]\n\tx = 1\n",
];

fn skipped(dir: &Path) {
    let skip = || io::GIT_SKIPPED.to_string();
    assert_eq!(io::git_status(dir), Err(skip()), "status");
    assert_eq!(io::git_files(dir), Err(skip()), "ls-files");
    assert_eq!(io::git_prefix(dir), Err(skip()), "rev-parse");
}

/// A repository with `a.rs`, and `target/x.o` that is ignored.
fn ignoring_repo(dir: &Path) {
    make(dir, &["a.rs", "target/x.o"]);
    std::fs::write(dir.join(".gitignore"), "target/\n").unwrap();
    git(dir, &["init", "-q"]);
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", "init"]);
}

#[test]
fn mark_1_a_config_that_could_run_commands_is_told_from_one_that_could_not() {
    for c in RUNS {
        assert!(io::runs_commands(c), "{c:?}");
    }
    for c in SAFE {
        assert!(!io::runs_commands(c), "{c:?}");
    }
}

#[test]
fn mark_1_git_is_not_run_in_a_repository_whose_config_could_run_commands() {
    for (i, config) in RUNS.iter().enumerate() {
        let d = temp_dir(&format!("mark1-unsafe-{i}"));
        ignoring_repo(&d.0);
        let mut text = std::fs::read_to_string(d.0.join(".git/config")).unwrap();
        text.push_str(config);
        std::fs::write(d.0.join(".git/config"), text).unwrap();
        skipped(&d.0);
        skipped(&d.0.join("target"));
        // The list of files comes from a walk then: ignored files and all (and never `.git`).
        let files = io::files(&d.0);
        assert!(files.contains(&"target/x.o".to_string()), "{config:?}: {files:?}");
        assert!(!files.iter().any(|f| f.starts_with(".git/")), "{files:?}");
    }
    for (i, config) in SAFE.iter().enumerate() {
        let d = temp_dir(&format!("mark1-safe-{i}"));
        ignoring_repo(&d.0);
        let mut text = std::fs::read_to_string(d.0.join(".git/config")).unwrap();
        text.push_str(config);
        std::fs::write(d.0.join(".git/config"), text).unwrap();
        assert_eq!(io::git_status(&d.0), Ok(vec![]), "{config:?}");
        assert!(!io::git_files(&d.0).unwrap().is_empty());
    }
}

#[test]
fn mark_1_a_filter_or_a_monitor_of_the_repository_never_runs() {
    use std::os::unix::fs::PermissionsExt;
    let d = temp_dir("mark1-witness");
    let witness = d.0.join("witness");
    let witness = witness.display();
    // A file git must read again: the same size, an mtime that moved.
    std::fs::write(d.0.join(".gitattributes"), "* filter=canary\n").unwrap();
    std::fs::write(d.0.join("a.txt"), "aaaa\n").unwrap();
    git(&d.0, &["init", "-q"]);
    git(&d.0, &["add", "-A"]);
    git(&d.0, &["commit", "-q", "-m", "init"]);
    std::fs::write(d.0.join("a.txt"), "bbbb\n").unwrap();
    let file = std::fs::File::options().write(true).open(d.0.join("a.txt")).unwrap();
    file.set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(60)).unwrap();
    let hook = d.0.join(".git/monitor.sh");
    std::fs::write(&hook, format!("#!/bin/sh\ntouch '{witness}-monitor'\necho\n")).unwrap();
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    git(&d.0, &["config", "filter.canary.clean", &format!("touch '{witness}-filter'; cat")]);
    git(&d.0, &["config", "core.fsmonitor", &hook.display().to_string()]);
    // The control: git as the user runs it does run both, so this test can tell.
    git(&d.0, &["status", "--porcelain"]);
    for what in ["filter", "monitor"] {
        let w = d.0.join(format!("witness-{what}"));
        assert!(w.exists(), "{what}: the repository's own command runs under plain git");
        std::fs::remove_file(w).unwrap();
    }
    skipped(&d.0);
    assert!(!d.0.join("witness-filter").exists() && !d.0.join("witness-monitor").exists());
}

/// The shell side of an unsafe config in a worktree's repository or a separate git directory.
fn unsafe_config(dir: &Path) {
    let text = std::fs::read_to_string(dir.join("config")).unwrap();
    std::fs::write(dir.join("config"), text + "[filter \"x\"]\n\tclean = cat\n").unwrap();
}

#[test]
fn mark_1_the_config_git_would_read_is_the_one_of_the_worktree_s_repository_or_the_named_directory()
{
    let d = temp_dir("mark1-worktree");
    let (main, wt, sep) = (d.0.join("main"), d.0.join("wt"), d.0.join("elsewhere.git"));
    std::fs::create_dir(&main).unwrap();
    repo(&main, &["a.rs"]);
    git(&main, &["worktree", "add", "-q", &wt.display().to_string()]);
    let configs: Vec<_> =
        io::git_configs(&wt).iter().map(|c| c.components().collect::<PathBuf>()).collect();
    let common = main.join(".git");
    let real = |c: &PathBuf| std::fs::canonicalize(c).ok();
    assert!(configs.iter().any(|c| real(c) == real(&common.join("config"))), "{configs:?}");
    assert!(configs.iter().all(|c| c.starts_with(&common)), "nothing else is read: {configs:?}");
    assert_eq!(io::git_prefix(&wt), Ok(String::new()), "safe: git runs");
    unsafe_config(&common);
    skipped(&wt);
    skipped(&main);
    // A `.git` file that names a directory somewhere else (--separate-git-dir, a submodule's).
    let work = d.0.join("work");
    std::fs::create_dir(&work).unwrap();
    git(&work, &["init", "-q", "--separate-git-dir", &sep.display().to_string()]);
    assert!(work.join(".git").is_file());
    assert_eq!(io::git_configs(&work)[0], sep.join("config"));
    assert_eq!(io::git_prefix(&work), Ok(String::new()));
    unsafe_config(&sep);
    skipped(&work);
    skipped(&work.join("sub")); // a directory that is not there yet is under the same repository
}

#[test]
fn mark_1_a_fake_git_directory_does_not_hide_the_repository_above_it() {
    let d = temp_dir("mark1-fake");
    repo(&d.0, &["a.rs", "sub/b.rs"]);
    let sub = d.0.join("sub");
    for fake in ["empty", "head"] {
        let dot = sub.join(".git");
        let _ = std::fs::remove_dir_all(&dot);
        std::fs::create_dir(&dot).unwrap();
        if fake == "head" {
            make(&dot, &["HEAD=not a ref\n", "objects/", "refs/"]);
        }
        // Git passes it by; so does the search for the config.
        assert_eq!(io::git_prefix(&sub), Ok("sub/".into()), "{fake}: git uses the outer one");
        let configs = io::git_configs(&sub);
        assert!(configs.contains(&d.0.join(".git/config")), "{fake}: {configs:?}");
    }
    unsafe_config(&d.0.join(".git"));
    skipped(&sub);
}

#[test]
fn mark_1_only_the_nearest_repository_counts() {
    let d = temp_dir("mark1-nested");
    let inner = d.0.join("inner");
    std::fs::create_dir(&inner).unwrap();
    repo(&d.0, &["a.rs"]);
    unsafe_config(&d.0.join(".git"));
    skipped(&d.0);
    git(&inner, &["init", "-q"]); // a repository of its own, with a safe config
    let configs = io::git_configs(&inner);
    assert!(configs.iter().all(|c| c.starts_with(inner.join(".git"))), "{configs:?}");
    assert_eq!(io::git_prefix(&inner), Ok(String::new()), "git runs there");
}

#[test]
fn mark_1_a_config_that_cannot_be_read_counts_as_one_that_could_run_commands() {
    use std::os::unix::fs::PermissionsExt;
    let d = temp_dir("mark1-unreadable");
    repo(&d.0, &["a.rs"]);
    let config = d.0.join(".git/config");
    std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o000)).unwrap();
    let readable = std::fs::read(&config).is_ok(); // the superuser reads anything
    assert_eq!(io::git_status(&d.0).is_err(), !readable);
    if !readable {
        skipped(&d.0);
    }
    std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert!(io::git_status(&d.0).is_ok());
    std::fs::remove_file(&config).unwrap(); // no config at all: nothing to run
    assert!(io::git_status(&d.0).is_ok());
}
