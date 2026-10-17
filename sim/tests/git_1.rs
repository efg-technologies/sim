//! GIT-1, GIT-2, GIT-3, GIT-4: the one guarded way to run git, what it neutralizes, what it still
//! refuses, and submodules. A witness test: a repository whose config names a script that leaves a
//! witness file; plain git runs it (the control, so that the test can tell) and sim's call does not.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use sim::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

/// A script `name` in `dir` that appends its name and arguments to `witness`, then runs `tail`
/// (`cat` for a filter, `cat "$1"` for a text conversion).
fn script(dir: &Path, name: &str, witness: &Path, tail: &str) -> String {
    let at = dir.join(name);
    let body = format!("#!/bin/sh\necho \"{name} $*\" >> '{}'\n{tail}\n", witness.display());
    std::fs::write(&at, body).unwrap();
    std::fs::set_permissions(&at, PermissionsExt::from_mode(0o755)).unwrap();
    at.to_string_lossy().into_owned()
}

/// What the witness says ran, line by line (nothing if it was never made).
fn ran(witness: &Path) -> Vec<String> {
    let text = std::fs::read_to_string(witness).unwrap_or_default();
    text.lines().map(|l| l.trim_end().to_string()).collect()
}

fn clear(witness: &Path) {
    let _ = std::fs::remove_file(witness);
}

fn set_mtime(file: &Path, t: SystemTime) {
    let f = std::fs::OpenOptions::new().write(true).open(file).unwrap();
    f.set_modified(t).unwrap();
}

/// The same content with a newer time: git has to read the file again to compare it with the index.
fn bump(file: &Path) {
    static N: AtomicU64 = AtomicU64::new(10);
    set_mtime(file, SystemTime::now() + Duration::from_secs(N.fetch_add(7, Ordering::Relaxed)));
}

/// A repository with `a.f` (a filter's), `a.txt`, `x.dat` (a diff driver's), their attributes, and
/// the tools' directory with the witness. Everything is committed, with old times so that no entry
/// is racily clean (git would read it again to be sure, and run a filter for nothing).
struct Fx {
    repo: PathBuf,
    tools: PathBuf,
    witness: PathBuf,
    _keep: (TempDir, TempDir),
}

fn fx(name: &str) -> Fx {
    let (repo, tools) = (temp_dir(name), temp_dir(&format!("{name}-tools")));
    let (r, t) = (repo.0.clone(), tools.0.clone());
    git(&r, &["init", "-q"]);
    let files = [
        ("a.f", "one\n"),
        ("a.txt", "alpha\n"),
        ("x.dat", "raw\n"),
        (".gitattributes", "*.f filter=wit\n*.dat diff=conv\n"),
    ];
    for (f, c) in files {
        std::fs::write(r.join(f), c).unwrap();
        set_mtime(&r.join(f), SystemTime::now() - Duration::from_secs(1000));
    }
    git(&r, &["add", "-A"]);
    git(&r, &["commit", "-q", "-m", "init"]);
    Fx { witness: t.join("witness"), repo: r, tools: t, _keep: (repo, tools) }
}

fn config(dir: &Path, key: &str, value: &str) {
    git(dir, &["config", key, value]);
}

/// Runs a guarded command and returns whether it succeeded (no input, nothing printed).
fn runs_ok(dir: &Path, args: &[&str]) -> bool {
    let mut c = io::git_command(dir, args).unwrap();
    c.stdin(Stdio::null()).output().unwrap().status.success()
}

fn args_of(c: &std::process::Command) -> Vec<String> {
    c.get_args().map(|a| a.to_string_lossy().into_owned()).collect()
}

fn source_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            source_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

