//! VI-7: ex commands and their ranges, and `:e` when the buffer is edited meanwhile.
//! Rows are (before, keys, after); `|` is the cursor.
mod common;
use common::*;
use si::core::app::{App, Effect, Event};

fn named(text: &str) -> App {
    app_with(Some("f"), text.as_bytes(), (70, 8))
}

fn msg(a: &mut App) -> String {
    status(&screen(a)).to_string()
}

fn says(before: &str, keys: &str) -> String {
    msg(&mut typed(before, keys))
}

fn open_of(fx: &[Effect]) -> Option<&str> {
    fx.iter().find_map(|e| if let Effect::Open(p) = e { Some(p.as_str()) } else { None })
}

fn quit(fx: &[Effect]) -> Option<i32> {
    fx.iter().find_map(|e| if let Effect::Quit(c) = e { Some(*c) } else { None })
}

#[test]
fn vi_7_a_line_number_goes_to_that_line() {
    table(&[
        ("|a\nb\nc", ":2<CR>", "a\n|b\nc"),
        ("|a\nb\nc", ":99<CR>", "a\nb\n|c"),
        ("a\nb\n|c", ":0<CR>", "|a\nb\nc"),
        ("|a\n  b\nc", ":2<CR>", "a\n  |b\nc"),
        ("|a\nb\nc", ":$<CR>", "a\nb\n|c"),
        ("a\nb\n|c", ":.<CR>", "a\nb\n|c"),
        ("|a\nb\nc", ":2,3<CR>", "a\nb\n|c"),
        ("|a\nb\nc", ":%<CR>", "a\nb\n|c"),
        ("|a\nb\nc", ": 2 <CR>", "a\n|b\nc"),
        ("|a\nb\nc", ":<CR>", "|a\nb\nc"),
    ]);
}

#[test]
fn vi_7_s_replaces_in_the_range() {
    table(&[
        ("|a\na\na", ":s/a/b/<CR>", "|b\na\na"),
        ("a\n|a\na", ":s/a/b/<CR>", "a\n|b\na"),
        ("|a\na\na", ":%s/a/b/<CR>", "b\nb\n|b"),
        ("|a\na\na", ":2,3s/a/b/<CR>", "a\nb\n|b"),
        ("|a\na\na", ":3,2s/a/b/<CR>", "a\nb\n|b"),
        ("|a\na\na", ":2s/a/b/<CR>", "a\n|b\na"),
        ("a\n|a\na", ":.,$s/a/b/<CR>", "a\nb\n|b"),
        ("|a\na\na", ":$s/a/b/<CR>", "a\na\n|b"),
        ("|a\na\na", ":1,2s/a/b/<CR>", "b\n|b\na"),
        ("|a\nb\na", ":%s/a/x/<CR>", "x\nb\n|x"),
        ("|aaa", ":s/a/b/<CR>", "|baa"),
        ("|aaa", ":s/a/b/g<CR>", "|bbb"),
        ("|a a\na a", ":%s/a/b/g<CR>", "b b\n|b b"),
        ("  |a", ":s/a/b/<CR>", "  |b"),
        ("|abc", ":s/b//<CR>", "|ac"),
        ("|abc", ":s/b/\\//<CR>", "|a/c"),
        ("|a/c", ":s/\\//-/<CR>", "|a-c"),
        ("|a/c", ":s/[\\/]/-/<CR>", "|a-c"),
        ("|abc", ":s/a.c/x/<CR>", "|x"),
        ("|ab", ":s/^/>/<CR>", "|>ab"),
        ("|ab", ":s/$/</<CR>", "|ab<"),
        ("|abc", ":s/b/x/<CR>", "|axc"),
    ]);
}

