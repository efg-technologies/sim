//! GUI-6: `SimEvent::Colors` tells sim the background and the ANSI colors that a terminal would have
//! answered with (OSC 11 and OSC 4), as they are, because a window has no terminal to ask. It does
//! what the answers do, with the same result: the screen after it is the screen after them.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use sim::color::Syntax;
use sim::layout::Skin;
use sim::sim::SimEvent;
use sim_core::core::app::Event;
use sim_core::core::color::Rgb;

const BG: Rgb = (0x28, 0x2c, 0x34);
const MD: &str = "# Title\n\nSome `code` here.\n\n```rust\nfn main() { let s = \"hi\"; }\n```\n";

/// The ANSI colors a window would tell (red, green, yellow, magenta, cyan; the others it has none for).
fn ansi() -> [Option<Rgb>; 8] {
    let mut a = [None; 8];
    a[1] = Some((0xe0, 0x6c, 0x75));
    a[2] = Some((0x98, 0xc3, 0x79));
    a[3] = Some((0xe5, 0xc0, 0x7b));
    a[5] = Some((0xc6, 0x78, 0xdd));
    a[6] = Some((0x56, 0xb6, 0xc2));
    a
}

/// A rig that has been told nothing yet: no background, no colors.
fn fresh(name: &str) -> Rig {
    rig_env(name, &[&format!("a.md={MD}")], &["a.md"], (100, 24), |e| {
        e.skin = Skin::Depth;
        e.syn = Syntax::new(true);
        e.chrome = true;
    })
}

fn osc((r, g, b): Rgb, i: usize) -> SimEvent {
    SimEvent::Osc(format!("4;{i};rgb:{r:02x}{r:02x}/{g:02x}{g:02x}/{b:02x}{b:02x}"))
}

#[test]
fn gui_6_the_colors_do_what_the_terminal_s_answers_do_and_the_screen_is_the_same() {
    let mut told = fresh("g6-told");
    told.send(SimEvent::Colors(BG, ansi()));
    let mut asked = fresh("g6-asked");
    asked.send(SimEvent::Term(Event::Bg(BG)));
    for (i, c) in ansi().iter().enumerate() {
        if let Some(c) = c {
            asked.send(osc(*c, i));
        }
    }
    assert_eq!(told.shot.frame, asked.shot.frame);
    assert_eq!(told.shot.looks, asked.shot.looks);
    assert_eq!(told.shot.paints, asked.shot.paints);
    assert_eq!(told.shot.syn, asked.shot.syn);
    assert_eq!(told.shot.layers, asked.shot.layers);
    assert!(told.shot.frame.pal.is_some(), "the palette is made of the background");
    // and the screen is the colored one, not the fallback of a terminal that was not heard from
    assert!(told.shot.paints.iter().flatten().any(|(_, p)| p.bg.is_some()));
}

#[test]
fn gui_6_a_window_that_has_told_its_colors_is_not_waiting_for_a_terminal() {
    let mut r = fresh("g6-asking");
    assert!(r.sim.asking(), "a terminal that has not answered may still be answering");
    r.send(SimEvent::Colors(BG, ansi()));
    // red, green, yellow, cyan, magenta are told; the others the syntax has no use for
    assert!(!r.sim.asking(), "nothing is pending: the quit does not wait");
}

#[test]
fn gui_6_a_color_not_told_is_made_of_the_hue_of_its_class_as_when_the_terminal_does_not_say() {
    let mut told = fresh("g6-part");
    let mut some = [None; 8];
    some[1] = ansi()[1];
    told.send(SimEvent::Colors(BG, some));
    let mut asked = fresh("g6-part-asked");
    asked.send(SimEvent::Term(Event::Bg(BG)));
    asked.send(osc(ansi()[1].unwrap(), 1));
    assert_eq!(told.shot.syn, asked.shot.syn);
    assert_eq!(told.shot.paints, asked.shot.paints);
    // a later Colors replaces the earlier: the background moves the palette
    told.send(SimEvent::Colors((255, 255, 255), [None; 8]));
    assert_eq!(told.shot.frame.pal.unwrap().bg, (255, 255, 255));
    assert_ne!(told.shot.frame.pal, asked.shot.frame.pal);
}

#[test]
fn gui_6_every_file_is_told_the_background_too() {
    let mut r = rig_env("g6-apps", &["a.md=# a\n", "b.txt=b\n"], &["a.md"], (100, 24), |e| {
        e.skin = Skin::Depth;
        e.syn = Syntax::new(true);
        e.chrome = true;
    });
    r.keys("<C-p>b.txt\n");
    assert_eq!(r.sim.bufs.len(), 2, "two files are open");
    r.send(SimEvent::Colors(BG, ansi()));
    for b in &r.sim.bufs {
        assert_eq!(b.app.pal.map(|p| p.bg), Some(BG), "{}", b.app.path.as_deref().unwrap_or("?"));
    }
}
