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
// The config is read by git itself (`git config --list --show-scope -z`), so the cases below are
// texts that git and a parser of sim's own could read differently.

/// Configs that define a command git would run, or could bring one in.
const RUNS: [&str; 32] = [
    "[filter \"lfs\"]\n\tclean = git-lfs clean -- %f\n",
    "[FILTER \"x\"]\n\tSmudge = cat\n",
    "[filter.x]\n\tprocess = cat\n",
    "[filter.x]\n\tclean = cat\n", // git's older way to write [filter "x"]
    "[Filter.X]\n\tClean = cat\n",
    "[filter \"a b\\\"c\\\\d ;# ]\"]\n\tclean = cat\n", // a subsection with odd characters
    "[filter]\n\tclean = cat\n",
    "[core]\n\tfsmonitor = /tmp/monitor\n",
    "[core]\n\tFsMonitor=true\n",
    "[Core] FsMonitor = /tmp/monitor\n",
    "[core] fsmonitor = true\n",
    "[core] ; a comment\n\tfsmonitor = true\n",
    "[core]\n\tbare = false\n\tfsmonitor = false\n",
    "[core]\n\tfsmonitor = \"true\" ; [user]\n",
    "[core]\n\tx = 1 \\\n[user]\n\tfsmonitor = true\n", // the header is the end of a continued value
    "[user]\n\tname = \"a\\\n[core]\"\n[core]\n\tfsmonitor = true\n",
    "[core]\n\thooksPath = /tmp/hooks\n",
    "[Core]\n\thookspath\n", // no value: git cannot read it at all
    "[core]\n\talternateRefsCommand = /tmp/refs\n",
    "[core]\n\tsshCommand = /tmp/ssh\n",
    "[core]\n\teditor = /tmp/ed\n\tpager = /tmp/pg\n",
    "[credential]\n\thelper = /tmp/cred\n",
    "[gpg]\n\tprogram = /tmp/gpg\n",
    "[diff \"x\"]\n\ttextconv = /tmp/conv\n",
    "[difftool \"x\"]\n\tCmd = /tmp/d\n",
    "[merge \"x\"]\n\tdriver = /tmp/m\n",
    "[future]\n\trunHook = /tmp/h\n", // a setting git does not have yet
    "[include]\n\tpath = ../other\n",
    "[includeIf \"gitdir:~/work/\"]\n\tpath = work\n",
    "[includeIf \"hasconfig:remote.*.url:*github.com*/**\"]\n\tpath = work\n",
    "\u{feff}[core]\n\tfsmonitor = true\n",
    "[user]\r\n\tname = a\r\n[core]\r\n\tfsmonitor = true\r\n",
];