#[test]
fn git_1_no_other_function_builds_a_git_command() {
    let mut files = vec![];
    source_files(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut files);
    let needle = ["Command::new(", "\"git\"", ")"].concat();
    let found: Vec<String> = files
        .iter()
        .filter(|f| std::fs::read_to_string(f).unwrap().matches(&needle).count() > 0)
        .map(|f| f.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(found, ["io.rs"], "git is started in one place");
    let text = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/io.rs"));
    assert_eq!(text.unwrap().matches(&needle).count(), 1, "and once there");
}

#[test]
fn git_1_the_command_has_the_flags_of_the_guard_and_the_environment_of_mark_1() {
    let f = fx("g1-flags");
    let c = io::git_command(&f.repo, &["status", "--porcelain=v1"]).unwrap();
    assert_eq!(c.get_program(), "git", "no shell");
    let args = args_of(&c);
    let head: Vec<&str> = args.iter().take(7).map(String::as_str).collect();
    assert_eq!(
        head,
        [
            "-c",
            "core.fsmonitor=false",
            "-c",
            "diff.submodule=short",
            "-c",
            "core.hooksPath=/dev/null",
            "--no-optional-locks"
        ]
    );
    let env_literal =
        c.get_envs().any(|(k, v)| k == "GIT_LITERAL_PATHSPECS" && v.is_some_and(|v| v == "1"));
    assert!(env_literal, "names are paths");
    let at = |a: &str| args.iter().position(|x| x == a).unwrap_or(usize::MAX);
    assert!(at("status") > at("--no-optional-locks"), "{args:?}");
    assert!(args.contains(&"--ignore-submodules=dirty".to_string()), "{args:?}");
    assert!(!args.contains(&"--no-ext-diff".to_string()), "status takes no diff flags: {args:?}");
    assert_eq!(c.get_current_dir(), Some(f.repo.as_path()));
    let env: Vec<(String, String)> = c
        .get_envs()
        .filter_map(|(k, v)| Some((k.to_string_lossy().into_owned(), v?.to_string_lossy().into())))
        .collect();
    for want in [
        ("LC_ALL", "C"),
        ("GIT_NO_LAZY_FETCH", "1"),
        ("GIT_ALLOW_PROTOCOL", "sim-none"),
        ("GIT_TERMINAL_PROMPT", "0"),
    ] {
        assert!(env.contains(&(want.0.into(), want.1.into())), "{want:?} in {env:?}");
    }
    // the commands that take diff flags have them, wherever the caller's own arguments are
    for cmd in ["diff", "show", "log"] {
        let args = args_of(&io::git_command(&f.repo, &[cmd, "HEAD"]).unwrap());
        let i = args.iter().position(|a| a == cmd).unwrap();
        for flag in ["--no-ext-diff", "--no-textconv", "--ignore-submodules=all"] {
            assert!(args.iter().position(|a| a == flag).unwrap() > i, "{cmd}: {flag} in {args:?}");
        }
        assert_eq!(args.last().map(String::as_str), Some("HEAD"), "{args:?}");
    }
    let args = args_of(&io::git_command(&f.repo, &["status"]).unwrap());
    assert!(args.contains(&"--ignore-submodules=dirty".to_string()), "{args:?}");
    let args = args_of(&io::git_command(&f.repo, &["ls-files", "-z"]).unwrap());
    let own = |a: &&String| a.starts_with("--ignore-submodules") || a.starts_with("--no-ext");
    assert!(!args.iter().any(|a| own(&a) || a == "--no-textconv"), "{args:?}");
}

#[test]
fn git_1_the_hooks_directory_is_private_and_empty_and_made_once() {
    let state = temp_dir("g1-state");
    let dir = io::no_hooks_dir(&state.0).unwrap();
    assert_eq!(dir, state.0.join("no-hooks"));
    let mode = std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o700, "private");
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0, "empty");
    std::fs::write(dir.join("planted"), "x").unwrap();
    let again = io::no_hooks_dir(&state.0).unwrap();
    assert_eq!(again, dir, "the same one");
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0, "emptied again: nothing planted stays");
    // a directory that others can write in is not one to use
    std::fs::remove_dir_all(&dir).unwrap();
    std::fs::create_dir(&dir).unwrap();
    std::fs::set_permissions(&dir, PermissionsExt::from_mode(0o777)).unwrap();
    let fixed = io::no_hooks_dir(&state.0).unwrap();
    let mode = std::fs::metadata(&fixed).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o700, "made private");
}

#[test]
fn git_2_a_monitor_never_runs_and_git_answers() {
    let f = fx("g2-monitor");
    let monitor = script(&f.tools, "monitor", &f.witness, "");
    config(&f.repo, "core.fsmonitor", &monitor);
    assert!(plain(&f.repo, &["status"]).status().unwrap().success());
    assert!(!ran(&f.witness).is_empty(), "the control: plain git asks the monitor");
    clear(&f.witness);
    assert_eq!(io::git_status(&f.repo), Ok(vec![]));
    assert_eq!(io::git_files(&f.repo).map(|l| !l.is_empty()), Ok(true));
    assert!(io::git_prefix(&f.repo).is_ok());
    assert_eq!(ran(&f.witness), Vec::<String>::new(), "sim's git never asks it");
}