#[test]
fn vi_7_s_replacement_escapes() {
    table(&[
        ("|ab", ":s/a/[&]/<CR>", "|[a]b"),
        ("|ab", ":s/(a)(b)/\\2\\1/<CR>", "|ba"),
        ("|ab", ":s/(a)(b)/\\2\\2-\\1/<CR>", "|bb-a"),
        ("|ab", ":s/a/\\&/<CR>", "|&b"),
        ("|ab", ":s/a/\\\\/<CR>", "|\\b"),
        ("|ab", ":s/a/x\\/y/<CR>", "|x/yb"),
        ("|ab", ":s/b/&&/<CR>", "|abb"),
        ("|a.b", ":s/\\./-/<CR>", "|a-b"),
        ("|ab", ":s/(a)/<\\1>/<CR>", "|<a>b"),
        ("|ab", ":s/a/\\9/<CR>", "|b"),
        ("|ab", ":s/a/&\\n/<CR>", "|anb"),
        ("|aXa", ":s/x/y/g<CR>", "|aya"),
        ("|aXa", ":s/X/y/g<CR>", "|aya"),
        ("|AxA", ":s/a/y/g<CR>", "|yxy"),
        ("|AxA", ":s/A/y/g<CR>", "|yxy"),
        ("|axA", ":s/A/y/g<CR>", "|axy"),
    ]);
}

#[test]
fn vi_7_s_says_how_much_it_did() {
    assert_eq!(says("|a", ":s/a/b/<CR>"), "1 substitution on 1 line");
    assert_eq!(says("|aa", ":s/a/b/g<CR>"), "2 substitutions on 1 line");
    assert_eq!(says("|a a\nb\na", ":%s/a/x/g<CR>"), "3 substitutions on 2 lines");
    assert_eq!(says("|a\na", ":%s/a/x/<CR>"), "2 substitutions on 2 lines");
    assert_eq!(says("|a", ":s/z/b/<CR>"), "not found: z");
    assert!(says("|a", ":s/(/b/<CR>").starts_with("bad pattern"));
    assert_eq!(says("|a", ":s/a/b/x<CR>"), "unknown flag: x");
    assert_eq!(says("|a", ":s#a#b#<CR>"), "usage: :s/re/rep/[g]");
    assert_eq!(says("|a", ":s<CR>"), "usage: :s/re/rep/[g]");
}

#[test]
fn vi_7_s_is_one_undo_unit() {
    table(&[
        ("|a\na\na", ":%s/a/b/<CR>u", "|a\na\na"),
        ("|a a a", ":s/a/b/g<CR>u", "|a a a"),
        ("|a\na", ":%s/a/b/<CR>u<C-r>", "b\n|b"),
        ("|a\na", ":s/a/b/<CR>j:s/a/b/<CR>u", "b\n|a"),
    ]);
    let mut a = typed("|a b", ":s/a/x/<CR>");
    assert!(a.buf.dirty());
    feed(&mut a, "u");
    assert!(!a.buf.dirty());
}

#[test]
fn vi_7_an_empty_pattern_uses_the_last_search() {
    table(&[("|x a", "/a<CR>:s//y/<CR>", "|x y")]);
}

#[test]
fn vi_7_s_in_a_read_only_buffer_is_refused() {
    let mut a = app_with(Some("f"), &[b'a', 0xff], (70, 6));
    feed(&mut a, ":s/a/b/\n");
    assert!(msg(&mut a).starts_with("read-only: "));
    assert!(!a.buf.dirty());
}

#[test]
fn vi_7_filter_and_ref_take_ranges_like_the_other_commands() {
    for (keys, cmd, input) in [
        (":!sort\n", "sort", "a\n"),
        (":%!sort\n", "sort", "a\nb\nc\n"),
        (":1,2!sort\n", "sort", "a\nb\n"),
        (":.,$!tr a b\n", "tr a b", "a\nb\nc\n"),
        (":3,2!cat\n", "cat", "b\nc\n"),
    ] {
        let mut a = named("a\nb\nc\n");
        let fx = feed(&mut a, keys);
        let spawn = fx.iter().find_map(|e| match e {
            Effect::Spawn { cmd, input } => Some((cmd.as_str(), input.as_slice())),
            _ => None,
        });
        assert_eq!(spawn, Some((cmd, input.as_bytes())), "{keys}");
        assert_eq!(text(&a), "a\nb\nc", "the filter changes nothing by itself");
    }
    for (keys, said) in [
        (":ref\n", "copied f:1-1"),
        (":%ref\n", "copied f:1-3"),
        (":2,3ref\n", "copied f:2-3"),
        ("Vj<Esc>:'<,'>ref\n", "copied f:1-2"),
        ("j:.ref\n", "copied f:2-2"),
    ] {
        let mut a = named("a\nb\nc\n");
        feed(&mut a, keys);
        assert_eq!(msg(&mut a), said, "{keys}");
    }
    assert_eq!(says("|a", ":nonsense<CR>"), "not an editor command: nonsense");
    assert_eq!(says("|a", ":5nonsense<CR>"), "not an editor command: nonsense");
    assert_eq!(says("|a", ":s<CR>"), "usage: :s/re/rep/[g]");
}