/// Configs that cannot run anything in `status` or `ls-files`.
const SAFE: [&str; 8] = [
    "",
    "[core]\n\trepositoryformatversion = 0\n\tfilemode = true\n\tbare = false\n\tlogallrefupdates = true\n\
     [remote \"origin\"]\n\turl = https://example.com/filter.git\n\tfetch = +refs/heads/*:refs/remotes/origin/*\n\
     [branch \"main\"]\n\tremote = origin\n[user]\n\tname = filter\n\temail = include@example.com\n",
    "# [filter \"x\"]\n; fsmonitor = true\n[core]\n\t# hooksPath = x\n\tautocrlf = true\n",
    "[remote \"origin\"]\n\turl = x\n\tfetch = +refs/heads/*:refs/remotes/origin/*\n\
     [branch \"release-1.0\"]\n\tremote = origin\n\tmerge = refs/heads/main\n\
     [submodule \"sub\"]\n\turl = x\n\tactive = true\n[lfs]\n\turl = https://example.com/lfs\n\
     \tconcurrenttransfers = 4\n[core]\n\tignorecase = true\n\tautocrlf = true\n\
     [extensions]\n\tworktreeConfig = true\n[url \"https://x.example/\"]\n\tinsteadOf = x:\n",
    "[alias]\n\tst = status\n[diff]\n\talgorithm = patience\n[core.fsmonitor]\n\tx = 1\n",
    "[filter \"x\"]\n[include]\n[core]\n", // sections with nothing in them define nothing
    "[remote \"filter\"]\n\turl = x\n[branch \"include\"]\n\tremote = origin\n[remote \"o r\\\"i\\\\g;#\"]\n\turl = y\n",
    "[alias]\n\tx = y \\\n[filter]\n[core]\n\tautocrlf = true\n", // a header in a continued value
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

fn append(file: &Path, text: &str) {
    let old = std::fs::read_to_string(file).unwrap_or_default();
    std::fs::write(file, old + text).unwrap();
}

#[test]
fn mark_1_git_is_not_run_in_a_repository_whose_config_could_run_commands() {
    for (i, config) in RUNS.iter().enumerate() {
        let d = temp_dir(&format!("mark1-unsafe-{i}"));
        ignoring_repo(&d.0);
        append(&d.0.join(".git/config"), config);
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
        append(&d.0.join(".git/config"), config);
        assert_eq!(io::git_status(&d.0), Ok(vec![]), "{config:?}");
        assert!(!io::git_files(&d.0).unwrap().is_empty(), "{config:?}");
    }
}

#[test]
fn mark_1_an_included_file_counts_as_the_repository_s_own() {
    let d = temp_dir("mark1-include");
    ignoring_repo(&d.0);
    // `inc` defines a filter; the config that includes it says nothing else of its own.
    std::fs::write(d.0.join(".git/inc"), "[filter \"x\"]\n\tclean = cat\n").unwrap();
    let dir = format!("{}/.git/", d.0.display());
    for include in
        ["[include]\n\tpath = inc\n", &format!("[includeIf \"gitdir:{dir}\"]\n\tpath = inc\n")]
    {
        let config = d.0.join(".git/config");
        let safe = std::fs::read_to_string(&config).unwrap();
        append(&config, include);
        skipped(&d.0);
        std::fs::write(&config, safe).unwrap();
        assert!(io::git_status(&d.0).is_ok());
    }
}

#[test]
fn mark_1_the_config_worktree_file_counts_once_git_reads_it() {
    let d = temp_dir("mark1-config-worktree");
    ignoring_repo(&d.0);
    let worktree = d.0.join(".git/config.worktree");
    std::fs::write(&worktree, "[core]\n\tfsmonitor = true\n").unwrap();
    assert_eq!(io::git_status(&d.0), Ok(vec![]), "git does not read it yet");
    git(&d.0, &["config", "extensions.worktreeConfig", "true"]);
    skipped(&d.0);
    std::fs::write(&worktree, "[user]\n\tname = filter\n").unwrap();
    assert_eq!(io::git_status(&d.0), Ok(vec![]), "read, and safe");
    // The `config.worktree` of a linked worktree is in the repository's directory for it.
    git(&d.0, &["worktree", "add", "-q", &d.0.join("wt").display().to_string()]);
    let wt = d.0.join("wt");
    assert_eq!(io::git_prefix(&wt), Ok(String::new()));
    std::fs::write(d.0.join(".git/worktrees/wt/config.worktree"), "[core]\n\tfsmonitor = x\n")
        .unwrap();
    skipped(&wt);
    assert_eq!(io::git_prefix(&d.0), Ok(String::new()), "the main worktree has its own file");
}

#[test]
fn mark_1_the_user_s_own_config_is_trusted_and_the_repository_s_is_not() {
    use std::process::Command;
    let d = temp_dir("mark1-scopes");
    repo(&d.0, &["a.rs"]);
    let global = d.0.join("global-config");
    let defines = "[filter \"lfs\"]\n\tclean = git-lfs clean -- %f\n[core]\n\tfsmonitor = true\n\thooksPath = /h\n";
    std::fs::write(&global, format!("{defines}[include]\n\tpath = more\n")).unwrap();
    std::fs::write(d.0.join("more"), "[filter \"inc\"]\n\tsmudge = cat\n").unwrap();
    // What sim asks git, with a global config of its own.
    let list = |dir: &Path| {
        let out = Command::new("git")
            .args(["-c", "core.fsmonitor=false", "config", "--list", "--show-scope", "-z"])
            .current_dir(dir)
            .env("GIT_CONFIG_GLOBAL", &global)
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .output()
            .unwrap();
        assert!(out.status.success());
        out.stdout
    };
    let own = list(&d.0);
    let has = |l: &[u8], what: &[u8]| l.windows(what.len()).any(|w| w == what);
    assert!(has(&own, b"global\0filter.lfs.clean\ngit-lfs"), "{own:?}");
    assert!(has(&own, b"global\0filter.inc.smudge\ncat\0"), "an included file has its scope");
    assert!(has(&own, b"command\0core.fsmonitor\nfalse\0"));
    assert!(!io::runs_commands(&own));
    append(&d.0.join(".git/config"), "[filter \"mine\"]\n\tclean = cat\n");
    let mixed = list(&d.0);
    assert!(has(&mixed, b"local\0filter.mine.clean\ncat\0"));
    assert!(io::runs_commands(&mixed));
    // The same by hand, for the scopes a test cannot make: system, and what does not parse.
    assert!(!io::runs_commands(b"system\0core.fsmonitor\ntrue\0global\0include.path\nx\0"));
    assert!(io::runs_commands(b"worktree\0core.hookspath\0"));
    assert!(
        io::runs_commands(b"submodule\0filter.x.clean\ncat\0"),
        "an unknown scope is not the user's"
    );
    assert!(!io::runs_commands(b"") && !io::runs_commands(b"local\0user.name\nx\0"));
    assert!(io::runs_commands(b"local"), "a listing that stops in the middle of an entry");
}

/// The listing git prints (`scope NUL key NL value NUL`) for entries of one scope.
fn listing(scope: &str, keys: &[&str]) -> Vec<u8> {
    keys.iter().flat_map(|k| format!("{scope}\0{k}\nv\0").into_bytes()).collect()
}

#[test]
fn mark_1_every_key_that_can_hold_a_command_is_skipped_and_ordinary_keys_are_not() {
    // The names of the class, in a section git does not use them in, in a subsection, and in a
    // subsection with dots; and the ends.
    let names = [
        "command",
        "cmd",
        "program",
        "driver",
        "textconv",
        "helper",
        "process",
        "clean",
        "smudge",
        "askpass",
        "editor",
        "pager",
        "gitproxy",
        "hookspath",
        "fsmonitor",
        "sshcommand",
        "hook",
        "alternaterefscommand",
        "uploadpackcommand",
        "my-program",
        "precmd",
        "pre-commit-hook",
    ];
    for name in names {
        for key in [format!("core.{name}"), format!("x.sub.{name}"), format!("x.a.b.c.{name}")] {
            assert!(io::runs_commands(&listing("local", &[&key])), "{key}");
            assert!(io::runs_commands(&listing("worktree", &[&key])), "{key}");
            let bare = format!("local\0{key}\0");
            assert!(io::runs_commands(bare.as_bytes()), "{key}: without a value");
            for trusted in ["system", "global", "command"] {
                assert!(!io::runs_commands(&listing(trusted, &[&key])), "{trusted} {key}");
            }
        }
    }
    // Whatever the variable: the section decides.
    for key in [
        "filter.lfs.required",
        "filter.x.anything",
        "filter.y",
        "include.path",
        "includeif.gitdir:/a.b/.path",
        "includeif.hasconfig:remote.*.url:*.example/**.path",
        "core.fsmonitor",
        "core.hookspath",
    ] {
        assert!(io::runs_commands(&listing("local", &[key])), "{key}");
    }
    // What git writes and uses in every repository, and what tools keep there.
    let ordinary = [
        "core.repositoryformatversion",
        "core.filemode",
        "core.bare",
        "core.logallrefupdates",
        "core.ignorecase",
        "core.autocrlf",
        "core.symlinks",
        "core.worktree",
        "core.fsmonitor.x",
        "remote.origin.url",
        "remote.origin.fetch",
        "remote.origin.pushurl",
        "remote.filter.url",
        "branch.main.remote",
        "branch.main.merge",
        "branch.release-1.0.remote",
        "branch.a.command.merge",
        "branch.include.remote",
        "user.name",
        "user.email",
        "user.signingkey",
        "lfs.url",
        "lfs.concurrenttransfers",
        "lfs.repositoryformatversion",
        "lfs.locksverify",
        "extensions.worktreeconfig",
        "submodule.sub.url",
        "submodule.sub.active",
        "submodule.sub.update",
        "url.https://x.example/.insteadof",
        "alias.st",
        "diff.algorithm",
        "pull.rebase",
        "init.defaultbranch",
        "gc.auto",
        "tag.sort",
        "rerere.enabled",
    ];
    for key in ordinary {
        assert!(!io::runs_commands(&listing("local", &[key])), "{key}");
        assert!(!io::runs_commands(&listing("worktree", &[key])), "{key}");
    }
    assert!(!io::runs_commands(&listing("local", &ordinary)));
    let mixed = [&ordinary[..], &["x.y.smudge"]].concat();
    assert!(io::runs_commands(&listing("local", &mixed)), "one is enough, wherever it is");
}

/// A repository with `.gitattributes` that sends every file through the filter `canary`, a
/// modified `a.txt` that git must read again (the same size, an mtime that moved), and a script
/// that would be its file system monitor. Returns the script.
fn canary_repo(dir: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(dir.join(".gitattributes"), "* filter=canary\n").unwrap();
    std::fs::write(dir.join("a.txt"), "aaaa\n").unwrap();
    git(dir, &["init", "-q"]);
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", "init"]);
    std::fs::write(dir.join("a.txt"), "bbbb\n").unwrap();
    let file = std::fs::File::options().write(true).open(dir.join("a.txt")).unwrap();
    file.set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(60)).unwrap();
    let hook = dir.join(".git/monitor.sh");
    let witness = dir.join("witness-monitor");
    std::fs::write(&hook, format!("#!/bin/sh\ntouch '{}'\necho\n", witness.display())).unwrap();
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    hook
}

