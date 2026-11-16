//! DIFF-11: `<Space>gr` gives the hunk under the cursor back to HEAD as one edit that `u` undoes, nothing
//! written; never a stale hunk; `]h` and `[h` walk the hunks. Real repositories, made in private
//! temporary directories with no configuration of the user's.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use sim::color::Syntax;
use sim::layout::Skin;
use sim::sim::SimEvent;
use sim_core::core::app::Event;

const HEAD: &str = "one\ntwo\nthree\nfour\nfive\nsix\n";
/// From HEAD: `two` changed, `3b` added after `three`, `five` deleted.
const NOW: &str = "one\nTWO\nthree\n3b\nfour\nsix\n";

/// A repository with `a.txt` committed as `HEAD` and then written as `now`, open in sim.
fn opened(name: &str, now: &str) -> Rig {
    rig_env(name, &[&format!("a.txt={HEAD}"), "b.txt=new\n"], &["a.txt"], (80, 24), |e| {
        repo(&e.cwd, &[]);
        git(&e.cwd, &["rm", "-q", "--cached", "b.txt"]);
        std::fs::write(e.cwd.join("a.txt"), now).unwrap();
    })
}

fn disk(r: &Rig) -> String {
    std::fs::read_to_string(r.cwd.join("a.txt")).unwrap()
}

/// The cursor's line, 1-based.
fn line(r: &Rig) -> usize {
    let b = &r.app().buf;
    b.line_of(b.cursor) + 1
}

#[test]
fn diff_11_space_g_r_gives_a_changed_line_back_to_head_as_one_edit_that_u_undoes() {
    let mut r = opened("d11-change", NOW);
    assert!(r.sim.bufs[0].app.buf.lines() == 6);
    r.keys("j<Space>gr");
    assert_eq!(r.status(), "hunk reset: 1 line (u undoes)");
    assert_eq!(r.text(), "one\ntwo\nthree\n3b\nfour\nsix");
    assert!(r.app().buf.dirty(), "modified until it is saved");
    assert_eq!(disk(&r), NOW, "nothing is written");
    r.keys("u");
    assert_eq!(r.text(), NOW.trim_end());
    assert_eq!(line(&r), 2, "and the cursor is back");
}

#[test]
fn diff_11_an_added_line_goes_and_deleted_lines_come_back() {
    let mut r = opened("d11-add-del", NOW);
    r.keys("4G<Space>gr");
    assert_eq!(r.text(), "one\nTWO\nthree\nfour\nsix");
    assert_eq!(r.status(), "hunk reset: 1 line (u undoes)");
    // saved, the signs are the text's again; the deletion is shown on the line above it (`four`), and the
    // line comes back below it
    r.keys(":w\n4G<Space>gr");
    assert_eq!(r.text(), "one\nTWO\nthree\nfour\nfive\nsix");
    r.keys(":w\n2G<Space>gr:w\n");
    assert_eq!(disk(&r), HEAD, "saved, it is HEAD's to the byte");
    assert_eq!(git(&r.cwd, &["status", "--porcelain", "a.txt"]), "");
}

#[test]
fn diff_11_a_hunk_at_the_end_and_at_the_top_keeps_the_file_s_last_line_break() {
    let mut r = opened("d11-ends", "zero\none\ntwo\nthree\nfour\nfive\nsix\nseven\n");
    r.keys("G<Space>gr");
    assert_eq!(r.status(), "hunk reset: 1 line (u undoes)");
    r.keys(":w\ngg<Space>gr:w\n");
    assert_eq!(disk(&r), HEAD);
    // a file emptied: its lines come back whole
    let mut e = opened("d11-empty", "");
    e.keys("<Space>gr");
    assert_eq!(e.status(), "hunk reset: 6 lines (u undoes)");
    e.keys(":w\n");
    assert_eq!(disk(&e), HEAD);
    // and the last line changed and two taken away
    let mut l = opened("d11-last", "one\ntwo\nthree\nFOUR\n");
    l.keys("G<Space>gr:w\n");
    assert_eq!(disk(&l), HEAD);
}

#[test]
fn diff_11_never_a_stale_hunk_and_never_where_git_does_not_track_the_file() {
    let mut r = opened("d11-stale", NOW);
    r.keys("Gx");
    r.keys("2G<Space>gr");
    assert_eq!(r.status(), "hunk not reset: the signs are older than the text (:w, then again)");
    assert_eq!(r.text(), "one\nTWO\nthree\n3b\nfour\nix");
    // saved, the signs are asked again and are the text's: then it goes
    r.keys(":w\n");
    r.keys("2G<Space>gr");
    assert_eq!(r.text(), "one\ntwo\nthree\n3b\nfour\nix");
    // undone to the saved text, the signs are of it again
    r.keys("u");
    assert!(!r.app().buf.dirty());
    r.keys("<Space>gr");
    assert_eq!(r.status(), "hunk reset: 1 line (u undoes)");
    // a line in no hunk
    r.keys("u3G<Space>gr");
    assert_eq!(r.status(), "no hunk here");
    // a file git does not track
    r.keys(":e b.txt\n<Space>gr");
    assert!(r.status().starts_with("hunk not reset: git does not track this file"));
    assert_eq!(r.text(), "new");
}

#[test]
fn diff_11_refused_in_plain_mode_and_in_head_s_read_only_version_and_outside_a_repository() {
    let mut p =
        rig_env("d11-plain", &[&format!("a.txt={HEAD}")], &["--plain", "a.txt"], (80, 24), |e| {
            repo(&e.cwd, &[]);
            std::fs::write(e.cwd.join("a.txt"), NOW).unwrap();
        });
    p.keys("j<Space>gr");
    assert_eq!(p.status(), "plain mode runs no program: no hunks");
    assert_eq!(p.text(), NOW.trim_end());
    let mut n = rig("d11-norepo", &[&format!("a.txt={NOW}")], &["a.txt"]);
    n.keys("j<Space>gr");
    assert_eq!(n.status(), "hunk not reset: git does not track this file");
    assert_eq!(n.text(), NOW.trim_end());
    let mut h = rig_env("d11-head", &CHANGED_FILES, &[], (100, 24), |e| {
        changed_repo(&e.cwd);
        (e.skin, e.chrome, e.syn) = (Skin::Depth, true, Syntax::new(true));
    });
    h.send(SimEvent::Term(Event::Bg((0x1e, 0x1e, 0x1e))));
    h.keys(" ge");
    walk_to(&mut h, "gone.rs");
    h.keys("\n<Space>gr");
    assert!(h.open_files().iter().any(|f| f.starts_with("HEAD:")), "{:?}", h.open_files());
    assert_eq!(h.status(), "read-only");
}

#[test]
fn diff_11_bracket_h_walks_the_hunks_against_head_and_wraps_round() {
    let mut r = opened("d11-walk", NOW);
    r.keys("]h");
    assert_eq!(line(&r), 2);
    r.keys("]h");
    assert_eq!(line(&r), 4);
    r.keys("]h");
    assert_eq!(line(&r), 5, "the deletion, on the line above it");
    r.keys("]h");
    assert_eq!(line(&r), 2, "round");
    r.keys("[h");
    assert_eq!(line(&r), 5);
    assert_eq!(r.text(), NOW.trim_end(), "nothing is edited");
    // `]c` is still the disk's
    r.keys("]c");
    assert_eq!(line(&r), 5);
    let mut n = rig("d11-none", &["a.txt=x\n"], &["a.txt"]);
    n.keys("]h");
    assert_eq!(n.status(), "no hunks against HEAD");
}
