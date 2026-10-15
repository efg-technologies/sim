//! VI-7: ex commands and their ranges. Rows are (before, keys, after); `|` is the cursor.
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
fn vi_7_commands_that_come_later_are_parsed_now() {
    for keys in [
        ":!sort\n",
        ":%!sort\n",
        ":1,2!sort\n",
        ":.,$!tr a b\n",
        ":seen\n",
        ":ref\n",
        ":%ref\n",
        ":'<,'>ref\n",
        ":.ref\n",
    ] {
        let mut a = typed("|a\nb\nc", keys);
        assert_eq!(msg(&mut a), "not available yet", "{keys}");
        assert_eq!(text(&a), "a\nb\nc");
    }
    let mut a = typed("|a\nb\nc", "Vj<Esc>:'<,'>ref\n");
    assert_eq!(msg(&mut a), "not available yet");
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
fn vi_7_a_newly_opened_file_starts_fresh() {
    let mut a = named("abc\ndef\n");
    feed(&mut a, "jlxvl<Esc>");
    feed(&mut a, "/e<CR>");
    a.update(Event::Opened("g".into(), Ok(Some(b"one\ntwo\nthree\n".to_vec()))));
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
    feed(&mut a, "jjV<Esc>");
    a.update(Event::Opened("g".into(), Ok(Some(b"x\ny\nz\n".to_vec()))));
    feed(&mut a, ":'<,'>s/./Q/\n");
    assert_eq!(text(&a), "Q\ny\nz");
}

#[test]
fn vi_7_opening_a_missing_or_unreadable_file_works_like_at_startup() {
    let mut a = named("abc\n");
    a.update(Event::Opened("new.txt".into(), Ok(None)));
    assert_eq!(
        (a.path.as_deref(), text(&a).as_str(), a.buf.readonly.clone()),
        (Some("new.txt"), "", None)
    );
    let fx = feed(&mut a, "ihi<Esc>:w\n");
    assert!(fx.iter().any(
        |e| matches!(e, Effect::Write { path, expect_hash: Some(0), .. } if path == "new.txt")
    ));
    let mut a = named("abc\n");
    a.update(Event::Opened("d".into(), Err("Is a directory".into())));
    assert_eq!(a.buf.readonly.as_deref(), Some("read-only: cannot read: Is a directory"));
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
