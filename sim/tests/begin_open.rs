//! PLAIN-1, GUI-4: the control of `begin_plain.rs`: the same start in a world that is not privileged runs
//! git, a filter and the hook, finds an `rg` and makes git's hooks directory, so that what the other
//! file finds not done is not done for another reason.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use sim::io;
use sim::shell::Shell;
use sim::sim::SimEvent;
use sim_core::core::app::Event;
use sim_core::io::filter;
use std::time::{Duration, Instant};

fn type_keys(shell: &mut Shell, sim: &mut sim::sim::Sim, s: &str) -> sim::shell::Out {
    let mut last = sim::shell::Out::default();
    for k in keys(s) {
        let out = shell.send(sim, SimEvent::Term(Event::Key(k)));
        last.frame = out.frame.or(last.frame);
    }
    last
}

fn repo_with_a_change(name: &str) -> TempDir {
    let dir = temp_dir(name);
    repo(&dir.0, &["a.txt"]);
    std::fs::write(dir.0.join("a.txt"), "two\n").unwrap();
    dir
}

#[test]
fn plain_1_a_world_that_is_not_privileged_gets_a_sim_that_runs_programs() {
    let dir = repo_with_a_change("bo-start");
    let on = begin_in(&dir.0, &["a.txt"], io::privileged(1000, Some("zsh\n")), None);
    assert!(!on.begun.sim.plain && filter::programs_allowed());
    assert_eq!(on.begun.cfg.rg, Some(on.bin.join("rg")), "the rg on the PATH");
    assert!(on.state.join("no-hooks").is_dir(), "git's hooks are pointed at a directory of ours");
}

#[test]
fn plain_1_git_a_filter_and_the_hook_run_where_the_world_is_not_privileged() {
    let dir = repo_with_a_change("bo-git");
    let witness = temp_dir("bo-witness");
    let (filter_at, hook_at) = (witness.0.join("filter"), witness.0.join("hook"));
    let hook = format!("touch {}", hook_at.display());
    let mut on = begin_in(&dir.0, &["a.txt"], false, Some(hook.clone()));
    let mut shell = Shell::inline(on.begun.cfg.clone());
    let fx = std::mem::take(&mut on.begun.fx);
    let mut sim = on.begun.sim;
    shell.run(&mut sim, fx);
    // (the interface, with the git letters, is drawn once the page's color is known)
    shell.send(&mut sim, SimEvent::Colors((0x1e, 0x1e, 0x1e), [None; 8]));
    type_keys(&mut shell, &mut sim, &format!(":%!touch {}\n", filter_at.display()));
    assert!(filter_at.exists(), "the filter ran");
    let out = type_keys(&mut shell, &mut sim, "<C-n>");
    let s = out.frame.unwrap();
    let row: String = s
        .frame
        .rows
        .iter()
        .map(|r| r.text.chars().take(34).collect::<String>())
        .find(|t| t.contains("a.txt"))
        .unwrap();
    assert!(row.contains('M'), "git ran and said so: {row:?}");
    // and the hook, on a thread: entering Normal mode runs it
    let on = begin_in(&dir.0, &["a.txt"], false, Some(hook));
    let (tx, _rx) = std::sync::mpsc::channel();
    let mut shell = Shell::threads(on.begun.cfg.clone(), tx);
    let mut sim = on.begun.sim;
    shell.run(&mut sim, on.begun.fx);
    type_keys(&mut shell, &mut sim, "i<Esc>");
    let end = Instant::now() + Duration::from_secs(10);
    while !hook_at.exists() && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(20));
    }
    shell.finish();
    assert!(hook_at.exists(), "the hook did not run in 10 s");
}