#[test]
fn git_2_hooks_never_run_wherever_the_repository_keeps_them() {
    for variant in ["in .git/hooks", "in a hooks path outside", "in a hooks path in the repository"]
    {
        let f = fx("g2-hooks");
        for k in ["user.name", "user.email"] {
            config(&f.repo, k, "sim");
        }
        let hooks = match variant {
            "in .git/hooks" => f.repo.join(".git/hooks"),
            "in a hooks path outside" => f.tools.join("hooks"),
            _ => f.repo.join(".husky"),
        };
        std::fs::create_dir_all(&hooks).unwrap();
        script(&hooks, "pre-commit", &f.witness, "");
        match variant {
            "in a hooks path outside" => config(&f.repo, "core.hooksPath", hooks.to_str().unwrap()),
            "in a hooks path in the repository" => config(&f.repo, "core.hooksPath", ".husky"),
            _ => {}
        }
        std::fs::write(f.repo.join("a.txt"), "alpha 2\n").unwrap();
        git(&f.repo, &["add", "a.txt"]);
        assert!(plain(&f.repo, &["commit", "-q", "-m", "plain"]).status().unwrap().success());
        assert_eq!(ran(&f.witness), ["pre-commit"], "{variant}: the control");
        clear(&f.witness);
        std::fs::write(f.repo.join("a.txt"), "alpha 3\n").unwrap();
        git(&f.repo, &["add", "a.txt"]);
        // sim never commits; the guard is a rule of the command line, so a commit built by it shows it
        assert!(runs_ok(&f.repo, &["commit", "-q", "-m", "guarded"]), "{variant}");
        assert_eq!(ran(&f.witness), Vec::<String>::new(), "{variant}: no hook ran");
        assert_eq!(git(&f.repo, &["rev-list", "--count", "HEAD"]).trim(), "3", "{variant}");
        // and the status that sim does ask for is not skipped for a hooks path
        assert!(io::git_status(&f.repo).is_ok(), "{variant}");
    }
}

#[test]
fn git_2_an_external_diff_and_a_text_conversion_never_run() {
    let f = fx("g2-diff");
    let ext = script(&f.tools, "extdiff", &f.witness, "");
    let conv = script(&f.tools, "conv", &f.witness, "cat \"$1\"");
    config(&f.repo, "diff.external", &ext);
    config(&f.repo, "diff.conv.textconv", &conv);
    std::fs::write(f.repo.join("a.txt"), "alpha\nbeta\n").unwrap();
    std::fs::write(f.repo.join("x.dat"), "raw\nmore\n").unwrap();
    // the controls: an external diff, and (with that one off) a text conversion
    assert!(plain(&f.repo, &["diff", "HEAD", "--", "a.txt"]).status().unwrap().success());
    assert!(ran(&f.witness).iter().any(|l| l.starts_with("extdiff ")), "{:?}", ran(&f.witness));
    clear(&f.witness);
    assert!(
        plain(&f.repo, &["diff", "--no-ext-diff", "HEAD", "--", "x.dat"])
            .status()
            .unwrap()
            .success()
    );
    assert!(ran(&f.witness).iter().any(|l| l.starts_with("conv ")), "{:?}", ran(&f.witness));
    clear(&f.witness);
    // sim's own diff answers with the diff itself
    let a = String::from_utf8(io::git_diff(&f.repo, "a.txt").unwrap()).unwrap();
    assert!(a.contains("+beta"), "{a}");
    let x = String::from_utf8(io::git_diff(&f.repo, "x.dat").unwrap()).unwrap();
    assert!(x.contains("+more"), "{x}");
    assert_eq!(ran(&f.witness), Vec::<String>::new());
    // an external driver named by an attribute is turned off the same way
    config(&f.repo, "diff.conv.command", &ext);
    assert!(
        plain(&f.repo, &["diff", "--no-textconv", "HEAD", "--", "x.dat"])
            .status()
            .unwrap()
            .success()
    );
    assert!(ran(&f.witness).iter().any(|l| l.starts_with("extdiff ")), "the control");
    clear(&f.witness);
    let x = String::from_utf8(io::git_diff(&f.repo, "x.dat").unwrap()).unwrap();
    assert!(x.contains("+more"), "{x}");
    assert_eq!(ran(&f.witness), Vec::<String>::new());
    // and the numbers of DIFF-2 come from a diff of the same kind
    assert!(runs_ok(&f.repo, &["diff", "--numstat", "HEAD"]));
    assert_eq!(ran(&f.witness), Vec::<String>::new());
}