#[test]
fn mark_1_a_filter_or_a_monitor_of_the_repository_never_runs() {
    // Two ways to define them: by `git config`, and by hand in forms that a parser of sim's own
    // read differently from git: a monitor after a header that is the end of a continued value,
    // and a filter in a dotted section of the `config.worktree` file.
    for by_hand in [false, true] {
        let d = temp_dir(&format!("mark1-witness-{by_hand}"));
        let hook = canary_repo(&d.0);
        let filter = format!("touch '{}-filter'; cat", d.0.join("witness").display());
        let monitor = hook.display().to_string();
        if by_hand {
            let hidden = format!("[core]\n\tx = 1 \\\n[user]\n\tfsmonitor = {monitor}\n");
            append(&d.0.join(".git/config"), &hidden);
            git(&d.0, &["config", "extensions.worktreeConfig", "true"]);
            let dotted = format!("[filter.canary]\n\tclean = \"{filter}\"\n");
            std::fs::write(d.0.join(".git/config.worktree"), dotted).unwrap();
        } else {
            git(&d.0, &["config", "filter.canary.clean", &filter]);
            git(&d.0, &["config", "core.fsmonitor", &monitor]);
        }
        // The control: git as the user runs it does run both, so this test can tell.
        git(&d.0, &["status", "--porcelain"]);
        for what in ["filter", "monitor"] {
            let w = d.0.join(format!("witness-{what}"));
            assert!(
                w.exists(),
                "{by_hand} {what}: the repository's own command runs under plain git"
            );
            std::fs::remove_file(w).unwrap();
        }
        skipped(&d.0);
        assert!(!d.0.join("witness-filter").exists() && !d.0.join("witness-monitor").exists());
    }
}

