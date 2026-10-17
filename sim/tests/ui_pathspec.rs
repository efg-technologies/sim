//! UI-5, MARK-1: a file's name is a path and never a pattern, and the sign column's diff never enters a
//! submodule. Git reads the name after `--` as a pathspec: a file named `*` or `:(top)` would match
//! the whole directory or repository, a submodule included; and a superproject's `diff.submodule =
//! diff` makes git start a child `git diff` in a submodule whose commit moved, under the submodule's
//! own config (`.git/modules/<name>/config`), which the guard of MARK-1 never read. The witness
//! tests make a submodule whose own config names a script that leaves a witness file: the control
//! (git run as sim ran it before) makes the witness, sim's call does not, and still answers.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use sim::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// A script that appends its name and arguments to `witness`.
fn script(dir: &Path, name: &str, witness: &Path) -> String {
    let at = dir.join(name);
    let body = format!("#!/bin/sh\necho \"{name} $*\" >> '{}'\n", witness.display());
    std::fs::write(&at, body).unwrap();
    std::fs::set_permissions(&at, PermissionsExt::from_mode(0o755)).unwrap();
    at.to_string_lossy().into_owned()
}

fn ran(witness: &Path) -> Vec<String> {
    let text = std::fs::read_to_string(witness).unwrap_or_default();
    text.lines().map(|l| l.trim_end().to_string()).collect()
}

/// A superproject with a submodule whose commit has moved, and a repository that asks for a
/// child git: `diff.submodule = diff`. The submodule's own config names an external diff and a file
/// system monitor, each a script that leaves a witness. The files of the superproject are `a.txt`,
/// `b.txt`, `[ab].txt` and `*`, committed, then each one changed.
struct Evil {
    sup: PathBuf,
    witness: PathBuf,
    _keep: Vec<TempDir>,
}

fn evil(name: &str) -> Evil {
    let (src, sup, tools) = (
        temp_dir(&format!("{name}-src")),
        temp_dir(&format!("{name}-super")),
        temp_dir(&format!("{name}-tools")),
    );
    let witness = tools.0.join("witness");
    git(&src.0, &["init", "-q"]);
    for text in ["one\n", "two\n"] {
        std::fs::write(src.0.join("f"), text).unwrap();
        git(&src.0, &["add", "f"]);
        git(&src.0, &["commit", "-q", "-m", text]);
    }
    git(&sup.0, &["init", "-q"]);
    let url = src.0.to_string_lossy().into_owned();
    git(&sup.0, &["-c", "protocol.file.allow=always", "submodule", "add", "-q", &url, "sub"]);
    // the superproject records the first commit; the submodule has the second checked out
    git(&sup.0.join("sub"), &["checkout", "-q", "HEAD~1"]);
    for f in ["a.txt", "b.txt", "[ab].txt", "*"] {
        std::fs::write(sup.0.join(f), format!("{f}\n")).unwrap();
    }
    git(&sup.0, &["add", "-A"]);
    git(&sup.0, &["commit", "-q", "-m", "top"]);
    git(&sup.0.join("sub"), &["checkout", "-q", "main"]);
    // the submodule's own config (not the repository's, which the guard reads)
    let own = sup.0.join(".git/modules/sub/config");
    let own = own.to_string_lossy().into_owned();
    git(&sup.0, &["config", "-f", &own, "diff.external", &script(&tools.0, "extdiff", &witness)]);
    git(&sup.0, &["config", "-f", &own, "core.fsmonitor", &script(&tools.0, "monitor", &witness)]);
    // the superproject's own config: nothing that holds a command
    git(&sup.0, &["config", "diff.submodule", "diff"]);
    for f in ["a.txt", "b.txt", "[ab].txt", "*"] {
        std::fs::write(sup.0.join(f), format!("{f}\nchanged\n")).unwrap();
    }
    Evil { sup: sup.0.clone(), witness, _keep: vec![src, sup, tools] }
}

