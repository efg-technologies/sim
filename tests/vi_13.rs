//! VI-13: finishing takes few keys: Ctrl-s saves everywhere, ZZ, ZQ, q, and Ctrl-c.
mod common;
use common::*;
use si::core::app::{App, Effect, Event};
use si::core::vi::Mode;

fn named(text: &str) -> App {
    app_with(Some("f"), text.as_bytes(), (60, 8))
}

fn writes(fx: &[Effect]) -> Vec<(&str, &[u8])> {
    fx.iter()
        .filter_map(|e| match e {
            Effect::Write { path, bytes, .. } => Some((path.as_str(), bytes.as_slice())),
            _ => None,
        })
        .collect()
}

fn quit(fx: &[Effect]) -> Option<i32> {
    fx.iter().find_map(|e| if let Effect::Quit(c) = e { Some(*c) } else { None })
}

fn msg(a: &mut App) -> String {
    status(&screen(a)).to_string()
}

#[test]
fn vi_13_ctrl_s_saves_in_every_mode_and_keeps_the_mode() {
    for (setup, mode) in [
        ("x", Mode::Normal),
        ("xi", Mode::Insert),
        ("xv", Mode::Visual),
        ("xV", Mode::VLine),
        ("x:q", Mode::Cmd),
        ("x/ab", Mode::Cmd),
    ] {
        let mut a = named("abc\n");
        feed(&mut a, setup);
        let fx = feed(&mut a, "<C-s>");
        assert_eq!(writes(&fx), [("f", &b"bc\n"[..])], "{setup}");
        assert_eq!(a.vi.mode, mode, "{setup}");
    }
    let mut a = named("abc\n");
    feed(&mut a, ":w");
    feed(&mut a, "<C-s>");
    assert_eq!(status(&screen(&mut a)), ":w", "the command line is left as it was");
    let mut a = named("abc\n");
    feed(&mut a, "d");
    let fx = feed(&mut a, "<C-s>");
    assert_eq!(writes(&fx).len(), 1, "a half typed command does not stop it");
}

#[test]
fn vi_13_ctrl_s_is_not_a_change_for_dot() {
    let mut a = named("a b c d");
    feed(&mut a, "dw<C-s>.");
    assert_eq!(text(&a), "c d");
}

#[test]
fn vi_13_zz_is_x() {
    let mut a = named("abc\n");
    let fx = feed(&mut a, "xZZ");
    assert_eq!((writes(&fx), quit(&fx)), (vec![("f", &b"bc\n"[..])], None));
    assert_eq!(
        quit(&a.update(Event::Saved(Ok((1, None)), 0))),
        Some(0),
        "quits once the save succeeded"
    );
    let mut a = named("abc\n");
    let fx = feed(&mut a, "ZZ");
    assert_eq!((writes(&fx).len(), quit(&fx)), (0, Some(0)), "nothing to save: just quit");
    let mut a = named("abc\n");
    feed(&mut a, "xZZ");
    a.update(Event::Saved(Err("disk full".into()), 0));
    assert_eq!(msg(&mut a), "disk full");
}

#[test]
fn vi_13_zq_is_q_bang() {
    let mut a = named("abc\n");
    let fx = feed(&mut a, "xZQ");
    assert_eq!((writes(&fx).len(), quit(&fx)), (0, Some(0)));
    let mut a = named("abc\n");
    assert_eq!(quit(&feed(&mut a, "ZQ")), Some(0));
    let mut a = named("abc\n");
    let fx = feed(&mut a, "ZxZ<Esc>");
    assert!(quit(&fx).is_none() && writes(&fx).is_empty(), "Z needs Z or Q after it");
}

#[test]
fn vi_13_q_is_colon_q() {
    let mut a = named("abc\n");
    assert_eq!(quit(&feed(&mut a, "q")), Some(0));
    let mut a = named("abc\n");
    let fx = feed(&mut a, "xq");
    assert!(quit(&fx).is_none());
    let refused = msg(&mut a);
    assert_eq!(refused, "no write since last change (:q! to override)");
    let mut b = named("abc\n");
    feed(&mut b, "x:q\n");
    assert_eq!(msg(&mut b), refused, "the same message as :q");
}

#[test]
fn vi_13_ctrl_c_says_q_to_quit_when_there_is_nothing_to_cancel() {
    let mut a = named("abc\n");
    feed(&mut a, "<C-c>");
    assert_eq!(msg(&mut a), "q to quit");
    feed(&mut a, "l");
    assert_eq!(msg(&mut a), "f  1:2  NORMAL");
    for (keys, want) in [
        ("3<C-c>x", "bc"),
        ("d<C-c>x", "bc"),
        ("2d3<C-c>x", "bc"),
        ("3<Esc>x", "bc"),
        ("d<Esc>x", "bc"),
        ("2f<Esc>x", "bc"),
        ("g<C-c>x", "bc"),
    ] {
        let mut a = named("abc\n");
        feed(&mut a, keys.trim_end_matches('x'));
        assert_eq!(msg(&mut a), "f  1:1  NORMAL", "{keys}: cancelling a pending command is silent");
        feed(&mut a, "x");
        assert_eq!(text(&a), want, "{keys}");
    }
    let mut a = named("abc\n");
    feed(&mut a, "<Esc>");
    assert_eq!(msg(&mut a), "f  1:1  NORMAL");
}

#[test]
fn vi_13_ctrl_c_leaves_visual_and_insert_and_the_command_line() {
    for setup in ["v", "V", "i", ":", "/", ":abc"] {
        let mut a = named("abc\n");
        feed(&mut a, setup);
        feed(&mut a, "<C-c>");
        assert_eq!(a.vi.mode, Mode::Normal, "{setup}");
        assert_eq!(msg(&mut a), "f  1:1  NORMAL", "{setup}");
    }
}
