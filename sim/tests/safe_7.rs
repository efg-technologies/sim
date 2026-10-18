//! SAFE-7 on sim's screen: once a save that overrode the disk (`:w!`) or took it (`:e!`) has
//! settled the conflict, the screen stops saying there is one: the badges in the border and the
//! status line, the marks and tints on the lines, and the mark on the tab. Where the other version
//! went stays on the status line until the next key.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use sim::color::Syntax;
use sim::layout::Skin;
use sim::sim::SimEvent;
use sim_core::core::app::Event;

const SRC: &str = "one\ntwo\nthree\nfour\nfive\nsix\n";
const THEIRS: &str = "one\nTWO\nthree\nfour\nFIVE\nsix\n";

/// `a.rs` open with two of its lines edited, then changed on the disk in the same two: a conflict.
fn conflicted(name: &str, skin: Skin) -> Rig {
    conflicted_at(name, skin, (100, 24), None)
}

/// The same on a screen of `size`, with `home` as the home directory (UI-2).
fn conflicted_at(
    name: &str,
    skin: Skin,
    size: (u16, u16),
    home: Option<std::path::PathBuf>,
) -> Rig {
    let mut r = rig_env(name, &[&format!("a.rs={SRC}")], &["a.rs"], size, |e| {
        e.skin = skin;
        e.chrome = true;
        e.syn = Syntax::new(true);
        e.home = home;
    });
    r.send(SimEvent::Term(Event::Bg((0x1e, 0x1e, 0x1e))));
    r.keys("jx3jx");
    let id = r.sim.bufs[0].id;
    std::fs::write(r.cwd.join("a.rs"), THEIRS).unwrap(); // (what the watcher read)
    r.send(SimEvent::App(id, disk(THEIRS.as_bytes())));
    assert!(r.app().diverged(), "a conflict");
    r
}

/// The screen's words: every row of it.
fn screen(r: &Rig) -> String {
    r.shot.frame.rows.iter().map(|x| x.text.clone() + "\n").collect()
}

/// The tab bar's row and the border's: what they say about the disk.
fn says_disk(r: &Rig) -> bool {
    let text = screen(r);
    text.contains("on disk") || text.contains("conflict") || text.contains('↯')
}

fn tinted(r: &Rig) -> bool {
    r.shot.paints.iter().flatten().any(|(_, p)| {
        p.bg.is_some_and(|b| {
            b == sim::color::Tones::new(&sim_core::core::color::palette((0x1e, 0x1e, 0x1e))).gain
        })
    })
}

#[test]
fn safe_7_after_w_bang_the_conflict_and_the_changes_on_disk_are_gone_from_the_screen() {
    for skin in [Skin::Flat, Skin::Depth] {
        let mut r = conflicted("safe7-wbang", skin);
        assert!(says_disk(&r), "{skin:?}: the conflict is on the screen:\n{}", screen(&r));
        assert!(tinted(&r), "{skin:?}: and the lines are tinted");
        r.keys(":w!\n");
        assert!(!r.app().diverged());
        assert!(r.app().buf.marks.is_empty(), "{skin:?}: no marks on the lines");
        assert!(!says_disk(&r), "{skin:?}: the screen says nothing of the disk:\n{}", screen(&r));
        assert!(!tinted(&r), "{skin:?}: no tint");
        let status = r.status().to_string();
        assert!(status.contains("saved a.rs; the disk's version is in "), "{skin:?}: {status:?}");
        // the status line keeps where the other version is until the next key
        r.keys("j");
        assert!(!r.status().contains("the disk's version"), "{:?}", r.status());
        assert!(!says_disk(&r), "{skin:?}: still nothing");
    }
}

#[test]
fn safe_7_after_e_bang_the_conflict_and_the_changes_on_disk_are_gone_from_the_screen() {
    for skin in [Skin::Flat, Skin::Depth] {
        let mut r = conflicted("safe7-ebang", skin);
        r.keys(":e!\n");
        assert!(!r.app().diverged());
        assert!(r.app().buf.marks.is_empty(), "{skin:?}: no marks on the lines");
        assert!(!says_disk(&r), "{skin:?}:\n{}", screen(&r));
        assert!(!tinted(&r), "{skin:?}: no tint");
        assert_eq!(r.text(), THEIRS.trim_end(), "the disk's version is what is shown");
    }
}

/// Where the other version went is told so that it can be found, whatever the state directory is and
/// however wide the screen: as a path a shell takes (`~/...` below the home), and again by `<Space>bk`,
/// alone on the line, cut from the left (`…`) only if it is wider than the screen, so that the file's
/// name stays. The message used to end in an absolute path, which a long `$TMPDIR` (macOS) cut off.
#[test]
fn safe_7_where_the_other_version_went_is_a_path_that_works_and_space_b_k_says_it_again() {
    let home = std::env::temp_dir();
    let mut r = conflicted_at("safe7-where", Skin::Depth, (100, 24), Some(home.clone()));
    r.keys(":w!\n");
    let kept = r.state.join("a.rs.1.disk");
    assert_eq!(std::fs::read_to_string(&kept).unwrap(), THEIRS, "the disk's version is the copy");
    let said = r.status().to_string();
    assert!(said.contains("saved a.rs; the disk's version is in ~/"), "{said:?}");
    r.keys("j");
    assert!(!r.status().contains("the disk's version"), "{:?}", r.status());
    r.keys("<Space>bk");
    let again = r.status().to_string();
    assert!(again.ends_with("a.rs.1.disk") && !again.contains(' '), "{again:?}");
    assert!(again.chars().count() <= 100, "{again:?}");
    if !again.starts_with('…') {
        // it fits: it is the whole path, and a shell takes it
        assert_eq!(home.join(again.strip_prefix("~/").unwrap()), kept, "{again:?}");
    }
    // a screen narrower than the path: cut from the left, the file's name stays, nothing is wider than the line
    let mut r = conflicted_at("safe7-narrow", Skin::Flat, (40, 24), Some(home));
    r.keys(":w!\n");
    r.keys("j<Space>bk");
    let again = r.status().to_string();
    assert!(again.starts_with('…') && again.ends_with("a.rs.1.disk"), "{again:?}");
    assert!(again.chars().count() <= 40, "{again:?}");
    // `:e!` keeps mine the same way and the key says where
    let mut r = conflicted("safe7-yours", Skin::Flat);
    r.keys(":e!\n<Space>bk");
    assert!(r.status().ends_with("a.rs.1.yours"), "{:?}", r.status());
    // nothing was kept: it says so
    let mut r = rig_sized("safe7-none", &["a.rs"], &["a.rs"], (100, 24));
    r.keys("<Space>bk");
    assert_eq!(r.status(), "no other version was kept");
}