/// Git run as sim ran the sign column's diff before: the name after `--` as it was given.
fn control(dir: &Path, name: &str) {
    let out = Command::new("git")
        .args(["-c", "core.fsmonitor=false", "--no-optional-locks"])
        .args(["diff", "--no-ext-diff", "--no-textconv", "--no-color", "-U0", "HEAD", "--", name])
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("XDG_CONFIG_HOME", "/nonexistent")
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn ui_5_a_name_that_is_a_glob_or_pathspec_magic_runs_nothing_in_a_submodule() {
    for name in ["*", ":(top)"] {
        let e = evil("ui5-glob");
        // the control: the names match the submodule, and the child git runs its own config
        control(&e.sup, name);
        assert!(
            ran(&e.witness).iter().any(|l| l.starts_with("extdiff ")),
            "{name}: the control makes the witness: {:?}",
            ran(&e.witness)
        );
        let _ = std::fs::remove_file(&e.witness);
        // sim's call answers, and nothing the submodule's config names runs
        let out = io::git_diff(&e.sup, name);
        assert!(out.is_ok(), "{name}: {out:?}");
        assert_eq!(ran(&e.witness), Vec::<String>::new(), "{name}");
    }
}

#[test]
fn ui_5_a_name_is_a_literal_path_and_the_sign_diff_is_that_file_s_alone() {
    let e = evil("ui5-literal");
    let text = |name: &str| String::from_utf8(io::git_diff(&e.sup, name).unwrap()).unwrap();
    let files = ["a.txt", "b.txt", "[ab].txt", "*"];
    for name in files {
        let diff = text(name);
        // only this file's own hunk: its name's old line stays, and one line is added
        assert_eq!(
            diff.matches("\n@@").count() + usize::from(diff.starts_with("@@")),
            1,
            "{name}: {diff}"
        );
        assert!(diff.contains("+changed"), "{name}: {diff}");
        let heads: Vec<&str> = diff.lines().filter(|l| l.starts_with("diff --git")).collect();
        assert_eq!(heads.len(), 1, "{name}: only its own file is in the diff: {heads:?}");
        assert!(heads[0].ends_with(&format!("b/{name}")), "{name}: {heads:?}");
    }
    // a name that matches nothing, as a pattern would match files, has no hunks
    assert_eq!(io::git_diff(&e.sup, "[ab]*"), Ok(vec![]), "a pattern is a name that no file has");
    assert_eq!(io::git_diff(&e.sup, ":(top)a.txt"), Ok(vec![]), "and so is magic");
}

#[test]
fn ui_5_the_sign_diff_never_enters_a_submodule_not_even_one_named_exactly_or_dirty() {
    let e = evil("ui5-dirty");
    // a submodule with its own changes: git would ask it (a child `git status` or `git diff` in it)
    std::fs::write(e.sup.join("sub/f"), "three\n").unwrap();
    control(&e.sup, "sub");
    assert!(
        !ran(&e.witness).is_empty(),
        "the control: git enters the submodule: {:?}",
        ran(&e.witness)
    );
    let _ = std::fs::remove_file(&e.witness);
    assert_eq!(io::git_diff(&e.sup, "sub"), Ok(vec![]), "a submodule has no signs: it is no file");
    assert_eq!(ran(&e.witness), Vec::<String>::new());
}

#[test]
fn mark_1_every_git_runs_with_literal_pathspecs_and_a_short_submodule_diff() {
    let e = evil("mark1-env");
    // the one builder of the command lines (GIT-1): the guard reads the config around `sup` first, and
    // the superproject's `diff.submodule = diff` is no key that holds a command
    let c = io::git_command(&e.sup, &["status"]).unwrap();
    let env: Vec<(String, String)> = c
        .get_envs()
        .filter_map(|(k, v)| Some((k.to_string_lossy().into_owned(), v?.to_string_lossy().into())))
        .collect();
    assert!(env.contains(&("GIT_LITERAL_PATHSPECS".into(), "1".into())), "{env:?}");
    let args: Vec<String> = c.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
    let pair = |k: &str, v: &str| args.windows(2).any(|w| w[0] == k && w[1] == v);
    assert!(pair("-c", "diff.submodule=short"), "{args:?}");
    assert!(pair("-c", "core.fsmonitor=false") && args.contains(&"--no-optional-locks".into()));
    // every diff of sim's takes no submodule in either way: the sign column's, and the numbers'
    let line = |args: &[&str]| -> Vec<String> {
        let c = io::git_command(&e.sup, args).unwrap();
        c.get_args().map(|a| a.to_string_lossy().into_owned()).collect()
    };
    let signs = line(&io::git_diff_args("a.txt"));
    assert!(signs.contains(&"--ignore-submodules=all".to_string()), "{signs:?}");
    assert_eq!(signs.last().map(String::as_str), Some("a.txt"), "{signs:?}");
    assert_eq!(signs[signs.len() - 2], "--", "the name comes after `--`: {signs:?}");
    let numbers = line(&["diff", "--numstat", "-z", "HEAD"]);
    assert!(numbers.contains(&"--ignore-submodules=all".to_string()), "{numbers:?}");
    // a status sees a submodule as one entry (its commit), and no more
    let status = line(&["status"]);
    assert!(status.contains(&"--ignore-submodules=dirty".to_string()), "{status:?}");
}