#[test]
fn mark_1_a_submodule_s_own_filter_or_monitor_never_runs() {
    use std::os::unix::fs::PermissionsExt;
    let d = temp_dir("mark1-submodule");
    let (src, sup) = (d.0.join("src"), d.0.join("sup"));
    for dir in [&src, &sup] {
        std::fs::create_dir(dir).unwrap();
    }
    repo(&src, &["s.txt=aaaa\n"]);
    repo(&sup, &["f.rs"]);
    let url = src.display().to_string();
    git(&sup, &["-c", "protocol.file.allow=always", "submodule", "add", "-q", &url, "sub"]);
    git(&sup, &["commit", "-q", "-m", "submodule"]);
    // The submodule's config is its own (`.git/modules/sub/config`), not the repository's: a
    // filter for every file in it, and a monitor.
    let (own, sub) = (sup.join(".git/modules/sub"), sup.join("sub"));
    std::fs::create_dir_all(own.join("info")).unwrap();
    std::fs::write(own.join("info/attributes"), "* filter=canary\n").unwrap();
    let filter = format!("touch '{}-filter'; cat", d.0.join("witness").display());
    git(&sub, &["config", "filter.canary.clean", &filter]);
    let hook = sup.join(".git/monitor.sh");
    let witness = d.0.join("witness-monitor");
    std::fs::write(&hook, format!("#!/bin/sh\ntouch '{}'\necho\n", witness.display())).unwrap();
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    git(&sub, &["config", "core.fsmonitor", &hook.display().to_string()]);
    // A file of the submodule that git must read again: the same size, an mtime that moved.
    std::fs::write(sub.join("s.txt"), "bbbb\n").unwrap();
    let file = std::fs::File::options().write(true).open(sub.join("s.txt")).unwrap();
    file.set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(60)).unwrap();
    // The control: the user's git, asking for the repository's status, does run them.
    git(&sup, &["status", "--porcelain"]);
    for what in ["filter", "monitor"] {
        let w = d.0.join(format!("witness-{what}"));
        assert!(w.exists(), "{what}: the submodule's own command runs under plain git");
        std::fs::remove_file(w).unwrap();
    }
    // The repository's own config is safe, so git runs; none of it reaches the submodule.
    let (status, files, prefix) = (io::git_status(&sup), io::git_files(&sup), io::git_prefix(&sup));
    assert!(!d.0.join("witness-filter").exists(), "the submodule's filter ran");
    assert!(!d.0.join("witness-monitor").exists(), "the submodule's monitor ran");
    assert_eq!(status, Ok(vec![]), "a submodule is not looked at");
    assert!(String::from_utf8_lossy(&files.unwrap()).contains("sub\0"));
    assert_eq!(prefix, Ok(String::new()));
}

