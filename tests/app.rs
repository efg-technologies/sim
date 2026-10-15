//! CLI-1, CLI-3, SAFE-1, SAFE-4 on App, and the minimal key set (typing, motions, delete, undo).
mod common;
use common::*;
use si::core::app::{App, Effect, Event};
use si::core::buffer::{ABSENT, hash};

fn quit(fx: &[Effect]) -> Option<i32> {
    fx.iter().find_map(|e| if let Effect::Quit(c) = e { Some(*c) } else { None })
}

fn write(fx: &[Effect]) -> Option<(&str, &[u8], Option<u64>)> {
    fx.iter().find_map(|e| match e {
        Effect::Write { path, bytes, expect_hash } => {
            Some((path.as_str(), bytes.as_slice(), *expect_hash))
        }
        _ => None,
    })
}

fn named(text: &str) -> App {
    app_with(Some("f"), text.as_bytes(), (40, 8))
}

fn msg(a: &mut App) -> String {
    status(&screen(a)).to_string()
}

#[test]
fn cli_1_the_command_runs_after_the_first_render() {
    let fx = named("a\nb\nc\n").start(":3\n");
    assert!(matches!(&fx[0], Effect::Render(f) if status(f) == "f  1:1  NORMAL"));
    assert!(matches!(fx.last(), Some(Effect::Render(f)) if status(f) == "f  3:1  NORMAL"));

    let fx = named("a\n").start(":w\n");
    assert!(matches!(fx[0], Effect::Render(_)) && write(&fx).is_some());

    let fx = named("a\n").start(":q\n");
    assert!(matches!(fx[0], Effect::Render(_)) && quit(&fx) == Some(0));
    assert!(
        matches!(&named("a\nb\n").start("")[..], [Effect::Render(_)]),
        "no command: only the first render"
    );
}

#[test]
fn cli_3_exit_codes() {
    let code = |keys: &str| quit(&feed(&mut named("a\n"), keys));
    assert_eq!((code(":q\n"), code(":q!\n"), code(":cq\n")), (Some(0), Some(0), Some(1)));
    assert_eq!(
        (code("q"), code("ZQ"), code("x:q!\n"), code("x:cq\n")),
        (Some(0), Some(0), Some(0), Some(1))
    );
    assert_eq!(
        (code("x:q\n"), code("xq")),
        (None, None),
        "`:q` is refused while the buffer is modified"
    );
    assert_eq!(code(":x\n"), Some(0), "`:x` writes only when modified");
    for keys in ["x:wq\n", "x:x\n", "xZZ"] {
        let mut a = named("a\n");
        let fx = feed(&mut a, keys);
        assert!(write(&fx).is_some() && quit(&fx).is_none(), "{keys:?}: quits after the save");
        assert_eq!(quit(&a.update(Event::Saved(Ok((0, None))))), Some(0), "{keys:?}");
    }
    let mut a = named("a\n");
    feed(&mut a, "x:wq\n");
    assert_eq!(
        quit(&a.update(Event::Saved(Err("disk full".into())))),
        None,
        "no exit when it failed"
    );
    assert_eq!(quit(&named("a\n").update(Event::Signal(15))), Some(143));
    assert_eq!(quit(&named("a\n").update(Event::Signal(1))), Some(129));
}

#[test]
fn safe_1_wq_quits_only_after_the_save_succeeded() {
    let mut a = named("a\n");
    let fx = feed(&mut a, "x:wq\n");
    assert!(write(&fx).is_some() && quit(&fx).is_none());
    let fx = a.update(Event::Saved(Err("cannot save f: disk full".into())));
    assert!(quit(&fx).is_none());
    assert_eq!(msg(&mut a), "cannot save f: disk full");
    assert!(a.buf.dirty(), "a failed save leaves the buffer modified");
    feed(&mut a, ":wq\n");
    assert_eq!(quit(&a.update(Event::Saved(Ok((1, None))))), Some(0));
}

#[test]
fn safe_1_a_failing_directory_sync_after_the_rename_is_only_a_warning() {
    let warn = || Event::Saved(Ok((1, Some("could not sync the directory: busy".into()))));
    let mut a = app_with(Some("f"), b"a\n", (80, 8));
    feed(&mut a, "x:w\n");
    a.update(warn());
    assert_eq!(msg(&mut a), "saved f (warning: could not sync the directory: busy)");
    assert!(!a.buf.dirty(), "the text is on disk");
    let mut a = app_with(Some("f"), b"a\n", (80, 8));
    feed(&mut a, "x:wq\n");
    assert_eq!(quit(&a.update(warn())), Some(0), "the save counts as done");
}

