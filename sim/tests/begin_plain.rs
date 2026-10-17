//! PLAIN-1, GUI-4: the start of every front end is `shell::begin`, and in a privileged world (root, or the
//! editor of `sudoedit`) it is plain whatever the front end: sim is plain, no program starts from then on
//! (not a filter, a hook, git or ripgrep), the shell is given no `rg`, and git's hooks directory is not
//! made. Every test of this file runs in a world that is privileged, and the first start makes the process
//! plain for good (`filter::forbid_programs` is one-way): its control, a world that is not, is `begin_open.rs`.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use sim::io;
use sim::shell::Shell;
use sim::sim::SimEvent;
use sim_core::core::app::Event;
use sim_core::io::filter;
use std::path::Path;

/// A repository with `a.txt` committed and then changed: git, if it runs, says `M`.
fn repo_with_a_change(name: &str) -> TempDir {
    let dir = temp_dir(name);
    repo(&dir.0, &["a.txt"]);
    std::fs::write(dir.0.join("a.txt"), "two\n").unwrap();
    dir
}

/// The panel's row of `a.txt` as the screen has it.
fn row_of_a(out: &sim::shell::Out) -> String {
    let s = out.frame.as_ref().unwrap();
    s.frame
        .rows
        .iter()
        .map(|r| r.text.chars().take(34).collect::<String>())
        .find(|t| t.contains("a.txt"))
        .unwrap_or_default()
}

fn type_keys(shell: &mut Shell, sim: &mut sim::sim::Sim, s: &str) -> sim::shell::Out {
    let mut last = sim::shell::Out::default();
    for k in keys(s) {
        let out = shell.send(sim, SimEvent::Term(Event::Key(k)));
        last.frame = out.frame.or(last.frame);
    }
    last
}

#[test]
fn plain_1_a_privileged_world_makes_every_front_end_plain_and_gives_it_no_rg_and_no_hooks_directory()
 {
    let dir = repo_with_a_change("bp-start");
    let on = begin_in(&dir.0, &["a.txt"], io::privileged(0, None), None);
    assert!(on.begun.sim.plain, "root's sim is plain");
    assert!(!filter::programs_allowed(), "no program starts from now on");
    assert!(
        on.begun.cfg.rg.is_none(),
        "no ripgrep, though one is on the PATH ({})",
        on.bin.display()
    );
    assert!(
        !on.state.join("no-hooks").exists(),
        "git is not run, so git's hooks have no directory"
    );
    // the sudoedit case: the parent says it, and the uid is not 0
    let sudo = begin_in(&dir.0, &["a.txt"], io::privileged(1000, Some("sudoedit\n")), None);
    assert!(sudo.begun.sim.plain && sudo.begun.cfg.rg.is_none());
    // and `--plain` in a world that is not privileged is plain too (the flag and the world are both heard)
    let flag = begin_in(&dir.0, &["--plain", "a.txt"], false, None);
    assert!(flag.begun.sim.plain && flag.begun.cfg.rg.is_none());
}

#[test]
fn plain_1_no_git_runs_and_no_filter_runs_in_a_privileged_world() {
    let dir = repo_with_a_change("bp-git");
    let witness = temp_dir("bp-witness");
    let mut on = begin_in(&dir.0, &["a.txt"], io::privileged(0, None), None);
    let cwd = dir.0.clone();
    let mut shell = Shell::inline(on.begun.cfg.clone());
    let fx = std::mem::take(&mut on.begun.fx);
    let mut sim = on.begun.sim;
    shell.run(&mut sim, fx);
    // (the interface, with the git letters, is drawn once the page's color is known)
    shell.send(&mut sim, SimEvent::Colors((0x1e, 0x1e, 0x1e), [None; 8]));
    // a filter is a program: it does not start, and its file is not made
    let at = witness.0.join("filter");
    let out = type_keys(&mut shell, &mut sim, &format!(":%!touch {}\n", at.display()));
    assert!(!Path::new(&at).exists(), "the filter ran");
    let msg =
        out.frame.unwrap().frame.rows.iter().map(|r| r.text.clone()).collect::<Vec<_>>().join("\n");
    assert!(msg.contains("plain mode runs no program"), "it says why: {msg}");
    // the tree with its git marks: git said nothing (`M`) because it was not run
    let out = type_keys(&mut shell, &mut sim, "<C-n>");
    let row = row_of_a(&out);
    assert!(row.contains("a.txt") && !row.contains('M'), "git ran: {row:?}");
    assert_eq!(std::fs::read_to_string(cwd.join("a.txt")).unwrap(), "two\n");
}

#[test]
fn plain_1_the_hook_of_a_front_end_does_not_run_in_a_privileged_world() {
    let dir = repo_with_a_change("bp-hook");
    let witness = temp_dir("bp-hook-witness");
    let at = witness.0.join("hook");
    let hook = format!("touch {}", at.display());
    let on = begin_in(&dir.0, &["a.txt"], io::privileged(0, None), Some(hook));
    assert!(on.begun.sim.plain);
    let (tx, rx) = std::sync::mpsc::channel();
    let mut shell = Shell::threads(on.begun.cfg.clone(), tx);
    let mut sim = on.begun.sim;
    shell.run(&mut sim, on.begun.fx);
    // entering Normal mode is what runs a hook: the filter's spawn refuses at once, so nothing is racing
    type_keys(&mut shell, &mut sim, "i<Esc>");
    drop(rx);
    shell.finish();
    assert!(!at.exists(), "the hook ran");
}
