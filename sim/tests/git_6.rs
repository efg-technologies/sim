//! GIT-6: the guard follows the repository while sim runs. A config written from outside is judged again
//! when the watcher tells of it, and the frame says what is so; a hook written into the hooks directory is
//! said, in the alert's color and with its name, on the status line and on the frame, and `<Space>gi` says it
//! again. Nothing that the config or a hook names is run. Driven through sim's state, and the watcher on a
//! real repository with its own thread.
#![allow(clippy::unwrap_used, clippy::panic)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use sim::color::Syntax;
use sim::io;
use sim::layout::Skin;
use sim::paint::Role;
use sim::sim::{SimEvent, Touched};
use sim_core::core::app::Event;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// A repository with `a.txt` changed from HEAD, the panel open in depth, with colors.
fn project(name: &str) -> Rig {
    let mut r = rig_env(name, &["a.txt=a\n", "b.txt=b\n"], &[], (100, 20), |e| {
        repo(&e.cwd, &[]);
        fs::write(e.cwd.join("a.txt"), "changed\n").unwrap();
        e.skin = Skin::Depth;
        e.chrome = true;
        e.syn = Syntax::new(true);
    });
    r.send(SimEvent::Term(Event::Bg((0x1e, 0x1e, 0x1e))));
    r
}

/// Whether `a.txt`'s row of the tree has git's letter `M`.
fn marked(r: &Rig) -> bool {
    let (pw, _) = r.sim.widths();
    let row = |l: &&sim_core::core::view::Row| l.text.contains("a.txt");
    let row = r
        .screen
        .rows
        .iter()
        .find(row)
        .map(|l| l.text.chars().take(usize::from(pw) + 2).collect::<String>());
    row.is_some_and(|l| l.trim_end_matches(['│', ' ']).ends_with('M'))
}

/// A script that leaves `witness` if it runs.
fn witness_script(at: &Path, witness: &Path) {
    fs::write(at, format!("#!/bin/sh\ntouch '{}'\n", witness.display())).unwrap();
    fs::set_permissions(at, fs::Permissions::from_mode(0o755)).unwrap();
}

fn git_moved() -> SimEvent {
    SimEvent::Touched(Touched { git: true, ..Touched::default() })
}

fn hooks(names: &[(&str, bool)]) -> SimEvent {
    let hooks = names.iter().map(|(n, x)| (n.to_string(), *x)).collect();
    SimEvent::Touched(Touched { hooks, ..Touched::default() })
}

#[test]
fn git_6_a_config_written_from_outside_is_judged_again_and_the_frame_follows_it() {
    let mut r = project("g6-config");
    let w = temp_dir("g6-config-witness");
    let witness = w.0.join("ran");
    assert!(marked(&r), "{:?}", r.screen.rows);
    assert!(!r.screen.rows[0].text.contains("git not run"));
    // a key that could run a command: git is not run, the frame says so, and the marks go
    git(&r.cwd, &["config", "core.sshCommand", &format!("touch '{}'", witness.display())]);
    r.send(git_moved());
    assert!(r.screen.rows[0].text.contains(" git not run "), "{}", r.screen.rows[0].text);
    assert!(r.status().starts_with("git skipped: core.sshcommand"), "{}", r.status());
    assert!(!marked(&r), "{:?}", r.screen.rows);
    // taken out again: git runs, the frame says nothing, and the marks are back
    git(&r.cwd, &["config", "--unset", "core.sshCommand"]);
    r.send(git_moved());
    assert!(!r.screen.rows[0].text.contains("git not run"), "{}", r.screen.rows[0].text);
    assert!(marked(&r), "{:?}", r.screen.rows);
    // a setting that is turned off: counted on the frame as it comes and goes
    git(&r.cwd, &["config", "core.fsmonitor", &witness.display().to_string()]);
    r.send(git_moved());
    assert!(r.screen.rows[0].text.contains("1 git setting off"), "{}", r.screen.rows[0].text);
    git(&r.cwd, &["config", "--unset", "core.fsmonitor"]);
    r.send(git_moved());
    assert!(!r.screen.rows[0].text.contains("off "), "{}", r.screen.rows[0].text);
    assert!(!witness.exists(), "nothing the config named was run");
}

#[test]
fn git_6_a_hook_written_is_said_in_the_alert_s_color_with_its_name_and_on_the_frame() {
    let mut r = project("g6-hook");
    let said = "hook written: hooks/pre-commit: sim does not run it, your git will";
    r.send(hooks(&[("pre-commit", true)]));
    assert_eq!(r.status().trim_end(), said);
    let t = sim::color::Tones::new(&sim_core::core::color::palette((0x1e, 0x1e, 0x1e)));
    let last = r.shot.paints.last().unwrap();
    assert!(
        last.iter().any(|(_, p)| p.fg == Some(t.alert_fg()) && p.role == Role::Alert),
        "{last:?}"
    );
    assert!(r.screen.rows[0].text.contains(" hook written "), "{}", r.screen.rows[0].text);
    // a key ends the message, the frame keeps saying it, and `<Space>gi` says it again
    r.keys("j");
    assert!(!r.status().contains("hook written"), "{}", r.status());
    assert!(r.screen.rows[0].text.contains(" hook written "), "{}", r.screen.rows[0].text);
    r.keys(" gi");
    assert!(r.status().starts_with(said), "{}", r.status());
    // two at once, and the guard's words share the frame with the hook's
    r.send(hooks(&[("pre-push", true), ("post-checkout", true)]));
    assert!(
        r.status().starts_with(
            "hooks written: hooks/pre-push, hooks/post-checkout: sim does not run them"
        ),
        "{}",
        r.status()
    );
    git(&r.cwd, &["config", "core.fsmonitor", "/bin/false"]);
    r.send(git_moved());
    assert!(r.screen.rows[0].text.contains(" 1 off, hook written "), "{}", r.screen.rows[0].text);
    // gone (or no longer executable): not said any more
    r.send(hooks(&[("pre-commit", false), ("pre-push", false), ("post-checkout", false)]));
    assert!(!r.screen.rows[0].text.contains("hook"), "{}", r.screen.rows[0].text);
    r.keys(" gi");
    assert!(!r.status().contains("hook"), "{}", r.status());
}