#[test]
fn vi_7_e_opens_a_file_unless_the_buffer_is_modified() {
    let mut a = named("abc\n");
    assert_eq!(open_of(&feed(&mut a, ":e other.txt\n")), Some("other.txt"));
    assert_eq!(open_of(&feed(&mut a, ":e\n")), Some("f"), "without a name: this file again");
    assert_eq!(open_of(&feed(&mut a, ":e!\n")), Some("f"));
    feed(&mut a, "x");
    let fx = feed(&mut a, ":e other.txt\n");
    assert_eq!(open_of(&fx), None);
    assert_eq!(msg(&mut a), "no write since last change (:e! to discard)");
    assert_eq!(
        open_of(&feed(&mut a, ":e!\n")),
        Some("f"),
        ":e! reloads this file and discards the changes"
    );
    assert_eq!(open_of(&feed(&mut a, ":e! other.txt\n")), Some("other.txt"));
    let mut a = app("abc\n");
    assert_eq!(open_of(&feed(&mut a, ":e!\n")), None);
    assert_eq!(msg(&mut a), "no file name");
}

#[test]
fn vi_7_a_file_is_not_loaded_over_edits_made_while_it_was_read() {
    let load = |a: &mut App| a.update(Event::Opened("g".into(), Ok(Some(b"new\n".to_vec())), 0));
    // `:e` on a clean buffer, then a key lands before the file arrives.
    let mut a = named("abc\n");
    feed(&mut a, ":e g\n");
    feed(&mut a, "x");
    load(&mut a);
    assert_eq!((text(&a).as_str(), a.path.as_deref(), a.buf.dirty()), ("bc", Some("f"), true));
    assert_eq!(msg(&mut a), "not opened: edited meanwhile");
    // `:e!` discards what was there when it ran, but not what came after.
    let mut a = named("abc\n");
    feed(&mut a, "x:e! g\n");
    feed(&mut a, "x");
    load(&mut a);
    assert_eq!((text(&a).as_str(), a.path.as_deref()), ("c", Some("f")));
    // Nothing happened meanwhile: the file is loaded, also after `:e!` of a modified buffer.
    for keys in [":e g\n", "x:e! g\n"] {
        let mut a = named("abc\n");
        feed(&mut a, keys);
        load(&mut a);
        assert_eq!(
            (text(&a).as_str(), a.path.as_deref(), a.buf.dirty()),
            ("new", Some("g"), false)
        );
    }
    // An edit that was undone leaves the text as it was: nothing to lose.
    let mut a = named("abc\n");
    feed(&mut a, ":e g\nxu");
    load(&mut a);
    assert_eq!(a.path.as_deref(), Some("g"));
    // The refusal is for that one load: a later `:e` works.
    let mut a = named("abc\n");
    feed(&mut a, ":e g\nx");
    load(&mut a);
    feed(&mut a, ":e! g\n");
    load(&mut a);
    assert_eq!((text(&a).as_str(), a.path.as_deref()), ("new", Some("g")));
}

#[test]
fn vi_7_a_newly_opened_file_starts_fresh() {
    let mut a = named("abc\ndef\n");
    feed(&mut a, "jlxvl<Esc>");
    feed(&mut a, "/e<CR>:e! g<CR>");
    a.update(Event::Opened("g".into(), Ok(Some(b"one\ntwo\nthree\n".to_vec())), 0));
    assert_eq!((a.path.as_deref(), text(&a).as_str()), (Some("g"), "one\ntwo\nthree"));
    assert_eq!((a.buf.cursor, a.top, a.buf.dirty()), (0, 0, false));
    feed(&mut a, "u");
    assert_eq!(msg(&mut a), "nothing to undo", "the undo history is gone");
    assert_eq!(msg(&mut a), "nothing to undo");
    // The disk hash is the new file's: a save expects exactly that.
    let fx = feed(&mut a, "x:w\n");
    let w = fx.iter().find_map(|e| match e {
        Effect::Write { path, expect_hash, .. } => Some((path.as_str(), *expect_hash)),
        _ => None,
    });
    assert_eq!(w, Some(("g", Some(si::core::buffer::hash(b"one\ntwo\nthree\n")))));
    // Marks restart too: `'<` is line 1 again.
    let mut a = named("a\nb\nc\n");
    feed(&mut a, "jjV<Esc>:e! g<CR>");
    a.update(Event::Opened("g".into(), Ok(Some(b"x\ny\nz\n".to_vec())), 0));
    feed(&mut a, ":'<,'>s/./Q/\n");
    assert_eq!(text(&a), "Q\ny\nz");
}

