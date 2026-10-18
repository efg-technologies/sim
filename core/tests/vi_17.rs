//! VI-17 (marks). Rows are (before, keys, after); `|` is the cursor; Neovim gave the `after` of the rows
//! that came from wip/vim, and `core/tests/oracle.rs` (`SIM_ORACLE_MODE=mark`) checks the rest.
mod common;
use common::*;
use sim_core::core::app::Event;

#[test]
fn vi_17_a_mark_is_a_place_and_a_line() {
    table(&[
        ("|one\ntwo\nthree\nfour", "jlmajjdd'a", "one\n|two\nthree"),
        ("one\n|two\nthree\nfour", "majj`a", "one\n|two\nthree\nfour"),
        ("one\n|two\nthree\nfour", "majjd'a", "|one"),
        ("one\n|two\nthree\nfour", "ma$jj`ax", "one\n|wo\nthree\nfour"),
        ("one\n|two three\nfour", "wmawj`aD", "one\ntwo| \nfour"),
        ("one\n|two three\nfour", "wmab`ad`a", "one\ntwo |three\nfour"),
        ("one tw|o three\nfour", "mab`ad`a", "one tw|o three\nfour"),
        ("one tw|o three\nfour", "mabd`a", "one |o three\nfour"),
        ("|ab cd\nef gh\nij", "jmad'a", "ab cd\n|ij"),
        ("|ab cd\nef gh\nij", "jwmak0d`a", "|gh\nij"),
        ("|ab cd\nef gh\nij", "jwma0d`a", "ab cd\n|gh\nij"),
        ("|ab cd\nef gh\nij", "jwmaggd`a", "|gh\nij"),
        ("|ab cd\nef gh\nij", "jwmaggy'ap", "ab cd\n|ab cd\nef gh\nef gh\nij"),
        ("|ab cd\nef gh\nij", "jwmaggd'a", "|ij"),
        ("|ab cd\nef gh\nij", "jwmaggc'aQ<Esc>", "|Q\nij"),
        ("|ab cd\nef gh\nij", "jwmagg>'a", "    |ab cd\n    ef gh\nij"),
        ("|ab cd\nef gh\nij", "jwmagg:.,'as/^/X/<CR>", "Xab cd\n|Xef gh\nij"),
        ("|ab cd\nef gh\nij", "jma:'a,$s/^/X/<CR>", "ab cd\nXef gh\n|Xij"),
    ]);
}

#[test]
fn vi_17_marks_move_with_the_edits() {
    table(&[
        ("|a\nb\nc\nd\ne", "jjmakOnew<Esc>gg`ax", "a\nnew\nb\n|\nd\ne"),
        ("|a\nb\nc\nd\ne", "jjmaggOnew<Esc>G`ax", "new\na\nb\n|\nd\ne"),
        ("|a\nb\nc\nd\ne", "jjmaggdd`ax", "b\n|\nd\ne"),
        ("|a\nb\nc\nd\ne", "jjmaggddu`ax", "a\nb\n|\nd\ne"),
        ("|a\nb\nc\nd\ne", "jjmakdd`ax", "a\n|\nd\ne"),
        ("|a\nb\nc\nd\ne", "jjmakddgg`ax", "a\n|\nd\ne"),
        ("|a\nb\nc\nd\ne", "jjmakdd'ax", "a\n|\nd\ne"),
        ("|a\nb\nc\nd\ne", "jjmajddgg'ax", "a\nb\n|\ne"),
        ("|a\nb\nc\nd\ne", "jjmakddugg'ax", "a\nb\n|\nd\ne"),
        ("|a\nb\nc\nd\ne", "jjmaggyyjp'ax", "a\nb\na\n|\nd\ne"),
        ("|a\nb\nc\nd\ne", "jjmaggyyPG'ax", "a\na\nb\n|\nd\ne"),
        ("|a\nb\nc\nd\ne", "jjmaGJgg'ax", "a\nb\n|\nd\ne"),
        ("|a\nb\nc\nd\ne", "jjmakJgg'ax", "a\n| c\nd\ne"),
        ("|abc d\nxyz", "wmaggJ`ax", "abc | xyz"),
        ("|abc d\nxyz", "jwmaggJ`ax", "abc d x|y"),
        ("|abc d\nxyz", "jmaggJ`ax", "abc d |yz"),
        ("|foo\n\n", "jmakJ`ax", "f|o"),
        ("|a\nb\nc\nd", "jmb/c\nggdn'bx", "|\nd"),
        ("|a\nb\nc\nd", "jjmb/d\nggdn'bx", "|"),
        ("|a\nb\nc\nd", "jjmbggdj'bx", "|\nd"),
        ("|abc d\nxyz", "jmaggdd`ax", "|yz"),
    ]);
}

