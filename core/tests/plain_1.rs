//! PLAIN-1 on App: plain mode runs no program from the core: a filter is refused with a message that
//! names plain mode, the hook does not run, and the status line says `plain`.
mod common;
use common::*;
use sim_core::core::app::{App, Effect};

fn plain(text: &str) -> App {
    let mut a = app_with(Some("f"), text.as_bytes(), (80, 12));
    a.plain = true;
    a
}

fn spawns(fx: &[Effect]) -> usize {
    fx.iter().filter(|e| matches!(e, Effect::Spawn { .. } | Effect::Hook(_))).count()
}

#[test]
fn plain_1_a_filter_is_refused_naming_plain_mode_and_no_program_is_spawned() {
    for keys in [":%!sort\n", ":1,2!tr a b\n", "Vj!cat\n", "!jcat\n"] {
        let mut a = plain("b\na\nc\n");
        let fx = feed(&mut a, keys);
        assert_eq!(spawns(&fx), 0, "{keys:?}");
        assert!(msg(&mut a).contains("plain mode"), "{keys:?}: {}", msg(&mut a));
        assert_eq!(text(&a), "b\na\nc");
    }
    // Not in plain mode the same keys start the program.
    let mut a = app_with(Some("f"), b"b\na\n", (80, 12));
    assert_eq!(spawns(&feed(&mut a, ":%!sort\n")), 1);
}

#[test]
fn plain_1_the_hook_does_not_run_at_the_start_or_when_normal_mode_is_entered() {
    let mut a = plain("a\n");
    a.hook = Some("fcitx5-remote -c".into());
    assert_eq!(spawns(&a.start("")), 0);
    assert_eq!(spawns(&feed(&mut a, "ix<Esc>")), 0);
    let mut b = app_with(Some("f"), b"a\n", (80, 12));
    b.hook = Some("fcitx5-remote -c".into());
    assert_eq!(spawns(&b.start("")), 1, "and it does when plain is off");
}

#[test]
fn plain_1_the_status_line_says_plain() {
    let mut a = plain("a\n");
    assert_eq!(msg(&mut a), "f  1:1  NORMAL  plain");
    feed(&mut a, "x");
    assert_eq!(msg(&mut a), "f  1:1  NORMAL  modified · plain");
    let mut b = app_with(Some("f"), b"a\n", (80, 12));
    assert_eq!(msg(&mut b), "f  1:1  NORMAL");
}

fn msg(a: &mut App) -> String {
    status(&screen(a)).to_string()
}