#[test]
fn vi_7_opening_a_missing_or_unreadable_file_works_like_at_startup() {
    let mut a = named("abc\n");
    feed(&mut a, ":e new.txt<CR>");
    a.update(Event::Opened("new.txt".into(), Ok(None), 0));
    assert_eq!(
        (a.path.as_deref(), text(&a).as_str(), a.buf.readonly.clone()),
        (Some("new.txt"), "", None)
    );
    let fx = feed(&mut a, "ihi<Esc>:w\n");
    assert!(fx.iter().any(
        |e| matches!(e, Effect::Write { path, expect_hash: Some(0), .. } if path == "new.txt")
    ));
    let mut a = named("abc\n");
    feed(&mut a, ":e d<CR>");
    a.update(Event::Opened("d".into(), Err("d is a directory".into()), 0));
    assert_eq!(a.buf.readonly.as_deref(), Some("read-only: d is a directory"));
}

#[test]
fn vi_7_the_startup_command_goes_through_the_same_path() {
    let mut a = named("abc\nfoo\nbar\nfoo\n");
    let fx = a.start("/foo\n");
    assert!(matches!(&fx[0], Effect::Render(f) if status(f) == "f  1:1  NORMAL"));
    assert_eq!((a.buf.line_of(a.buf.cursor), a.buf.col()), (1, 0));
    let mut a = named("a\nb\nc\n");
    a.start(":3\n");
    assert_eq!(a.buf.line_of(a.buf.cursor), 2);
    let mut a = named("a a\nb\n");
    a.start(":%s/a/x/g\n");
    assert_eq!(text(&a), "x x\nb");
    let mut a = named("a\n");
    assert_eq!(quit(&a.start(":q\n")), Some(0));
}

#[test]
fn vi_7_w_q_wq_x_and_cq_are_still_there() {
    let mut a = named("a\n");
    assert_eq!(quit(&feed(&mut a, ":cq\n")), Some(1));
    assert_eq!(quit(&feed(&mut named("a\n"), ":q\n")), Some(0));
    assert_eq!(quit(&feed(&mut named("a\n"), "x:q!\n")), Some(0));
    assert_eq!(quit(&feed(&mut named("a\n"), "x:q\n")), None);
    let fx = feed(&mut named("a\n"), "x:wq!\n");
    assert!(fx.iter().any(|e| matches!(e, Effect::Write { expect_hash: None, .. })), ":wq! forces");
}

/// `:w` is refused while the file is on its way (and `:e` while a save runs), so no save is running
/// when the file arrives. An answer that comes anyway (from a thread that outlived its buffer)
/// must not touch the new buffer: it would read as modified, with the wrong text as its base.
#[test]
fn vi_7_a_late_saved_does_not_touch_a_reloaded_buffer() {
    let mut a = named("a\n");
    feed(&mut a, "x:e! g\n"); // `:e!` asked for the file ...
    let fx = feed(&mut a, ":w\n"); // ... and a save is refused until it arrives
    assert!(!fx.iter().any(|e| matches!(e, Effect::Write { .. })));
    a.update(Event::Opened("g".into(), Ok(Some(b"g text\n".to_vec())), 0));
    a.update(Event::Saved(Ok((si::core::buffer::hash(b"\n"), None)), 0));
    assert_eq!(
        (a.path.as_deref(), a.buf.dirty()),
        (Some("g"), false),
        "a just-read buffer is clean"
    );
    assert_eq!(a.buf.base.to_string(), "g text");
    assert_eq!(quit(&feed(&mut a, ":q\n")), Some(0), "`:q` works on the unmodified buffer");
    // And the answer of a save that fails is not told about the new buffer either.
    let mut a = named("a\n");
    feed(&mut a, "x:e! g\n");
    a.update(Event::Opened("g".into(), Ok(Some(b"g text\n".to_vec())), 0));
    a.update(Event::Saved(Err("disk full".into()), 0));
    assert_eq!(msg(&mut a), "g  1:1  NORMAL");
}