#[test]
fn safe_1_editing_goes_on_while_a_save_runs() {
    let mut a = named("a\n");
    let fx = feed(&mut a, "ib\x1b:w\n");
    assert_eq!(write(&fx).map(|w| w.1), Some(&b"ba\n"[..]));
    feed(&mut a, "ic\x1b");
    a.update(Event::Saved(Ok((7, None))));
    assert!(a.buf.dirty(), "only the saved text is clean");
    assert_eq!(a.buf.base.to_string(), "ba");
    feed(&mut a, "l");
    assert_eq!(msg(&mut a), "f  1:2  NORMAL  modified");
    feed(&mut a, "u");
    assert!(!a.buf.dirty(), "undoing back to the saved text is clean");
    assert_eq!(msg(&mut a), "f  1:1  NORMAL");
    let fx = feed(&mut a, "ix\x1b:wq\n");
    assert_eq!(
        write(&fx).map(|w| w.2),
        Some(Some(7)),
        "the hash of what we wrote is expected next time"
    );
    feed(&mut a, "ia\x1b");
    assert!(
        quit(&a.update(Event::Saved(Ok((8, None))))).is_none(),
        "edited while saving: not quitting"
    );
}

#[test]
fn safe_1_a_save_is_refused_when_the_disk_changed() {
    let mut a = app_with(Some("f"), b"a\n", (80, 8));
    let w = |fx: &[Effect]| write(fx).map(|w| w.2);
    assert_eq!(w(&feed(&mut a, ":w\n")), Some(Some(hash(b"a\n"))));
    a.update(Event::SaveConflict(b"other\n".to_vec()));
    assert_eq!(msg(&mut a), "not saved: file changed on disk (:w! to overwrite)");
    assert_eq!(w(&feed(&mut a, ":w!\n")), Some(None), "`:w!` overwrites");
    assert_eq!(w(&feed(&mut a, ":w other\n")), None, "a save is already running");
    a.update(Event::Saved(Ok((1, None))));
    assert_eq!(
        w(&feed(&mut a, ":w other\n")),
        Some(Some(ABSENT)),
        "another file must not exist yet"
    );
    a.update(Event::Saved(Ok((2, None))));
    assert_eq!(msg(&mut a), "saved other");
    assert_eq!(a.path.as_deref(), Some("f"), "`:w other` writes a copy");
}

#[test]
fn safe_1_an_unnamed_buffer_takes_the_name_of_the_first_write() {
    let mut a = app("");
    assert_eq!(msg(&mut a), "[no name]  1:1  NORMAL");
    feed(&mut a, ":w\n");
    assert_eq!(msg(&mut a), "no file name");
    let fx = feed(&mut a, "ihi\x1b:w new.txt\n");
    assert_eq!(write(&fx), Some(("new.txt", &b"hi\n"[..], Some(ABSENT))));
    a.update(Event::Saved(Ok((hash(b"hi\n"), None))));
    assert_eq!(msg(&mut a), "saved new.txt");
    assert_eq!(
        write(&feed(&mut a, ":w\n")).map(|w| (w.0, w.2)),
        Some(("new.txt", Some(hash(b"hi\n"))))
    );
}

#[test]
fn safe_4_a_read_only_buffer_refuses_every_edit_and_write() {
    for bytes in [&[b'a', 0xff, b'\n'][..], b"a\0b\n"] {
        let mut a = app_with(Some("f"), bytes, (60, 8));
        let why = a.buf.readonly.clone().unwrap();
        assert!(why.starts_with("read-only: "), "{why}");
        for keys in ["x", "dd", "ix", "o", "a", ":w\n", ":w!\n", ":wq\n"] {
            let before = text(&a);
            let fx = feed(&mut a, keys);
            assert!(write(&fx).is_none() && quit(&fx).is_none(), "{keys:?}");
            assert_eq!(text(&a), before, "{keys:?}");
            assert!(!a.buf.dirty());
            assert_eq!(msg(&mut a), why, "{keys:?}");
        }
        a.update(Event::Paste("x".into()));
        assert_eq!(msg(&mut a), why);
        assert_eq!(quit(&feed(&mut a, ":q\n")), Some(0), "a read-only buffer can be left");
    }
    assert!(app_with(Some("f"), "é\n".as_bytes(), (60, 8)).buf.readonly.is_none());
    let nul_late = [vec![b'a'; 8192], vec![0]].concat();
    assert!(
        app_with(Some("f"), &nul_late, (60, 8)).buf.readonly.is_none(),
        "only the first 8 KB count"
    );
    let mut a = App::new(Some("d".into()), Err("Is a directory".into()), (60, 8));
    assert_eq!(
        (text(&a).as_str(), a.buf.readonly.as_deref()),
        ("", Some("read-only: cannot read: Is a directory"))
    );
    assert!(write(&feed(&mut a, ":w\n")).is_none());
}