#[test]
fn vi_17_the_previous_place_is_where_the_last_jump_left() {
    table(&[
        ("|a\nb c\nc d\nd\ne", "Gd`", "a\nb c\nc d\nd\n|e"),
        ("|a\nb c\nc d\nd\ne", "Gj``x", "|\nb c\nc d\nd\ne"),
        ("|a\nb c\nc d\nd\ne", "jjmaggd`a", "|c d\nd\ne"),
        ("|a\nb c\nc d\nd\ne", "jw%``x", "a\nb| \nc d\nd\ne"),
        ("|a\nb c\nc d\nd\ne", "jw*``x", "a\nb| \nc d\nd\ne"),
        ("|a\nb c\nc d\nd\ne", "jw/d<CR>``x", "a\nb| \nc d\nd\ne"),
        ("|a\nb c\nc d\nd\ne", "jwf*``x", "a\nb| \nc d\nd\ne"),
        ("|a\nb c\nc d\nd\ne", "jwwG````x", "a\nb c\nc d\nd\n|"),
        ("|a\nb c\nc d\nd\ne", "jwwG``x", "a\nb c\n| d\nd\ne"),
        ("|a\nb c\nc d\nd\ne", "jwwjd``", "a\nb c\nc d\n|d\ne"),
        ("|a\nb\nc\nd\ne", "''x", "|\nb\nc\nd\ne"),
        ("|a\nb\nc\nd\ne", "'ax", "|\nb\nc\nd\ne"),
        ("|a\nb\nc\nd\ne", "/c<CR>``x", "|\nb\nc\nd\ne"),
        ("|a\nb\nc\nd\ne", "3Gggx``x", "\nb\n|\nd\ne"),
        ("|a\nb\nc\nd\ne", ":3<CR>''''x", "a\nb\n|\nd\ne"),
        ("|a\nb\nc\nd\ne", ":3<CR>``x", "a\nb\n|\nd\ne"),
        ("|a\nb\nc\nd\ne", "Gggx``x", "\nb\nc\nd\n|"),
        ("|a\nb\nc\nd\ne", "`ax", "|\nb\nc\nd\ne"),
        ("|a\nb\nc\nd\ne", "jjG''''x", "a\nb\nc\nd\n|"),
        ("|a\nb\nc\nd\ne", "jjG''x", "a\nb\n|\nd\ne"),
        ("|a\nb\nc\nd\ne", "jjG``x", "a\nb\n|\nd\ne"),
        ("|a\nb\nc\nd\ne", "jjGx''x", "a\nb\n|\nd\n"),
        ("|a\nb\nc\nd\ne", "jjmaG'a``x", "a\nb\nc\nd\n|"),
        ("|a\nb\nc\nd\ne", "jjmaG'ax", "a\nb\n|\nd\ne"),
        ("|a\nb\nc\nd\ne", "jjmaG``x", "a\nb\n|\nd\ne"),
    ]);
}

#[test]
fn vi_17_a_mark_that_is_not_set_says_so_and_does_nothing() {
    for keys in ["'a", "`a", "d'a", "y`a", "v'a", "d`b", "''", "d``"] {
        let mut a = typed("one\n|two\nthree", "");
        feed(&mut a, keys);
        assert_eq!(marked(&a), "one\n|two\nthree", "{keys}");
        assert!(status(&screen(&mut a)).starts_with("mark not set: "), "{keys}");
    }
}

#[test]
fn vi_17_only_a_to_z_are_marks_here() {
    for keys in ["mA", "m1", "m.", "'A", "`1", "'<", "`>"] {
        let mut a = typed("one\n|two\nthree", "");
        feed(&mut a, keys);
        assert_eq!(marked(&a), "one\n|two\nthree", "{keys}");
        assert!(status(&screen(&mut a)).starts_with("marks: a-z only"), "{keys}");
    }
    let mut a = app("ab");
    feed(&mut a, "m<Esc>x");
    assert_eq!(text(&a), "b", "Esc cancels the half-typed command");
}

#[test]
fn vi_17_marks_work_in_visual_mode_and_with_the_other_operators() {
    table(&[
        ("|ab\ncd\nef", "jmaggv'ad", "|d\nef"),
        ("|ab cd\nef gh", "wjmaggv`ad", "|h"),
        ("|ab\ncd\nef", "jvma<Esc>gg`ax", "ab\n|d\nef"),
        ("|ab\ncd\nef", "jmaggVj'ay", "|ab\ncd\nef"),
        ("ab\n|cd\nef", "majVy`ax", "ab\n|d\nef"),
    ]);
}