#[test]
fn vi_7_e_is_refused_while_a_save_runs() {
    // A modified buffer is refused anyway unless it is `:e!`; a clean one is refused for the save.
    for (before, cmd) in
        [("x:w\n", ":e! g\n"), ("x:w\n", ":e!\n"), (":w\n", ":e g\n"), (":w\n", ":e\n")]
    {
        let mut a = named("a\n");
        feed(&mut a, before);
        let fx = feed(&mut a, cmd);
        assert_eq!(
            (open_of(&fx), msg(&mut a).as_str()),
            (None, "a save is already running"),
            "{cmd}"
        );
        // Once it has ended the command works.
        a.update(Event::Saved(Ok((si::core::buffer::hash(b"\n"), None)), 0));
        assert!(open_of(&feed(&mut a, cmd)).is_some(), "{cmd}");
    }
}

#[test]
fn vi_7_w_bang_cmd_is_refused_because_filters_are_range_bang_cmd() {
    for cmd in [":w !cat\n", ":wq !cat\n", ":x !cat\n", ":w! !cat\n", ":wq! !cat\n"] {
        let mut a = named("a\n");
        feed(&mut a, "x");
        let fx = feed(&mut a, cmd);
        let acted = fx.iter().any(|e| matches!(e, Effect::Write { .. } | Effect::Quit(_)));
        assert!(!acted, "{cmd}: no file named !cat, no filter");
        assert_eq!(msg(&mut a), "filters are :{range}!cmd", "{cmd}");
    }
}

/// The other file is written, but the buffer's own file still differs, so nothing quits.
#[test]
fn vi_7_wq_and_x_with_another_name_do_not_quit_while_the_own_file_differs() {
    for cmd in [":wq other\n", ":x other\n", ":wq! other\n"] {
        let mut a = named("a\n");
        feed(&mut a, "x");
        let fx = feed(&mut a, cmd);
        assert!(fx.iter().any(|e| matches!(e, Effect::Write { path, .. } if path == "other")));
        let fx = a.update(Event::Saved(Ok((si::core::buffer::hash(b"\n"), None)), 0));
        assert_eq!(quit(&fx), None, "{cmd}");
        assert_eq!(msg(&mut a), "saved other; not quitting: f has unsaved changes", "{cmd}");
        assert!(a.buf.dirty() && a.path.as_deref() == Some("f"));
    }
    // With nothing unsaved in the own file, writing a copy and quitting is fine.
    let mut a = named("a\n");
    feed(&mut a, ":wq other\n");
    assert_eq!(quit(&a.update(Event::Saved(Ok((1, None)), 0))), Some(0));
    // An unnamed buffer takes the name, which is its own file then.
    let mut a = app("a\n");
    feed(&mut a, "x:wq new\n");
    assert_eq!(quit(&a.update(Event::Saved(Ok((1, None)), 0))), Some(0));
}

#[test]
fn vi_7_a_range_that_does_not_parse_is_an_error() {
    for line in [":,$s/./X/", ":.+1", ":3,", ":'a", ":1;2", ":,$!sort", ":3,x", ":%%", ":12 34"] {
        let mut a = named("a\nb\nc\n");
        feed(&mut a, &format!("{line}\n"));
        assert_eq!((text(&a).as_str(), a.buf.cursor), ("a\nb\nc", 0), "{line}");
        assert!(msg(&mut a).starts_with("bad range: "), "{line}: {}", msg(&mut a));
    }
    // The ranges that do parse are as before.
    table(&[
        ("|a\nb\nc", ":2,3s/./X/<CR>", "a\nX\n|X"),
        ("|a\nb\nc", ":$<CR>", "a\nb\n|c"),
        ("|a\nb\nc", ":%s/./X/<CR>", "X\nX\n|X"),
        ("|a\nb\nc", ":<CR>", "|a\nb\nc"),
        ("|a\nb\nc", ": <CR>", "|a\nb\nc"),
    ]);
}
