//! PLAIN-1, GUI-4
//! The control of `plain_1.rs`: the window's start in a world that is not privileged runs git and a
//! filter, finds an `rg` and makes git's hooks directory, so that what the other file finds not done is
//! not done for another reason.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // helpers; the tests themselves may unwrap
use sim_gui::api::*;
use sim_gui::demo;
use sim_gui::input::{notation, to_sim};
use sim_gui::safe_io::PrivateDir;
use sim_gui::session::{Exec, Session, Settings};
use sim_gui::theme::DARK;
use std::os::unix::fs::PermissionsExt;

#[test]
fn plain_1_a_window_in_a_world_that_is_not_privileged_runs_git_a_filter_and_finds_an_rg() {
    let (home, state, bin) = (
        PrivateDir::new("sim-gui-bo-").unwrap(),
        PrivateDir::new("sim-gui-bo-state-").unwrap(),
        PrivateDir::new("sim-gui-bo-bin-").unwrap(),
    );
    let rg = bin.path().join("rg");
    std::fs::write(&rg, "#!/bin/sh\nexit 0\n").unwrap();
    std::fs::set_permissions(&rg, std::fs::Permissions::from_mode(0o755)).unwrap();
    let dir = home.path().join("lantern");
    demo::create(&dir).unwrap();
    let dir = std::fs::canonicalize(&dir).unwrap();
    let witness = home.path().join("witness");
    assert!(!sio::privileged(1000, Some("zsh\n")));
    let world = World {
        privileged: sio::privileged(1000, Some("zsh\n")),
        path: Some(bin.path().into()),
        state: Some(state.path().into()),
        history: None,
        moved: None,
    };
    let set = Settings { world, home: None, icons: false, size: (100, 30) };
    let args = vec!["README.md".to_string()];
    let mut session = Session::new(&args, &dir, set, &DARK, Exec::Inline).unwrap().0;
    assert!(!session.sim.plain && filter::programs_allowed());
    assert!(
        state.path().join("no-hooks").is_dir(),
        "git's hooks are pointed at a directory of ours"
    );
    for keys in [":%!touch ".to_string() + &witness.display().to_string() + "\n", "<C-n>".into()] {
        for ev in notation(&keys) {
            for e in to_sim(ev) {
                session.send(e);
            }
        }
    }
    assert!(witness.exists(), "the filter ran");
    let rows = &session.screen().unwrap().frame.rows;
    let row = rows
        .iter()
        .map(|r| r.text.chars().take(34).collect::<String>())
        .find(|t| t.contains("README.md"))
        .unwrap();
    assert!(row.trim_end_matches('│').trim_end().ends_with(" M"), "git ran and said so: {row:?}");
    session.finish();
}
