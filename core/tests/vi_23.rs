//! VI-23 (`:r {file}` and `:r !{cmd}`): the lines go below the line, as one undo step. The shell is played
//! here: the file is the answer to `Effect::Open`, the command's output that to `Effect::Spawn`.
#![allow(clippy::unwrap_used, clippy::panic)] // helpers
mod common;
use common::*;
use sim_core::core::app::{App, Effect, Event, Read};

fn opened(fx: &[Effect]) -> String {
    fx.iter().find_map(|e| if let Effect::Open(p) = e { Some(p.clone()) } else { None }).unwrap()
}

/// Types `keys` on a file `f` holding `text`, answers the `:r` with `file`, and gives the App back.
fn read(text: &str, keys: &str, file: Read) -> App {
    let mut a = app_with(Some("f"), text.as_bytes(), (80, 24));
    let path = opened(&feed(&mut a, keys));
    a.update(Event::Opened(path, file, 1));
    a
}

#[test]
fn vi_23_r_file_puts_the_file_s_lines_below_the_line_as_one_undo_step() {
    let mut a = read("a\nb\nc\n", ":r other\n", Ok(Some(b"x\n  y\n".to_vec())));
    assert_eq!(marked(&a), "a\n|x\n  y\nb\nc");
    assert_eq!(a.msg.as_deref(), Some("2 lines read from other"));
    feed(&mut a, "u");
    assert_eq!(text(&a), "a\nb\nc");
    // an address says which line, a range its last; `:$r` is at the end
    let a = read("a\nb\nc\n", ":2r o\n", Ok(Some(b"x\n".to_vec())));
    assert_eq!(marked(&a), "a\nb\n|x\nc");
    let a = read("a\nb\nc\n", ":1,2r o\n", Ok(Some(b"x\n".to_vec())));
    assert_eq!(marked(&a), "a\nb\n|x\nc");
    let a = read("a\nb\nc\n", ":$r o\n", Ok(Some(b"x".to_vec())));
    assert_eq!(marked(&a), "a\nb\nc\n|x");
    // what the file holds is its text: no CRLF and no BOM come in, and a line without its line break too
    let a = read("a\n", ":r o\n", Ok(Some(b"\xEF\xBB\xBFx\r\ny\r\n".to_vec())));
    assert_eq!(text(&a), "a\nx\ny");
    // no name: the file itself
    let mut a = app_with(Some("f"), b"a\n", (80, 24));
    assert_eq!(opened(&feed(&mut a, ":r\n")), "f");
}

#[test]
fn vi_23_a_file_that_is_not_text_or_not_there_is_not_read_and_says_why() {
    for (file, why) in [
        (Ok(Some(b"x\xff\n".to_vec())), "not read: o: not valid UTF-8"),
        (Ok(Some(b"x\0y\n".to_vec())), "not read: o: binary file (NUL in the first 8 KB)"),
        (Ok(None), "not read: o does not exist"),
        (Err("Permission denied".into()), "not read: Permission denied"),
        (Ok(Some(vec![])), "0 lines read from o"),
    ] {
        let a = read("a\n", ":r o\n", file);
        assert_eq!((text(&a), a.msg.as_deref()), ("a".into(), Some(why)));
        assert!(!a.buf.dirty(), "{why}");
    }
}

#[test]
fn vi_23_a_read_that_cannot_go_in_changes_nothing() {
    // typed meanwhile: the line it was to go below may be another
    let mut a = app_with(Some("f"), b"a\nb\n", (80, 24));
    let path = opened(&feed(&mut a, ":r o\n"));
    feed(&mut a, "x");
    a.update(Event::Opened(path, Ok(Some(b"x\n".to_vec())), 1));
    assert_eq!((text(&a), a.msg.as_deref()), ("\nb".into(), Some("not read: edited meanwhile")));
    // a read-only buffer, and a save still running
    let mut a = App::new(Some("f".into()), Err("no access".into()), (80, 24));
    assert!(!feed(&mut a, ":r o\n").iter().any(|e| matches!(e, Effect::Open(_))));
    assert_eq!(a.msg.as_deref(), Some("read-only: no access"));
    let mut a = app_with(Some("f"), b"a\n", (80, 24));
    feed(&mut a, "x:w\n");
    assert!(!feed(&mut a, ":r o\n").iter().any(|e| matches!(e, Effect::Open(_))));
    assert_eq!(a.msg.as_deref(), Some("a save is already running"));
    // a read that is still to be answered: another `:r` and a save wait for it
    let mut a = app_with(Some("f"), b"a\n", (80, 24));
    feed(&mut a, ":r o\n");
    assert!(!feed(&mut a, ":r p\n").iter().any(|e| matches!(e, Effect::Open(_))));
    assert_eq!(a.msg.as_deref(), Some("a read is already running"));
    a.update(Event::Opened("o".into(), Ok(Some(b"x\n".to_vec())), 1));
    assert_eq!(text(&a), "a\nx");
}

#[test]
fn vi_23_r_cmd_runs_the_command_as_a_filter_of_no_lines_and_puts_its_output_below() {
    for keys in [":r !date\n", ":r!date\n"] {
        let mut a = app_with(Some("f"), b"a\nb\n", (80, 24));
        let fx = feed(&mut a, keys);
        let spawn = fx.iter().find_map(|e| match e {
            Effect::Spawn { cmd, input } => Some((cmd.clone(), input.clone())),
            _ => None,
        });
        assert_eq!(spawn, Some(("date".into(), vec![])), "{keys}");
        a.update(Event::FilterDone(Some(0), b"now\nlater".to_vec(), vec![], far()));
        assert_eq!(text(&a), "a\nnow\nlater\nb", "{keys}");
        assert_eq!(a.msg.as_deref(), Some("2 lines from the filter · u to undo"));
        feed(&mut a, "u");
        assert_eq!(text(&a), "a\nb");
    }
    let mut a = app_with(Some("f"), b"a\nb\n", (80, 24));
    feed(&mut a, ":$r !x\n");
    a.update(Event::FilterDone(Some(0), b"z\n".to_vec(), vec![], far()));
    assert_eq!(text(&a), "a\nb\nz");
    // an empty command is no command, and plain mode runs none (PLAIN-1)
    let mut a = app_with(Some("f"), b"a\n", (80, 24));
    feed(&mut a, ":r !\n");
    assert_eq!(a.msg.as_deref(), Some("usage: :r {file} or :r !cmd"));
    a.plain = true;
    assert!(!feed(&mut a, ":r !date\n").iter().any(|e| matches!(e, Effect::Spawn { .. })));
    assert_eq!(a.msg.as_deref(), Some("plain mode runs no program: a filter is refused"));
}