#[test]
fn git_2_clean_and_process_filters_never_run_and_status_and_diff_answer() {
    for key in ["clean", "process"] {
        let f = fx(&format!("g2-{key}"));
        let tail = if key == "clean" { "cat" } else { "" };
        let filter = script(&f.tools, key, &f.witness, tail);
        config(&f.repo, &format!("filter.wit.{key}"), &filter);
        let a = f.repo.join("a.f");
        bump(&a);
        assert!(plain(&f.repo, &["status"]).status().unwrap().success());
        assert!(!ran(&f.witness).is_empty(), "{key}: the control, `status` runs it");
        clear(&f.witness);
        bump(&a);
        assert!(plain(&f.repo, &["diff", "HEAD"]).status().unwrap().success());
        assert!(!ran(&f.witness).is_empty(), "{key}: the control, `diff HEAD` runs it");
        clear(&f.witness);
        bump(&a);
        assert_eq!(io::git_status(&f.repo), Ok(vec![]), "{key}");
        assert_eq!(ran(&f.witness), Vec::<String>::new(), "{key}: status");
        bump(&a);
        assert_eq!(io::git_diff(&f.repo, "a.f"), Ok(vec![]), "{key}");
        assert_eq!(ran(&f.witness), Vec::<String>::new(), "{key}: diff");
    }
}

#[test]
fn git_2_a_smudge_filter_never_runs_and_a_required_one_does_not_stop_git() {
    let f = fx("g2-smudge");
    let smudge = script(&f.tools, "smudge", &f.witness, "cat");
    config(&f.repo, "filter.wit.smudge", &smudge);
    let a = f.repo.join("a.f");
    std::fs::write(&a, "two\n").unwrap();
    assert!(plain(&f.repo, &["checkout", "--", "a.f"]).status().unwrap().success());
    assert!(!ran(&f.witness).is_empty(), "the control: a checkout runs it");
    clear(&f.witness);
    std::fs::write(&a, "three\n").unwrap();
    assert!(runs_ok(&f.repo, &["checkout", "--", "a.f"]));
    assert_eq!(std::fs::read_to_string(&a).unwrap(), "one\n", "the file is back");
    assert_eq!(ran(&f.witness), Vec::<String>::new(), "no smudge");
    // a required filter whose command fails stops plain git; sim's status does not depend on it
    config(&f.repo, "filter.wit.required", "true");
    config(&f.repo, "filter.wit.clean", "/bin/false");
    bump(&a);
    assert!(!plain(&f.repo, &["status"]).status().unwrap().success(), "the control: it fails");
    bump(&a);
    assert_eq!(io::git_status(&f.repo), Ok(vec![]));
}

#[test]
fn git_2_a_filter_name_is_passed_as_one_argument_as_it_is() {
    let f = fx("g2-names");
    let name = "my f;#\"x\\y '$z";
    let mut text = std::fs::read_to_string(f.repo.join(".git/config")).unwrap();
    text.push_str(&format!(
        "[filter \"{}\"]\n\tclean = /bin/false\n\trequired = true\n",
        name.replace('\\', "\\\\").replace('"', "\\\"")
    ));
    std::fs::write(f.repo.join(".git/config"), text).unwrap();
    let args = args_of(&io::git_command(&f.repo, &["status"]).unwrap());
    for v in ["clean=", "smudge=", "process=", "required=false"] {
        let want = format!("filter.{name}.{v}");
        assert_eq!(args.iter().filter(|a| **a == want).count(), 1, "{want:?} in {args:?}");
    }
    assert_eq!(io::git_status(&f.repo), Ok(vec![]));
}

/// The listing git prints (`scope NUL origin NUL key NL value NUL`).
fn listing(scope: &str, entries: &[(&str, &str)]) -> Vec<u8> {
    let one = |(k, v): &(&str, &str)| format!("{scope}\0file:.git/config\0{k}\n{v}\0").into_bytes();
    entries.iter().flat_map(one).collect()
}

