//! PLAIN-1: `--plain` (and root, and sudoedit's editor): calm, the panel closed, and no program: no
//! git (no status, no diff, no list from `git ls-files`), no ripgrep, no filter, no hook; the
//! status line says `plain`; the options and the defaults are told apart as the core reads them.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use sim::io;
use sim::layout::Skin;
use sim::sim::{SimEffect, SimEvent, start};
use std::path::Path;

fn args(a: &[&str]) -> Vec<String> {
    a.iter().map(ToString::to_string).collect()
}

fn go(a: &[&str]) -> Option<sim::sim::Start> {
    start(&args(a), Path::new("/w"), |_| false, |_| false)
}

#[test]
fn plain_1_the_flag_is_an_option_before_the_arguments_and_after_two_dashes_a_file_name() {
    let s = go(&["--plain", "f.rs"]).unwrap();
    assert_eq!((s.plain, s.file.as_deref()), (true, Some("f.rs")));
    let s = go(&["f.rs", "--plain", "+q"]).unwrap();
    assert_eq!((s.plain, s.file.as_deref(), s.keys.as_str()), (true, Some("f.rs"), ":q\n"));
    let s = go(&["--", "--plain"]).unwrap();
    assert_eq!((s.plain, s.file.as_deref()), (false, Some("--plain")));
    assert!(!go(&["f.rs"]).unwrap().plain);
    assert!(go(&["--plan", "f.rs"]).is_none(), "any other option is a usage error");
    assert!(go(&["--plain", "a", "b"]).is_none(), "and so is a second file");
}

#[test]
fn plain_1_it_starts_calm_with_the_panel_closed_and_asks_for_no_git() {
    let mut r =
        rig_env("p1-start", &["a.txt=one\n", "b.txt"], &["--plain", "a.txt"], (80, 24), |e| {
            e.skin = Skin::Depth; // what was remembered: plain ignores it
            e.hook = Some("fcitx5-remote -c".into());
        });
    assert!(r.sim.plain && !r.sim.panel.open);
    assert_eq!(r.sim.skin, Skin::Calm);
    assert_eq!(r.open_files(), ["a.txt"]);
    assert_eq!(r.status(), "a.txt  1:1  NORMAL  plain");
    r.keys("x<Esc>");
    for job in ["status", "diff", "history"] {
        assert!(!r.jobs.contains(&job), "plain mode asked for {job}: {:?}", r.jobs);
    }
    assert!(
        !r.log.iter().any(|e| matches!(e, SimEffect::App(_, sim_core::core::app::Effect::Hook(_))))
    );
}

#[test]
fn plain_1_without_a_file_it_is_the_unnamed_buffer_with_the_intro() {
    let r = rig("p1-unnamed", &["a.txt"], &["--plain"]);
    assert!(!r.sim.panel.open);
    assert_eq!((r.open_files(), r.text()), (vec![String::new()], String::new()));
    assert!(r.app().path.is_none());
    let shown: String =
        r.screen.rows.iter().map(|x| x.text.as_str()).collect::<Vec<_>>().join("\n");
    assert!(shown.contains(&format!("sim {}", env!("CARGO_PKG_VERSION"))), "{shown}");
    assert_eq!(r.status(), "[no name]  1:1  NORMAL  plain");
}

#[test]
fn plain_1_root_and_sudoedit_are_plain_without_the_flag() {
    let r = rig_env("p1-default", &["a.txt=x\n"], &["a.txt"], (80, 24), |e| e.plain = true);
    assert!(r.sim.plain && r.sim.skin == Skin::Calm && r.status().ends_with("plain"));
    let root = "Name:\tsim\nUid:\t0\t0\t0\t0\nGid:\t0\t0\t0\t0\n";
    let user = "Name:\tsim\nUid:\t1000\t1000\t1000\t1000\n";
    assert!(io::privileged(root, None), "root");
    assert!(
        io::privileged("Uid:\t1000\t0\t0\t0\n", None),
        "the effective uid counts, not the real one"
    );
    assert!(!io::privileged(user, Some("zsh\n")), "a user's own shell");
    for parent in ["sudo\n", "sudoedit\n"] {
        assert!(io::privileged(user, Some(parent)), "the editor that sudoedit runs: {parent:?}");
    }
    assert!(!io::privileged(user, None) && !io::privileged("", None));
}

#[test]
fn plain_1_a_filter_is_refused_naming_plain_mode_and_the_looks_stay_calm() {
    let mut r = rig("p1-keys", &["a.txt=b\na\n"], &["--plain", "a.txt"]);
    r.keys(":%!sort\n");
    assert!(r.status().contains("plain mode"), "{}", r.status());
    assert_eq!(r.text(), "b\na");
    r.keys("zl");
    assert_eq!((r.sim.skin, r.status()), (Skin::Calm, "plain mode: the look is calm"));
    assert!(!r.jobs.contains(&"look"), "and nothing is remembered");
    // A normal start runs the same filter.
    let mut n = rig("p1-normal", &["a.txt=b\na\n"], &["a.txt"]);
    n.keys(":%!sort\n");
    assert!(!n.status().contains("plain"), "{}", n.status());
}

#[test]
fn plain_1_ctrl_n_still_shows_the_tree_and_it_asks_for_no_git() {
    let mut r = rig("p1-tree", &["a.txt=x\n", "src/b.rs"], &["--plain", "a.txt"]);
    r.keys("<C-n>");
    assert!(r.sim.panel.open);
    assert_eq!(r.panel(), ["▸ src/", "• a.txt"], "(the file that was opened is the likely one)");
    assert!(!r.jobs.contains(&"status"), "{:?}", r.jobs);
    r.send(SimEvent::Tick(1000));
}

#[test]
fn vi_11_the_hook_is_sim_on_normal_and_si_on_normal_is_still_read_as_the_old_name() {
    let s = |v: &str| Some(v.to_string());
    assert_eq!(io::hook(s("a"), s("b")), s("a"), "the new name wins");
    assert_eq!(io::hook(None, s("b")), s("b"), "the old one is read when the new is not set");
    assert_eq!(io::hook(s(""), s("b")), s("b"), "an empty one counts as unset");
    assert_eq!(io::hook(s(""), s("")), None);
    assert_eq!(io::hook(None, None), None);
}
