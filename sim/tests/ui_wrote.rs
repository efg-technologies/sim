//! UI-19: a write from outside is seen when it arrives. The lines another process has written are lit
//! for `WROTE_MS` in the change's solid green (inverse with no color), over everything else of their
//! cells, and then stay as the marks of DISK-6 have them. The lines (pure, by the core's list), the
//! screens, the time, and what lights nothing.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use sim::color::{Syntax, Tones};
use sim::layout::Skin;
use sim::paint::{Painter, Role};
use sim::sim::{FLASH_MS, SimEffect, SimEvent, WROTE_MS};
use sim_core::core::app::Event;
use sim_core::core::color::{Rgb, palette};

const BG: Rgb = (0x1e, 0x1e, 0x1e);
const OLD: &str = "one\ntwo plain words\nthree\nfour\n";
const NEW: &str = "one\ntwo CHANGED words\nthree\nfour\nfive\n";

fn body(name: &str, skin: Skin, mono: bool, text: &str) -> Rig {
    let mut r = rig_env(name, &[&format!("a.txt={text}")], &["a.txt"], (80, 14), |e| {
        e.skin = skin;
        e.chrome = true;
        e.syn = if mono { Syntax::mono() } else { Syntax::new(true) };
    });
    r.send(SimEvent::Term(Event::Bg(BG)));
    r
}

/// The file now holds `text` on the disk.
fn written(r: &mut Rig, text: &str) {
    let id = r.sim.bufs[0].id;
    r.send(SimEvent::App(id, disk(text.as_bytes())));
}

fn flashes(r: &Rig) -> Vec<(u64, u64)> {
    r.log
        .iter()
        .filter_map(|e| if let SimEffect::Flash(n, ms) = e { Some((*n, *ms)) } else { None })
        .collect()
}

/// The row of the screen that has `text`.
fn row(r: &Rig, text: &str) -> usize {
    r.shot.frame.rows.iter().position(|x| x.text.contains(text)).unwrap()
}

fn col(r: &Rig, y: usize, needle: &str) -> u16 {
    let t = &r.shot.frame.rows[y].text;
    t[..t.find(needle).unwrap()].chars().count() as u16
}

/// Whether the cell is the change's solid green, bold (a flash), in the first paint that has a background.
/// (It is not the `add` block of the words that came, which an added line has whole.)
fn lit(r: &Rig, y: usize, x: u16) -> bool {
    let (green, on) = Tones::new(&palette(BG)).add_solid();
    r.shot.paints[y].iter().find(|(c, p)| c.contains(&x) && p.bg.is_some()).is_some_and(|(_, p)| {
        (p.bg, p.fg) == (Some(green), Some(on)) && p.bold && p.role == Role::Patch
    })
}

#[test]
fn ui_19_the_lines_another_process_wrote_are_lit_whole_and_then_put_out() {
    for skin in [Skin::Flat, Skin::Depth] {
        let mut r = body("ui19-lit", skin, false, OLD);
        written(&mut r, NEW);
        let (two, five, one) = (row(&r, "two CHANGED"), row(&r, "five"), row(&r, "one"));
        let x = col(&r, two, "two");
        let words = col(&r, two, "words");
        assert!(
            lit(&r, two, x) && lit(&r, two, words),
            "{skin:?}: the whole text of the changed line"
        );
        assert!(lit(&r, five, col(&r, five, "five")), "{skin:?}: and the added one");
        assert!(!lit(&r, one, col(&r, one, "one")), "{skin:?}: not a line that did not change");
        let fx = flashes(&r);
        assert_eq!(fx.len(), 1, "{skin:?}: one flash is asked for");
        assert_eq!(fx[0].1, WROTE_MS);
        // (150 ms is the yank's; a write comes from nowhere and is on the screen longer)
        assert_ne!(WROTE_MS, FLASH_MS);
        // when the shell says the time is over, the lines are as the marks have them
        r.send(SimEvent::Unflash(fx[0].0));
        let t = Tones::new(&palette(BG));
        assert!(!lit(&r, two, x), "{skin:?}: put out");
        let tint = r.shot.paints[two].iter().find(|(c, p)| c.contains(&x) && p.bg.is_some());
        assert_eq!(tint.and_then(|p| p.1.bg), Some(t.gain), "{skin:?}: the mark's tint stays");
    }
}

#[test]
fn ui_19_an_older_flash_s_end_does_not_put_out_a_newer_one() {
    let mut r = body("ui19-newer", Skin::Flat, false, OLD);
    written(&mut r, NEW);
    written(&mut r, "one\ntwo CHANGED words\nthree\nfour\nfive\nsix\n");
    let fx = flashes(&r);
    assert_eq!(fx.len(), 2);
    r.send(SimEvent::Unflash(fx[0].0));
    let six = row(&r, "six");
    assert!(lit(&r, six, col(&r, six, "six")), "the newer one is still lit");
    r.send(SimEvent::Unflash(fx[1].0));
    assert!(!lit(&r, six, col(&r, six, "six")));
}