const LFS: [(&str, &str); 4] = [
    ("filter.lfs.required", "true"),
    ("filter.lfs.clean", "git-lfs clean -- %f"),
    ("filter.lfs.smudge", "git-lfs smudge -- %f"),
    ("filter.lfs.process", "git-lfs filter-process"),
];

fn off(name: &str) -> Vec<String> {
    let mut out = vec![];
    for v in ["clean=", "smudge=", "process=", "required=false"] {
        out.extend(["-c".to_string(), format!("filter.{name}.{v}")]);
    }
    out
}

#[test]
fn git_2_the_exact_lfs_filter_is_left_alone_and_any_other_is_neutralized() {
    assert_eq!(io::neutralizing(&listing("local", &LFS)), Vec::<String>::new());
    assert_eq!(io::neutralizing(&listing("local", &LFS[1..3])), Vec::<String>::new(), "a part");
    // what `git lfs install --skip-smudge` writes
    let mut skip = LFS;
    skip[2].1 = "git-lfs smudge --skip -- %f";
    assert_eq!(io::neutralizing(&listing("local", &skip)), off("lfs"));
    skip = LFS;
    skip[3].1 = "git-lfs filter-process --skip";
    assert_eq!(io::neutralizing(&listing("local", &skip)), off("lfs"));
    // one entry that is not git-lfs's own is enough, a program of the repository's own too
    skip = LFS;
    skip[1].1 = "./git-lfs clean -- %f";
    assert_eq!(io::neutralizing(&listing("local", &skip)), off("lfs"));
    skip = LFS;
    skip[0].1 = "false";
    assert_eq!(io::neutralizing(&listing("worktree", &skip)), off("lfs"));
    // the exception is for `lfs` alone, in the case git keeps for a name; others are neutralized
    let upper = [
        ("filter.LFS.required", "true"),
        ("filter.LFS.clean", "git-lfs clean -- %f"),
        ("filter.LFS.smudge", "git-lfs smudge -- %f"),
        ("filter.LFS.process", "git-lfs filter-process"),
    ];
    assert_eq!(io::neutralizing(&listing("local", &upper)), off("LFS"));
    let mut both = LFS.to_vec();
    both.push(("filter.foo.clean", "foo"));
    both.push(("filter.foo.clean", "foo again"));
    assert_eq!(io::neutralizing(&listing("local", &both)), off("foo"), "once for each name");
    // the user's own scopes are not the repository's: nothing is done to their filters
    assert_eq!(
        io::neutralizing(&listing("global", &[("filter.x.clean", "x")])),
        Vec::<String>::new()
    );
    assert_eq!(
        io::neutralizing(&listing("local", &[("core.bare", "false")])),
        Vec::<String>::new()
    );
    // a name with dots keeps them
    assert_eq!(io::neutralizing(&listing("local", &[("filter.a.b.c.clean", "x")])), off("a.b.c"));
}

#[test]
fn git_2_only_git_lfs_s_own_harmless_settings_keep_the_exception() {
    let with = |extra: (&'static str, &'static str)| {
        let mut all = LFS.to_vec();
        all.push(extra);
        io::neutralizing(&listing("local", &all))
    };
    // what git-lfs lets a `.lfsconfig` set (git-lfs-config(5): lfs.allowincompletepush, fetchexclude,
    // fetchinclude, gitprotocol, locksverify, pushurl, skipdownloaderrors, url, lfs.*.access and
    // remote.*.lfsurl), and what `git lfs` writes into `.git/config`: its format version, and a url's
    // access and locksverify
    for ok in [
        ("lfs.repositoryformatversion", "0"),
        ("lfs.url", "https://a.example/lfs"),
        ("lfs.pushurl", "https://a.example/lfs"),
        ("lfs.locksverify", "false"),
        ("lfs.fetchinclude", "src/"),
        ("lfs.fetchexclude", "docs/"),
        ("lfs.gitprotocol", "ssh"),
        ("lfs.allowincompletepush", "true"),
        ("lfs.skipdownloaderrors", "true"),
        ("lfs.https://a.example/lfs.access", "basic"),
        ("lfs.https://a.example/lfs.locksverify", "true"),
        ("remote.origin.lfsurl", "https://a.example/lfs"),
        ("core.bare", "false"),
    ] {
        assert_eq!(with(ok), Vec::<String>::new(), "{ok:?}");
    }
    // the others can make git-lfs write or run outside the repository: git-lfs's own warning names
    // lfs.storage, concurrenttransfers, standalonetransferagent, cachecredentials, pathfiltercachesize,
    // tustransfers, customtransfer.*.path, extension.*.clean; and a key it does not have yet
    for bad in [
        ("lfs.storage", "/elsewhere"),
        ("lfs.concurrenttransfers", "4"),
        ("lfs.standalonetransferagent", "lfs-standalone-file"),
        ("lfs.cachecredentials", "true"),
        ("lfs.pathfiltercachesize", "5"),
        ("lfs.tustransfers", "true"),
        ("lfs.customtransfer.foo.path", "/bin/true"),
        ("lfs.extension.x.clean", "true"),
        ("lfs.something-new", "x"),
        ("lfs.access", "basic"),
    ] {
        assert_eq!(with(bad), off("lfs"), "{bad:?}");
    }
    // even where only the user's own config defines the filter (`git lfs install` writes it there)
    let mut mixed = listing("global", &LFS);
    mixed.extend(listing("local", &[("lfs.storage", "/elsewhere")]));
    assert_eq!(io::neutralizing(&mixed), off("lfs"));
    mixed = listing("global", &LFS);
    mixed.extend(listing("local", &[("lfs.url", "https://a.example/lfs")]));
    assert_eq!(io::neutralizing(&mixed), Vec::<String>::new());
    // and the user's own settings are the user's
    let mut own = listing("global", &[("lfs.storage", "/mine")]);
    own.extend(listing("local", &LFS));
    assert_eq!(io::neutralizing(&own), Vec::<String>::new());
}