/// The shell side of an unsafe config in a worktree's repository or a separate git directory.
fn unsafe_config(dir: &Path) {
    append(&dir.join("config"), "[filter \"x\"]\n\tclean = cat\n");
}

#[test]
fn mark_1_the_config_of_the_worktree_s_repository_or_of_the_named_directory_counts() {
    let d = temp_dir("mark1-worktree");
    let (main, wt, sep) = (d.0.join("main"), d.0.join("wt"), d.0.join("elsewhere.git"));
    std::fs::create_dir(&main).unwrap();
    repo(&main, &["a.rs"]);
    git(&main, &["worktree", "add", "-q", &wt.display().to_string()]);
    assert_eq!(io::git_prefix(&wt), Ok(String::new()), "safe: git runs");
    unsafe_config(&main.join(".git"));
    skipped(&wt);
    skipped(&main);
    // A `.git` file that names a directory somewhere else (--separate-git-dir, a submodule's).
    let work = d.0.join("work");
    std::fs::create_dir(&work).unwrap();
    git(&work, &["init", "-q", "--separate-git-dir", &sep.display().to_string()]);
    assert!(work.join(".git").is_file());
    assert_eq!(io::git_prefix(&work), Ok(String::new()));
    unsafe_config(&sep);
    skipped(&work);
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
        // Git passes it by, and uses the one above.
        assert_eq!(io::git_prefix(&sub), Ok("sub/".into()), "{fake}: git uses the outer one");
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
    let text = std::fs::read_to_string(&config).unwrap();
    std::fs::write(&config, "[core\n\tfsmonitor = true\n").unwrap(); // git cannot read it
    skipped(&d.0);
    std::fs::write(&config, text).unwrap();
    std::fs::remove_file(&config).unwrap(); // no config at all: nothing to run
    assert!(io::git_status(&d.0).is_ok());
}
