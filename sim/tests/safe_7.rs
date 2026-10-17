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
    let mut r = rig_env(name, &[&format!("a.rs={SRC}")], &["a.rs"], (100, 24), |e| {
        e.skin = skin;
        e.chrome = true;
        e.syn = Syntax::new(true);
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
        assert!(std::path::Path::new(status.rsplit(" is in ").next().unwrap()).exists());
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
