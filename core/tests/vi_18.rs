//! VI-18 (the comment toggle). Rows are (before, keys, after); `|` is the cursor. The rows of a Rust file
//! are what Neovim 0.12.6 (`-u NONE`, `commentstring` `// %s`) gave; `core/tests/oracle.rs`
//! (`SIM_ORACLE_MODE=comment`) checks more.
#![allow(clippy::unwrap_used)] // the helper
mod common;
use common::*;

/// An App on a file named `path` holding `before` (`|` is the cursor), after typing `keys`.
fn on(path: &str, before: &str, keys: &str) -> sim_core::core::app::App {
    let c = before.find('|').unwrap();
    let t = before.replacen('|', "", 1);
    let mut a = app_with(Some(path), format!("{t}\n").as_bytes(), (80, 24));
    a.buf.cursor = t[..c].chars().count();
    feed(&mut a, keys);
    a
}

fn rows(path: &str, rows: &[(&str, &str, &str)]) {
    for (before, keys, after) in rows {
        assert_eq!(marked(&on(path, before, keys)), *after, "{path}: {before:?} then {keys:?}");
    }
}

#[test]
fn vi_18_gc_comments_at_the_smallest_indent_and_uncomments_when_every_line_is_commented() {
    rows(
        "a.rs",
        &[
            ("    f|oo\n  bar\n\n    baz", "gcc", "    |// foo\n  bar\n\n    baz"),
            ("    foo\n  b|ar\n\n    baz", "gcj", "    foo\n  /|/ bar\n  //\n    baz"),
            ("    fo|o\n  bar\n\n    baz", "gcG", "  //  | foo\n  // bar\n  //\n  //   baz"),
            ("    /|/ foo\n  // bar\n\n    baz", "gcj", "    f|oo\n  bar\n\n    baz"),
            ("    /|/ foo\n  // bar\n    \n  // baz", "gcG", "    f|oo\n  bar\n    \n  baz"),
            ("|a\nb\nc", "2gcc", "|// a\n// b\nc"),
            ("a\nb\n|c", "gck", "a\n|// b\n// c"),
            ("|a\nb\nc", "Vjgc", "|// a\n// b\nc"),
            ("|a\nb\nc", "gcc.", "|a\nb\nc"),
            ("|a\nb\nc", "gcgc", "|// a\nb\nc"),
            ("|a\nb\nc", "gcc", "|// a\nb\nc"),
            // a line that only looks commented with the parts trimmed is uncommented by them
            ("|//a\n  //b", "gcj", "|a\n  b"),
            // a commented blank line keeps what follows the leader's own blank, and no indent (Neovim's)
            ("|// a\n  //  \n// b", "gcG", "|a\n \nb"),
        ],
    );
}

#[test]
fn vi_18_the_comment_string_is_the_file_name_s() {
    rows("a.py", &[("|x = 1", "gcc", "|# x = 1")]);
    rows("Makefile", &[("|all:", "gcc", "|# all:")]);
    rows(".vimrc", &[("|set nu", "gcc", "|\"set nu")]);
    rows("a.lua", &[("|x()", "gcc", "|-- x()")]);
    rows("a.el", &[("|(x)", "gcc", "|; (x)")]);
    rows("a.tex", &[("|x", "gcc", "|% x")]);
    rows(
        "a.css",
        &[("|a {}\nb {}", "gcj", "|/* a {} */\n/* b {} */"), ("|/* a {} */", "gcc", "|a {}")],
    );
    rows("a.html", &[("  |<p>\n", "gcc", "  |<!-- <p> -->\n"), ("|<!-- x -->", "gcc", "|x")]);
    rows("x/y.md", &[("|# t", "gcc", "|<!-- # t -->")]);
}

#[test]
fn vi_18_a_file_with_no_comment_string_says_so_and_nothing_changes() {
    for path in ["a.json", "README", "a.unknown"] {
        let mut a = on(path, "|x", "gcc");
        assert_eq!(marked(&a), "|x");
        assert_eq!(a.msg.as_deref(), Some(format!("no comment string for {path}").as_str()));
        assert!(!a.buf.dirty());
        assert_eq!(status(&screen(&mut a)), format!("no comment string for {path}"));
    }
    let mut a = typed("|x", "gcc");
    assert_eq!(
        (marked(&a), a.msg.as_deref()),
        ("|x".into(), Some("no comment string for [no name]"))
    );
    feed(&mut a, "u");
    assert_eq!(a.msg.as_deref(), Some("nothing to undo"));
}

#[test]
fn vi_18_one_toggle_is_one_undo_step_and_the_registers_and_marks_stay() {
    let mut a = on("a.rs", "|a\nb\nc\nd", "yyjmagcGu");
    assert_eq!(marked(&a), "a\n|b\nc\nd");
    feed(&mut a, "gcG");
    assert_eq!(text(&a), "a\n// b\n// c\n// d");
    // the yank is still what `p` puts (VI-15: the toggle fills no register), and `'a` is where it was (VI-17)
    feed(&mut a, "'aP");
    assert_eq!(text(&a), "a\na\n// b\n// c\n// d");
    feed(&mut a, "uu");
    assert_eq!(text(&a), "a\nb\nc\nd");
    assert!(!a.buf.dirty());
}

#[test]
fn vi_18_a_read_only_buffer_is_not_commented() {
    let mut a =
        sim_core::core::app::App::new(Some("a.rs".into()), Err("no access".into()), (80, 24));
    feed(&mut a, "gcc");
    assert_eq!(a.msg.as_deref(), Some("read-only: no access"));
}