#[test]
fn ui_19_a_line_of_a_conflict_is_lit_and_a_deletion_point_is_not() {
    let mut r = body("ui19-conflict", Skin::Flat, false, "a\nb\nc\nd\ne\nf\n");
    r.keys("jrXjjjrY");
    written(&mut r, "a\nB\nc\nd\nE\nf\n");
    let (x, y) = (row(&r, " X"), row(&r, " Y"));
    assert!(
        lit(&r, x, col(&r, x, "X")) && lit(&r, y, col(&r, y, "Y")),
        "mine, which the disk wrote too"
    );
    let mut r = body("ui19-gone", Skin::Flat, false, "one\ntwo\nthree\nfour\n");
    written(&mut r, "one\nfour\n");
    assert_eq!(flashes(&r).len(), 0, "nothing was written but a deletion: no text to light");
    assert!((0..r.shot.frame.rows.len()).all(|y| (0..80).all(|x| !lit(&r, y, x))));
}

#[test]
fn ui_19_an_edit_of_the_text_puts_the_flash_out_and_a_yank_s_is_over_it() {
    let mut r = body("ui19-edit", Skin::Flat, false, OLD);
    written(&mut r, NEW);
    let two = row(&r, "two CHANGED");
    let x = col(&r, two, "two");
    assert!(lit(&r, two, x));
    // a yank of that line: its flash (the accent's) is over the write's
    r.keys("jyy");
    let t = Tones::new(&palette(BG));
    let on = r.shot.paints[two].iter().find(|(c, p)| c.contains(&x) && p.bg.is_some()).unwrap();
    assert_eq!(on.1.bg, Some(t.accent_block().0));
    // typing changes the text: the flash was of a version of it
    r.keys("Ax<Esc>");
    assert!(!lit(&r, two, x));
}

#[test]
fn ui_19_what_the_user_did_lights_nothing_and_neither_does_a_write_that_changes_nothing() {
    let mut r = body("ui19-user", Skin::Flat, false, OLD);
    written(&mut r, NEW);
    let n = flashes(&r).len();
    // the same bytes again (a save's own echo): no marks, no flash
    written(&mut r, NEW);
    assert_eq!(flashes(&r).len(), n);
    // undo of the merge brings the old text back: it is not a write
    let fx = flashes(&r)[0].0;
    r.send(SimEvent::Unflash(fx));
    r.keys("u");
    let all = (0..r.shot.frame.rows.len()).all(|y| (0..80).all(|x| !lit(&r, y, x)));
    assert!(all && flashes(&r).len() == n);
}

#[test]
fn ui_19_without_color_the_lines_are_inverse_and_in_calm_there_is_nothing() {
    let mut r = body("ui19-mono", Skin::Flat, true, OLD);
    written(&mut r, NEW);
    let mut p = Painter::new(Vec::new());
    p.draw(r.shot.clone()).unwrap();
    let out = String::from_utf8(p.out).unwrap();
    let at = out.find("two ").unwrap();
    let sgr = out[..at].rsplit("\x1b[").next().unwrap();
    assert!(sgr.starts_with("0") && sgr.contains(";7"), "inverse: {sgr:?}");
    let mut r = body("ui19-calm", Skin::Calm, false, OLD);
    written(&mut r, NEW);
    assert!(
        (0..r.shot.frame.rows.len()).all(|y| (0..80).all(|x| !lit(&r, y, x))),
        "calm draws no flash, as it draws none of the yank's"
    );
}

#[test]
fn ui_19_a_char_shown_in_another_form_in_a_lit_line_is_still_the_alert_s_block() {
    let mut r = body("ui19-hostile", Skin::Flat, false, OLD);
    written(&mut r, "one\ntwo pl\u{1b}ain wo\u{202e}rds\nthree\nfour\n");
    let y = row(&r, "two pl");
    let (back, on) = Tones::new(&palette(BG)).alert_block();
    let mut p = Painter::new(Vec::new());
    p.draw(r.shot.clone()).unwrap();
    let out = String::from_utf8(p.out).unwrap();
    let chunk = out[..out.find("^[").unwrap()].rsplit("\x1b[").next().unwrap().to_string();
    let block = format!("0;38;2;{};{};{};48;2;{};{};{}m", on.0, on.1, on.2, back.0, back.1, back.2);
    assert_eq!(chunk, block, "the token is the alert's block, whatever is lit around it");
    assert!(lit(&r, y, col(&r, y, "two")), "and the rest of the line is lit");
    assert!(!r.shot.frame.rows[y].text.contains('\u{1b}'));
}