#[test]
fn flow_insert_enter_keeps_the_indent_and_backspace_joins_lines() {
    let mut a = named("  foo\nbar");
    feed(&mut a, "4la\nx");
    assert_eq!(text(&a), "  foo\n  x\nbar");
    feed(&mut a, "\x7f\x7f\x7f\x7f");
    assert_eq!(text(&a), "  foo\nbar", "BS at the start of a line joins it to the line above");
    assert_eq!(a.buf.cursor, 5);
    feed(&mut a, "\x1b");
    assert_eq!(a.buf.col(), 4, "Esc steps back");
    feed(&mut a, "ox\x1bu");
    assert_eq!(text(&a), "  foo\nbar");
    feed(&mut a, "\x12");
    assert_eq!(text(&a), "  foo\n  x\nbar", "`o` keeps the indent");
}

#[test]
fn flow_motions_stay_inside_the_text() {
    let mut a = named("abc\n\nlonger line\nxy");
    feed(&mut a, "hhh");
    assert_eq!(a.buf.cursor, 0);
    feed(&mut a, "99l");
    assert_eq!(a.buf.col(), 2, "l stops on the last char");
    feed(&mut a, "jj");
    assert_eq!(
        (a.buf.line_of(a.buf.cursor), a.buf.col()),
        (2, 2),
        "j remembers the column through a short line"
    );
    feed(&mut a, "jjjj");
    assert_eq!((a.buf.line_of(a.buf.cursor), a.buf.col()), (3, 1));
    feed(&mut a, "kkkkkk");
    assert_eq!(a.buf.line_of(a.buf.cursor), 0);
    feed(&mut a, ":3\n");
    assert_eq!(a.buf.line_of(a.buf.cursor), 2);
    feed(&mut a, ":99\n");
    assert_eq!(a.buf.line_of(a.buf.cursor), 3);
    feed(&mut a, ":0\n:nonsense\n");
    assert_eq!((a.buf.cursor, msg(&mut a).as_str()), (0, "not an editor command: nonsense"));
}

#[test]
fn flow_delete_and_undo() {
    let mut a = named("one\ntwo\nthree\nfour");
    for (keys, want) in
        [("2x", "e\ntwo\nthree\nfour"), ("dd", "two\nthree\nfour"), ("2dd", "four"), ("dd", "")]
    {
        feed(&mut a, keys);
        assert_eq!(text(&a), want, "{keys}");
    }
    for want in ["four", "two\nthree\nfour", "e\ntwo\nthree\nfour", "one\ntwo\nthree\nfour"] {
        feed(&mut a, "u");
        assert_eq!(text(&a), want);
    }
    feed(&mut a, "u");
    assert_eq!(msg(&mut a), "nothing to undo");
    feed(&mut a, "\x12\x12\x12\x12");
    assert_eq!(text(&a), "", "redo goes forward again");
    feed(&mut a, "\x12");
    assert_eq!(msg(&mut a), "nothing to redo");
    feed(&mut a, "ixyz\x1b");
    assert_eq!(text(&a), "xyz");
}

#[test]
fn flow_the_last_line_goes_with_the_break_before_it() {
    let mut a = named("a\nb\nc");
    feed(&mut a, "jjdd");
    assert_eq!((text(&a).as_str(), a.buf.line_of(a.buf.cursor)), ("a\nb", 1));
    let mut a = named("a\nb\nc\n");
    feed(&mut a, "jjdd");
    assert_eq!(a.buf.encode(), b"a\nb\n", "the file's final newline stays");
    assert_eq!(a.buf.line_of(a.buf.cursor), 1, "the cursor stays on the last line");
}

#[test]
fn flow_one_insert_session_and_one_paste_are_one_undo_unit() {
    let mut a = named("a");
    feed(&mut a, "ihello\nworld\x1b");
    feed(&mut a, "u");
    assert_eq!(text(&a), "a");
    feed(&mut a, "i");
    a.update(Event::Paste("p\r\nq\rr".into()));
    assert_eq!(text(&a), "p\nq\nra");
    feed(&mut a, "\x1bu");
    assert_eq!(text(&a), "a");
    a.update(Event::Paste("zz".into()));
    assert_eq!(
        (text(&a).as_str(), a.buf.col()),
        ("zza", 1),
        "a paste in Normal mode is text, not keys"
    );
    feed(&mut a, ":");
    a.update(Event::Paste("w\nq".into()));
    assert_eq!(status(&screen(&mut a)), ":w q");
}

#[test]
fn flow_keys_that_mean_nothing_are_ignored() {
    let mut a = named("abc");
    feed(&mut a, "Q&K\x1b3\x03");
    assert_eq!((text(&a).as_str(), a.buf.cursor), ("abc", 0));
    feed(&mut a, "dx");
    assert_eq!(text(&a), "abc", "an operator followed by a key that is no motion does nothing");
    assert_eq!(a.update(Event::Tick(5)).len(), 0);
}
