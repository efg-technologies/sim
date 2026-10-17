//! PLAIN-1, GUI-4
//! The window is a front end like the terminal: in a privileged world (root, or the editor of
//! `sudoedit`) it is plain, no program starts (not git, a filter or ripgrep), because its start is
//! `begin`, which applies the rules once for every front end. Every test of this file is in a privileged
//! world, and the first start makes the process plain for good: its control, a world that is not, is
//! `begin_open.rs`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // helpers; the tests themselves may unwrap
use sim_gui::api::*;
use sim_gui::demo;
use sim_gui::input::{notation, to_sim};
use sim_gui::safe_io::PrivateDir;
use sim_gui::session::{Exec, Session, Settings};
use sim_gui::theme::DARK;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// A world for a start: privileged or not, a private state directory, and a `PATH` of one directory
/// that has an `rg`.
fn world(privileged: bool, state: &Path, bin: &Path) -> World {
    let rg = bin.join("rg");
    std::fs::write(&rg, "#!/bin/sh\nexit 0\n").unwrap();
    std::fs::set_permissions(&rg, std::fs::Permissions::from_mode(0o755)).unwrap();
    World {
        privileged,
        path: Some(bin.into()),
        state: Some(state.into()),
        history: None,
        moved: None,
    }
}

/// The demo project in a private directory, with `README.md` changed from HEAD (git says `M`).
fn project(home: &PrivateDir) -> PathBuf {
    let dir = home.path().join("lantern");
    demo::create(&dir).unwrap();
    std::fs::canonicalize(&dir).unwrap()
}

fn typed(session: &mut Session, keys: &str) {
    for ev in notation(keys) {
        for e in to_sim(ev) {
            session.send(e);
        }
    }
}

fn start(dir: &Path, world: World) -> Session {
    let set = Settings { world, home: None, icons: false, size: (100, 30) };
    let args = vec!["README.md".to_string()];
    Session::new(&args, dir, set, &DARK, Exec::Inline).unwrap().0
}

/// The panel's row of `README.md`, cut where the panel ends.
fn row_of_readme(s: &Session) -> String {
    let rows = &s.screen().unwrap().frame.rows;
    let cut = |text: &str| text.chars().take(34).collect::<String>();
    rows.iter().map(|r| cut(&r.text)).find(|t| t.contains("README.md")).unwrap_or_default()
}

#[test]
fn plain_1_a_window_in_a_privileged_world_is_plain_and_runs_no_program() {
    let (home, state, bin) = (
        PrivateDir::new("sim-gui-p1-").unwrap(),
        PrivateDir::new("sim-gui-p1-state-").unwrap(),
        PrivateDir::new("sim-gui-p1-bin-").unwrap(),
    );
    let dir = project(&home);
    let witness = dir.parent().unwrap().join("witness");
    // the privileged seam: the effective uid is 0 (and the other way in, a sudoedit parent)
    assert!(sio::privileged(0, None) && sio::privileged(1000, Some("sudoedit\n")));
    let mut session = start(&dir, world(sio::privileged(0, None), state.path(), bin.path()));
    assert!(session.sim.plain, "the window's sim is plain as the terminal's is");
    assert!(!filter::programs_allowed(), "no program starts from now on");
    // git did not run: the tree has no letter for the changed file
    typed(&mut session, "<C-n>");
    let row = row_of_readme(&session);
    assert!(
        row.contains("README.md") && !row.trim_end_matches('│').trim_end().ends_with(" M"),
        "git ran: {row:?}"
    );
    assert!(
        !state.path().join("no-hooks").exists(),
        "git's hooks have no directory: git is not run"
    );
    // a filter does not start, and says why
    typed(&mut session, "<Esc>");
    typed(&mut session, &format!(":%!touch {}\n", witness.display()));
    assert!(!witness.exists(), "the filter ran");
    let text: String =
        session.screen().unwrap().frame.rows.iter().map(|r| r.text.as_str()).collect();
    assert!(text.contains("plain mode runs no program"), "{text}");
    session.finish();
}

#[test]
fn plain_1_the_shell_of_a_window_in_a_privileged_world_is_given_no_rg_and_no_hooks_directory() {
    let (home, state, bin) = (
        PrivateDir::new("sim-gui-p1b-").unwrap(),
        PrivateDir::new("sim-gui-p1b-state-").unwrap(),
        PrivateDir::new("sim-gui-p1b-bin-").unwrap(),
    );
    let dir = project(&home);
    let front = Front {
        size: (100, 30),
        syn: Syntax::new(true),
        skin: Some(Skin::Depth),
        icons: Some(false),
        tab_uses: Some(3),
        hook: None,
        home: None,
    };
    let w = world(sio::privileged(0, None), state.path(), bin.path());
    let args = vec!["README.md".to_string()];
    let begun = begin(arguments(&args, &dir).unwrap(), &dir, w, front);
    assert!(begun.sim.plain);
    assert_eq!(begun.cfg.rg, None, "an rg is on the PATH, and is not used");
    assert_eq!(begun.cfg.state.as_deref(), Some(state.path()), "the state is the world's");
}
