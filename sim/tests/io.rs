//! TREE-5, FIND-3: reading a directory, and listing the files of a repository.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use sim::io;
use sim::tree::Entry;
use std::os::unix::fs::symlink;

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