#[test]
fn git_6_a_narrow_panel_gives_the_guard_s_words_up_before_the_hook_s() {
    let mut r = project("g6-narrow");
    git(&r.cwd, &["config", "core.fsmonitor", "/bin/false"]);
    git(&r.cwd, &["config", "diff.external", "/bin/false"]);
    r.send(git_moved());
    r.send(hooks(&[("pre-commit", true)]));
    r.send(SimEvent::Term(Event::Resize(80, 20)));
    let top = r.screen.rows[0].text.clone();
    assert!(top.contains(" git: 2 off, hook ") || top.contains(" hook written "), "{top}");
    r.send(SimEvent::Term(Event::Resize(60, 20)));
    let top = r.screen.rows[0].text.clone();
    assert!(top.contains(" hook"), "{top}");
    // calm has no frame: the status line said it
    r.keys("zlzl"); // depth, calm
    assert!(!r.screen.rows.iter().take(2).any(|l| l.text.contains(" hook ")));
}

#[test]
fn git_6_nothing_a_hook_or_the_config_names_runs_when_sim_asks_git() {
    let r = project("g6-witness");
    let w = temp_dir("g6-witness-ran");
    let witness = w.0.join("ran");
    let hooks = r.cwd.join(".git/hooks");
    fs::create_dir_all(&hooks).unwrap();
    for h in ["pre-commit", "post-index-change", "post-checkout", "reference-transaction"] {
        witness_script(&hooks.join(h), &witness);
    }
    git(&r.cwd, &["config", "core.fsmonitor", &witness.display().to_string()]);
    let root = &r.sim.root;
    io::status(root).unwrap();
    io::changes(root, &[]).unwrap();
    let req = sim::sim::PatchReq { n: 1, path: "a.txt".into(), old: None, new: false };
    io::patch(root, &req);
    assert!(!witness.exists());
    // the control: the user's git runs a hook
    assert!(plain(root, &["commit", "-q", "--allow-empty", "-m", "x"]).status().unwrap().success());
    assert!(witness.exists(), "the control: git commit runs pre-commit");
}

#[test]
fn git_6_the_watcher_names_a_hook_that_can_run_and_not_a_sample_or_a_name_git_never_runs() {
    let d = temp_dir("g6-watch");
    make(&d.0, &["a.txt"]);
    repo(&d.0, &[]);
    let hooks = fs::canonicalize(d.0.join(".git/hooks")).unwrap();
    let (tx, rx) = mpsc::channel();
    let p = io::project(tx, fs::canonicalize(&d.0).unwrap(), None, io::POLL);
    p.files(&io::files(&d.0));
    std::thread::sleep(Duration::from_millis(300));
    while rx.try_recv().is_ok() {}
    let said = |wait: u64| -> Vec<Touched> {
        let until = Instant::now() + Duration::from_millis(wait);
        let mut out = vec![];
        while let Some(left) = until.checked_duration_since(Instant::now()) {
            if let Ok(SimEvent::Touched(t)) = rx.recv_timeout(left) {
                out.push(t);
            }
        }
        out
    };
    let named =
        |b: &[Touched]| -> Vec<(String, bool)> { b.iter().flat_map(|t| t.hooks.clone()).collect() };
    fs::write(hooks.join("pre-commit"), "#!/bin/sh\n").unwrap();
    let b = said(400);
    assert_eq!(named(&b), [("pre-commit".to_string(), false)], "a file that cannot run");
    assert!(b.iter().all(|t| !t.git && t.paths.is_empty()), "no news for git's status: {b:?}");
    fs::set_permissions(hooks.join("pre-commit"), fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(named(&said(400)), [("pre-commit".to_string(), true)]);
    // a sample and a name with a dot are never run by git
    witness_script(&hooks.join("pre-push.sample"), &d.0.join("x"));
    witness_script(&hooks.join("pre-push.bak"), &d.0.join("x"));
    assert_eq!(named(&said(400)), Vec::<(String, bool)>::new());
    fs::remove_file(hooks.join("pre-commit")).unwrap();
    assert_eq!(named(&said(400)), [("pre-commit".to_string(), false)]);
    // the config is git's news
    git(&d.0, &["config", "core.fsmonitor", "/bin/false"]);
    assert!(said(400).iter().any(|t| t.git));
    std::mem::forget(p);
}