#[test]
fn vi_17_marks_lists_the_marks_on_one_line() {
    let mut a = typed("|one\n  two\nthree\nfour", "");
    feed(&mut a, ":marks\n");
    assert_eq!(status(&screen(&mut a)), "no marks");
    feed(&mut a, "jmaGlmbgg:marks\n");
    assert_eq!(status(&screen(&mut a)), "'a 2:1 two · 'b 4:2 four · '' 4:2 four");
    // A name is set again to move it; the place before a jump is the last jump's.
    feed(&mut a, "jjma:marks\n");
    assert_eq!(status(&screen(&mut a)), "'a 3:1 three · 'b 4:2 four · '' 4:2 four");
}

#[test]
fn vi_17_marks_share_the_line_as_registers_do() {
    let long = "x".repeat(100);
    let mut a = app(&format!("{long}\nshort\n{long}"));
    feed(&mut a, "majmbjmc:marks\n");
    let said = status(&screen(&mut a)).to_string();
    assert!(said.contains("'b 2:1 short · 'c "), "the short one is whole: {said}");
    assert!(said.contains('…') && said.chars().count() <= 80, "{said}");
}

#[test]
fn vi_17_no_text_of_a_mark_reaches_the_terminal_raw() {
    let mut a = app("a\u{1b}[31mb\u{202e}c");
    feed(&mut a, "ma:marks\n");
    let said = status(&screen(&mut a)).to_string();
    assert!(said.contains("^["), "{said}");
    assert!(!said.chars().any(|c| c.is_control()) && !said.contains('\u{202e}'), "{said:?}");
}

#[test]
fn vi_17_marks_follow_their_lines_through_a_change_from_the_disk() {
    let mut a = app_with(Some("f"), b"one\ntwo\nthree\nfour\n", (80, 24));
    feed(&mut a, "majmbjmcjmd"); // a on one, b on two, c on three, d on four
    let disk = |a: &mut sim_core::core::app::App, text: &str| {
        a.update(Event::Disk {
            bytes: Some(text.as_bytes().to_vec()),
            hash: None,
            deadline: far(),
            seq: 0,
        });
    };
    // a line is put above `one`, `two` is taken away and `four` is changed
    disk(&mut a, "zero\none\nthree\nFOUR\n");
    feed(&mut a, ":marks\n");
    assert_eq!(status(&screen(&mut a)), "'a 2:1 one · 'c 3:1 three · 'd 4:1 FOUR");
    // undo of the merge puts back what it took away, and the marks with it
    feed(&mut a, "u:marks\n");
    let said = status(&screen(&mut a)).to_string();
    assert!(said.starts_with("'a 1:1 one · 'b 2:1 two · 'c 3:1 three · 'd 4:1 four"), "{said}");
}

#[test]
fn vi_17_undo_follows_the_lines_of_a_file_of_many_chunks() {
    // (the texts share their chunks, which are found by address: lines, and the same line twice in a row)
    let body: String = (0..30_000).map(|i| format!("line {}\n", i / 2)).collect();
    let mut a = app_with(None, body.as_bytes(), (80, 24));
    let at = |a: &sim_core::core::app::App| a.buf.vmark[0].map(|m| m.0);
    feed(&mut a, "5000Gdd9999Gma25000Gmb");
    assert_eq!(at(&a), Some(9998));
    feed(&mut a, "u");
    assert_eq!(at(&a), Some(9999));
    feed(&mut a, "\x12");
    assert_eq!(at(&a), Some(9998));
    feed(&mut a, "29000Gddu");
    assert_eq!(at(&a), Some(9998));
    feed(&mut a, "ggddu");
    assert_eq!(at(&a), Some(9998));
    feed(&mut a, "Gddu");
    assert_eq!(at(&a), Some(9998));
}

#[test]
fn vi_17_an_edit_that_takes_whole_lines_takes_their_marks_whoever_makes_it() {
    // (a filter's output, or a command that says nothing of which lines it takes: the text tells. The marks
    // of the first line are not asked: the same edit of an empty line is a join, which keeps them.)
    let at = |a: &sim_core::core::app::App, i: usize| a.buf.vmark[i].map(|p| p.0);
    for (range, after) in [
        (0..4, &[(1, None), (2, Some(0)), (3, Some(1))][..]), // "a\nb\n"
        (1..5, &[(1, None), (3, Some(1))][..]),               // "\nb\nc", with the break before it
        (2..6, &[(2, None), (3, Some(1))][..]),               // "b\nc\n"
    ] {
        let mut a = app("a\nb\nc\nd");
        a.buf.vmark[..4].copy_from_slice(&[Some((0, 0)), Some((1, 0)), Some((2, 0)), Some((3, 0))]);
        a.buf.replace(range.clone(), "");
        for &(i, line) in after {
            assert_eq!(at(&a, i), line, "{range:?}: mark {i}");
        }
    }
}