fn has_lfs() -> bool {
    std::process::Command::new("git")
        .args(["lfs", "version"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

#[test]
fn git_2_a_repository_with_git_lfs_files_has_the_status_plain_git_has() {
    if !has_lfs() {
        eprintln!("skipped: git-lfs is not installed (the choice itself is tested above)");
        return;
    }
    let f = fx("g2-lfs");
    git(&f.repo, &["lfs", "install", "--local"]);
    git(&f.repo, &["lfs", "track", "*.bin"]);
    // the settings git-lfs writes into `.git/config` and the ones a repository sets for it
    for (k, v) in [
        ("lfs.url", "https://lfs.example/x/info/lfs"),
        ("lfs.https://lfs.example/x/info/lfs.access", "basic"),
        ("lfs.https://lfs.example/x/info/lfs.locksverify", "false"),
        ("lfs.fetchinclude", "*.bin"),
    ] {
        config(&f.repo, k, v);
    }
    for (n, body) in [("a.bin", "x".repeat(100)), ("b.bin", "y".repeat(200))] {
        std::fs::write(f.repo.join(n), body).unwrap();
        set_mtime(&f.repo.join(n), SystemTime::now() - Duration::from_secs(1000));
    }
    git(&f.repo, &["add", "-A"]);
    git(&f.repo, &["commit", "-q", "-m", "lfs"]);
    for n in ["a.bin", "b.bin"] {
        bump(&f.repo.join(n));
    }
    // the control: plain git reads the files through the filter and finds them clean
    let out = plain(&f.repo, &["status", "--porcelain"]).stdout(Stdio::piped()).output().unwrap();
    assert!(out.stdout.is_empty(), "{}", String::from_utf8_lossy(&out.stdout));
    for n in ["a.bin", "b.bin"] {
        bump(&f.repo.join(n));
    }
    assert_eq!(io::git_status(&f.repo), Ok(vec![]), "the exception: git-lfs runs and agrees");
    assert!(io::git_files(&f.repo).unwrap().windows(5).any(|w| w == b"a.bin"));
    // one changed value (what --skip-smudge writes): neutralized, so the files look changed
    git(&f.repo, &["lfs", "install", "--local", "--skip-smudge"]);
    for n in ["a.bin", "b.bin"] {
        bump(&f.repo.join(n));
    }
    let status = String::from_utf8(io::git_status(&f.repo).unwrap()).unwrap();
    let entries: Vec<&str> = status.split('\0').filter(|e| !e.is_empty()).collect();
    assert_eq!(entries, [" M a.bin", " M b.bin"], "the cost of neutralizing");
}

#[test]
fn git_3_what_the_command_line_stops_is_not_refused_and_the_rest_of_the_class_is() {
    let refused = |entries: &[(&str, &str)]| io::runs_commands(&listing("local", entries));
    for ok in [
        ("core.fsmonitor", "/x"),
        ("core.fsmonitor", "true"),
        ("core.hookspath", ".husky"),
        ("filter.x.clean", "c"),
        ("filter.x.smudge", "c"),
        ("filter.x.process", "c"),
        ("filter.x.required", "true"),
        ("filter.a b.clean", "c"),
        ("diff.x.textconv", "c"),
        ("diff.x.command", "c"),
        ("diff.external", "c"),
    ] {
        assert_eq!(refused(&[ok]), None, "{ok:?}");
    }
    let named = |key: &str, why: &str| {
        let r = refused(&[(key, "v")]).unwrap_or_else(|| panic!("{key}: not refused"));
        assert!(r.contains(why) && r.ends_with("could run a command"), "{key}: {r}");
    };
    named("core.alternaterefscommand", "core.alternaterefscommand in .git/config");
    named("include.path", "include.path in .git/config");
    named("includeif.gitdir:/a.path", "in .git/config");
    named("merge.x.driver", "merge.x.driver in .git/config");
    named("core.sshcommand", "core.sshcommand in .git/config");
    named("core.pager", "core.pager in .git/config");
    // a filter's other keys, and a name that cannot be given to `-c`
    named("filter.x.anything", "filter.x.anything");
    named("filter.x", "filter.x");
    named("filter.a=b.clean", "clean in .git/config");
    // one refused key among neutralized ones is enough, and the neutralized ones stay no reason
    let mixed = [("filter.x.clean", "c"), ("core.fsmonitor", "m"), ("core.sshcommand", "s")];
    assert!(refused(&mixed).unwrap().contains("core.sshcommand"));
    // a name that is not UTF-8 cannot be given either
    let bad = b"local\0file:.git/config\0filter.\xff.clean\nc\0".to_vec();
    assert!(io::runs_commands(&bad).is_some());
}

#[test]
fn git_3_git_runs_in_a_repository_that_uses_all_of_them_and_is_not_run_where_a_key_remains() {
    let f = fx("g3-real");
    config(&f.repo, "core.fsmonitor", "/nonexistent/monitor");
    config(&f.repo, "filter.wit.clean", "/nonexistent/clean");
    config(&f.repo, "filter.wit.smudge", "/nonexistent/smudge");
    config(&f.repo, "filter.wit.process", "/nonexistent/process");
    config(&f.repo, "filter.wit.required", "true");
    config(&f.repo, "diff.conv.textconv", "/nonexistent/conv");
    config(&f.repo, "diff.external", "/nonexistent/ext");
    config(&f.repo, "core.hooksPath", "/nonexistent/hooks");
    bump(&f.repo.join("a.f"));
    assert_eq!(io::git_status(&f.repo), Ok(vec![]));
    assert_eq!(io::git_diff(&f.repo, "a.f"), Ok(vec![]));
    config(&f.repo, "core.alternateRefsCommand", "/nonexistent/refs");
    let why = io::git_status(&f.repo).unwrap_err();
    assert_eq!(why, "git skipped: core.alternaterefscommand in .git/config could run a command");
    assert_eq!(io::git_diff(&f.repo, "a.f").unwrap_err(), why, "every command is guarded");
    assert_eq!(io::git_command(&f.repo, &["status"]).err(), Some(why));
}

#[test]
fn git_4_a_submodule_is_one_entry_and_its_own_config_never_runs() {
    let (sub0, sup) = (temp_dir("g4-source"), temp_dir("g4-super"));
    let tools = temp_dir("g4-tools");
    let witness = tools.0.join("witness");
    git(&sub0.0, &["init", "-q"]);
    std::fs::write(sub0.0.join("a.txt"), "alpha\n").unwrap();
    git(&sub0.0, &["add", "-A"]);
    git(&sub0.0, &["commit", "-q", "-m", "source"]);
    repo(&sup.0, &["top.txt"]);
    let url = sub0.0.to_string_lossy().into_owned();
    git(&sup.0, &["-c", "protocol.file.allow=always", "submodule", "add", "-q", &url, "sub"]);
    git(&sup.0, &["commit", "-q", "-m", "a submodule"]);
    // the submodule's own config and attributes: a monitor and a filter that leave a witness
    let own = sup.0.join(".git/modules/sub/config");
    let monitor = script(&tools.0, "monitor", &witness, "");
    let filter = script(&tools.0, "filter", &witness, "cat");
    git(&sup.0, &["config", "-f", own.to_str().unwrap(), "core.fsmonitor", &monitor]);
    git(&sup.0, &["config", "-f", own.to_str().unwrap(), "filter.x.clean", &filter]);
    std::fs::write(sup.0.join("sub/.gitattributes"), "*.txt filter=x\n").unwrap();
    let a = sup.0.join("sub/a.txt");
    bump(&a);
    // the control: plain git starts a child in the submodule, under its own config
    assert!(plain(&sup.0, &["status"]).status().unwrap().success());
    assert!(!ran(&witness).is_empty(), "the control: {:?}", ran(&witness));
    clear(&witness);
    bump(&a);
    // sim's: nothing runs, and a submodule whose commit has not moved shows nothing
    assert_eq!(io::git_status(&sup.0), Ok(vec![]));
    assert_eq!(ran(&witness), Vec::<String>::new());
    // its commit moves (a commit inside it, as git would make it): one entry, ` M sub`
    git(
        &sup.0.join("sub"),
        &[
            "-c",
            "core.fsmonitor=false",
            "-c",
            "filter.x.clean=",
            "commit",
            "--allow-empty",
            "-q",
            "-m",
            "moved",
        ],
    );
    clear(&witness);
    let status = String::from_utf8(io::git_status(&sup.0).unwrap()).unwrap();
    let entries: Vec<&str> = status.split('\0').filter(|e| !e.is_empty()).collect();
    assert_eq!(entries, [" M sub"], "one entry for the submodule, and none for what is in it");
    assert_eq!(ran(&witness), Vec::<String>::new(), "a child git is not started");
    // git ls-files does not recurse: the submodule is one path
    let files = String::from_utf8(io::git_files(&sup.0).unwrap()).unwrap();
    let names: Vec<&str> = files.split('\0').filter(|n| !n.is_empty()).collect();
    assert!(names.contains(&"sub") && names.contains(&".gitmodules"), "{names:?}");
    assert!(!names.iter().any(|n| n.starts_with("sub/")), "{names:?}");
    // the numbers of the changes view see no counts for it
    let out =
        io::git_command(&sup.0, &["diff", "--numstat", "-z", "--ignore-submodules=all", "HEAD"]);
    let out = out.unwrap().stdin(Stdio::null()).output().unwrap();
    assert!(out.stdout.is_empty(), "{:?}", String::from_utf8_lossy(&out.stdout));
    assert_eq!(ran(&witness), Vec::<String>::new());
}

#[test]
fn git_2_lfs_storage_in_the_repository_s_config_writes_nothing_outside_it() {
    if !has_lfs() {
        eprintln!("skipped: git-lfs is not installed (the choice itself is tested above)");
        return;
    }
    let f = fx("g2-lfs-storage");
    let outside = temp_dir("g2-lfs-outside");
    git(&f.repo, &["lfs", "install", "--local"]);
    git(&f.repo, &["lfs", "track", "*.bin"]);
    let a = f.repo.join("a.bin");
    std::fs::write(&a, "x".repeat(100)).unwrap();
    set_mtime(&a, SystemTime::now() - Duration::from_secs(1000));
    git(&f.repo, &["add", "-A"]);
    git(&f.repo, &["commit", "-q", "-m", "lfs"]);
    // the repository points git-lfs's storage out of the repository, and the file gets new content of
    // the same size, which git must read through the filter to see
    config(&f.repo, "lfs.storage", outside.0.to_str().unwrap());
    let stored = |dir: &Path| std::fs::read_dir(dir).unwrap().count();
    std::fs::write(&a, "y".repeat(100)).unwrap();
    bump(&a);
    // the control: plain git runs git-lfs, which stores the object and its temporary files there
    assert!(plain(&f.repo, &["status", "--porcelain"]).status().unwrap().success());
    assert!(stored(&outside.0) > 0, "the control: git-lfs writes under lfs.storage");
    std::fs::remove_dir_all(&outside.0).unwrap();
    std::fs::create_dir(&outside.0).unwrap();
    std::fs::write(&a, "z".repeat(100)).unwrap();
    bump(&a);
    let status = String::from_utf8(io::git_status(&f.repo).unwrap()).unwrap();
    assert_eq!(status.split('\0').filter(|e| !e.is_empty()).collect::<Vec<_>>(), [" M a.bin"]);
    assert_eq!(stored(&outside.0), 0, "nothing is written outside the repository");
}
