//! VI-2 (editing), VI-8 (the one register and the clipboard), and `.` and undo units.
//! Rows are (before, keys, after); `|` is the cursor.
mod common;
use common::*;
use si::core::app::{Effect, Event};

fn clipboard(fx: &[Effect]) -> Vec<&str> {
    fx.iter()
        .filter_map(|e| if let Effect::Clipboard(t) = e { Some(t.as_str()) } else { None })
        .collect()
}

#[test]
fn vi_2_x_r_and_j() {
    table(&[
        ("a|bc", "x", "a|c"),
        ("ab|c", "x", "a|b"),
        ("|abcd", "2x", "|cd"),
        ("a|bcd", "9x", "|a"),
        ("|", "x", "|"),
        ("|ab\ncd", "3x", "|\ncd"),
        ("|abc", "rx", "|xbc"),
        ("|abc", "3rx", "xx|x"),
        ("|abc", "4rx", "|abc"),
        ("a|bc", "2rx", "ax|x"),
        ("|a\nb", "J", "a| b"),
        ("|a\n  \tb", "J", "a| b"),
        ("|a\n\nb", "J", "|a\nb"),
        ("|a \nb", "J", "a |b"),
        ("|\nb", "J", "|b"),
        ("|a\nb\nc\nd", "3J", "a b| c\nd"),
        ("|a\nb", "9J", "a| b"),
        ("a\n|b", "J", "a\n|b"),
    ]);
}

#[test]
fn vi_2_p_and_p_with_the_register() {
    table(&[
        ("|abc", "xp", "b|ac"),
        ("|abc", "xP", "|abc"),
        ("|ab", "x3p", "baa|a"),
        ("|a\nb", "yyp", "a\n|a\nb"),
        ("a\n|b", "yyP", "a\n|b\nb"),
        ("|a\nb", "yy2p", "a\n|a\na\nb"),
        ("a\n|b", "yyp", "a\nb\n|b"),
        ("  |a", "yyp", "  a\n  |a"),
        ("|a\nb", "yyjp", "a\nb\n|a"),
        ("|a b", "dwp", "ba| "),
        ("a|bc", "Yp", "abb|cc"),
        ("|ab\ncd", "ddp", "cd\n|ab"),
        ("|ab", "p", "|ab"),
    ]);
}

#[test]
fn vi_2_undo_redo_and_u_for_redo() {
    table(&[
        ("|abc", "xu", "|abc"),
        ("|abc", "xu<C-r>", "|bc"),
        ("|abc", "xuU", "|bc"),
        ("|abc", "xxuu", "|abc"),
        ("|abc", "xxuu<C-r><C-r>", "|c"),
        ("|abc", "u", "|abc"),
        ("|abc", "<C-r>", "|abc"),
        ("|abc", "xux<C-r>", "|bc"),
        ("|abc", "ia<Esc>uib<Esc>", "|babc"),
    ]);
    let mut a = typed("|abc", "u");
    assert_eq!(status(&screen(&mut a)), "nothing to undo");
    feed(&mut a, "<C-r>");
    assert_eq!(status(&screen(&mut a)), "nothing to redo");
}

#[test]
fn vi_2_a_change_clears_the_redo_stack() {
    let mut a = typed("|abc", "xu");
    feed(&mut a, "iX<Esc><C-r>");
    assert_eq!(status(&screen(&mut a)), "nothing to redo");
    assert_eq!(text(&a), "Xabc");
}

#[test]
fn vi_2_i_a_i_a_o_o() {
    table(&[
        ("a|b", "iX<Esc>", "a|Xb"),
        ("a|b", "aX<Esc>", "ab|X"),
        ("|", "aX<Esc>", "|X"),
        ("  a|b", "IX<Esc>", "  |Xab"),
        ("|ab", "AX<Esc>", "ab|X"),
        ("  a|b\nc", "ox<Esc>", "  ab\n  |x\nc"),
        ("c\n  a|b", "Ox<Esc>", "c\n  |x\n  ab"),
        ("a|b", "ox<CR>y<Esc>", "ab\nx\n|y"),
        ("\tab|c", "ox<Esc>", "\tabc\n\t|x"),
    ]);
}

#[test]
fn vi_2_dd_cc_yy_d_c_y() {
    table(&[
        ("a\n|b\nc", "dd", "a\n|c"),
        ("a\n|b", "dd", "|a"),
        ("|a\nb\nc", "2dd", "|c"),
        ("|a", "dd", "|"),
        ("|a\nb", "9dd", "|"),
        ("  a|b\nc", "ccX<Esc>", "  |X\nc"),
        ("a|b\nc\nd", "2ccX<Esc>", "|X\nd"),
        ("a|bc\nd", "D", "|a\nd"),
        ("a|bc\nde\nf", "2D", "|a\nf"),
        ("a|bc\nd", "CX<Esc>", "a|X\nd"),
        ("|a\nb", "yyjp", "a\nb\n|a"),
        ("a|bc", "Yp", "abb|cc"),
        ("a|bc", "yyP", "|abc\nabc"),
        ("a|b", "yl", "a|b"),
        ("a|bc", "yh", "|abc"),
    ]);
}

#[test]
fn vi_2_shift_lines_by_the_indent_unit() {
    table(&[
        ("|a\nb", ">>", "    |a\nb"),
        ("|a\nb", "2>>", "    |a\n    b"),
        ("|a\n\nb", "3>>", "    |a\n\n    b"),
        ("    |a", "<<", "|a"),
        ("    x\n      |a", "<<", "    x\n  |a"),
        ("  |a", "<<", "|a"),
        ("|a", "<<", "|a"),
        // Tab-indented file: the unit is a tab, and `<<` takes a tab away.
        ("\tx\n|a", ">>", "\tx\n\t|a"),
        ("\tx\n\t|a", "<<", "\tx\n|a"),
        // Two-space file.
        ("  x\n|a", ">>", "  x\n  |a"),
        ("  x\n    |a", "<<", "  x\n  |a"),
        // The first indented line decides, even when it holds more spaces.
        ("      x\n|a", ">>", "      x\n      |a"),
        // Blank-only lines do not decide.
        ("   \n\tx\n|a", ">>", "   \n\tx\n\t|a"),
        ("\tx\n    |a", "<<", "\tx\n|a"),
    ]);
}

#[test]
fn vi_8_every_yank_goes_to_the_clipboard_and_nothing_else_does() {
    for (keys, want) in [
        ("yw", "foo "),
        ("yy", "foo bar\n"),
        ("Y", "foo bar"),
        ("y$", "foo bar"),
        ("2yy", "foo bar\nbaz\n"),
        ("yj", "foo bar\nbaz\n"),
        ("vly", "fo"),
        ("Vy", "foo bar\n"),
        ("viwy", "foo"),
    ] {
        let mut a = app("foo bar\nbaz");
        let fx = feed(&mut a, keys);
        assert_eq!(clipboard(&fx), [want], "{keys}");
    }
    for keys in [
        "x", "dd", "dw", "D", "cwx<Esc>", "ccx<Esc>", "p", "P", "J", "vd", "Vd", "vc<Esc>", "u",
        ".",
    ] {
        let mut a = app("foo bar\nbaz");
        let fx = feed(&mut a, keys);
        assert!(clipboard(&fx).is_empty(), "{keys} must not touch the clipboard");
    }
}

#[test]
fn vi_8_a_yank_says_what_it_took() {
    for (text_, keys, want) in [
        ("foo bar\nbaz", "yy", "1 line yanked"),
        ("foo bar\nbaz", "2yy", "2 lines yanked"),
        ("a\nb\nc\nd", "3yy", "3 lines yanked"),
        ("foo bar\nbaz", "yj", "2 lines yanked"),
        ("foo bar\nbaz", "Vy", "1 line yanked"),
        ("foo bar\nbaz", "Vjy", "2 lines yanked"),
        ("foo bar\nbaz", "yl", "1 char yanked"),
        ("foo bar\nbaz", "yw", "4 chars yanked"),
        ("foo bar\nbaz", "Y", "7 chars yanked"),
        ("foo bar\nbaz", "y$", "7 chars yanked"),
        ("foo bar\nbaz", "vly", "2 chars yanked"),
        ("foo bar\nbaz", "v", "1 char yanked"),
        ("hello world 12", "y$", "14 chars yanked"),
        ("日本語", "yl", "1 char yanked"),
        ("日本語", "y$", "3 chars yanked"),
    ] {
        let mut a = app(text_);
        // (`v` alone selects a char; yanking it is `vy`.)
        feed(&mut a, if keys == "v" { "vy" } else { keys });
        assert_eq!(status(&screen(&mut a)), want, "{text_:?} {keys}");
    }
    // The message is for the next key only, and a yank of nothing says nothing.
    let mut a = app("a b\nc");
    feed(&mut a, "yw");
    assert_eq!(status(&screen(&mut a)), "2 chars yanked");
    feed(&mut a, "l");
    assert_eq!(status(&screen(&mut a)), "[no name]  1:2  NORMAL");
    let mut a = app("");
    feed(&mut a, "yy");
    assert_eq!(status(&screen(&mut a)), "1 line yanked", "an empty line is still a line");
    feed(&mut a, "yl");
    assert_eq!(status(&screen(&mut a)), "[no name]  1:1  NORMAL", "no char, no message");
}

#[test]
fn vi_8_there_is_one_register_and_deletes_fill_it() {
    table(&[
        ("|ab cd", "dwp", "cab| d"),
        ("|ab cd", "ywwdwP", "abc|d "),
        ("|a\nb", "ddp", "b\n|a"),
        ("|a b", "ywxP", "|a b"),
    ]);
    let mut a = app("abc");
    feed(&mut a, "\"");
    assert_eq!(status(&screen(&mut a)), "registers: there is only one");
    assert_eq!(text(&a), "abc");
}

#[test]
fn vi_8_a_yank_leaves_the_text_alone_and_a_delete_the_clipboard_alone() {
    let mut a = app("one\ntwo");
    let fx = feed(&mut a, "yyjddp");
    assert_eq!(clipboard(&fx), ["one\n"]);
    assert_eq!(text(&a), "one\ntwo", "dd then p put the deleted line back");
}

#[test]
fn vi_2_dot_repeats_the_last_change() {
    table(&[
        ("|a b c d", "dw.", "|c d"),
        ("|abcd", "x.", "|cd"),
        ("|abcdefgh", "2x.", "|efgh"),
        ("|abcdefgh", "x2.", "|defgh"),
        ("|a b c d e", "dw2.", "|d e"),
        ("|foo bar baz", "cwX<Esc>w.", "X |X baz"),
        ("|ab", "iX<Esc>.", "|XXab"),
        ("|a\nb", "A!<Esc>j.", "a!\nb|!"),
        ("|a", "ox<Esc>.", "a\nx\n|x"),
        ("|a\nb\nc", "dd.", "|c"),
        ("|a", "yyp.", "a\na\n|a"),
        ("|a\nb", ">>j.", "    a\n    |b"),
        ("|abc", "rxl.", "x|xc"),
        ("|a b c d", "dwyw.", "|c d"),
        ("|a b c d", "dwj.", "|c d"),
        ("|a\nb\nc\nd", "J.", "a b| c\nd"),
        ("|abc", ".", "|abc"),
        ("|a b c d", "dw..", "|d"),
        ("|abcdefg", "2x3.", "|fg"),
        // A change that fails stops the replay: its typed text must not run as commands.
        ("|\"a\" x", "ci\"dd<Esc>$.", "\"dd\" |x"),
    ]);
}

#[test]
fn vi_2_dot_is_one_undo_unit() {
    table(&[
        ("|a b c d", "dw.u", "|b c d"),
        ("|foo bar", "cwxy<Esc>u", "|foo bar"),
        ("|foo bar", "cwxy<Esc>w.u", "xy |bar"),
        ("|a b c d e f", "dw3.u", "|b c d e f"),
    ]);
}

#[test]
fn vi_2_one_undo_unit_per_command_and_per_insert_session() {
    table(&[
        ("|abc", "xxxu", "|c"),
        ("|abc", "ihi<Esc>ihi<Esc>u", "h|iabc"),
        ("|a\nb\nc", "2ddu", "|a\nb\nc"),
        ("|abc", "ihello<CR>world<Esc>u", "|abc"),
        ("|abc", "ahello<Esc>u<C-r>", "ahell|obc"),
        ("|a b", "cwx<Esc>u", "|a b"),
        ("|a\nb", "ox<Esc>u", "|a\nb"),
        ("|a\nb", ">>u", "|a\nb"),
        ("|ab", "Jp", "|ab"),
    ]);
}

#[test]
fn vi_2_commands_that_edit_are_refused_in_a_read_only_buffer() {
    let mut a = app_with(Some("f"), &[b'a', 0xff, b'\n'], (40, 8));
    for keys in ["x", "dd", "J", "p", "rx", "i", "o", ">>", "cw", "dw", "D", "vd", "Vd", "vc"] {
        feed(&mut a, keys);
        assert!(!a.buf.dirty(), "{keys}");
        feed(&mut a, "<Esc>");
    }
    feed(&mut a, "yy");
    assert_eq!(status(&screen(&mut a)), "1 line yanked", "yanking is fine");
    assert!(matches!(a.update(Event::Tick(0)).len(), 0));
}

/// A count of 20 digits used to overflow (a panic in debug, a wrap in release: `j` moved up).
#[test]
fn vi_2_huge_counts_saturate_instead_of_overflowing() {
    let keys =
        ["j", "k", "l", "h", "$", "dd", "yy", ">>", "x", "rx", "<C-d>", "<C-u>", "<PageDown>"];
    for k in keys.iter().chain(&["G", "J", "dj", "d$", "dl", "cc", "<lt><lt>", "p", "P", "."]) {
        let keys = format!("jl99999999999999999999{k}<Esc>");
        let r = within(10, move || {
            let mut a = app("abc\ndef\nghi\njkl\n");
            feed(&mut a, "yw");
            feed(&mut a, &keys);
            (a.buf.line_of(a.buf.cursor), text(&a).len())
        });
        assert!(r.is_some_and(|(_, len)| len < 1 << 20), "{k}: panicked, spun or grew: {r:?}");
    }
    table(&[
        ("|abc\ndef\nghi", "99999999999999999999j", "abc\ndef\n|ghi"),
        ("abc\ndef\n|ghi", "99999999999999999999k", "|abc\ndef\nghi"),
        ("|abc", "99999999999999999999l", "ab|c"),
        ("|abc\ndef", "99999999999999999999$", "abc\nde|f"),
        ("a|bc\ndef", "99999999999999999999dd", "|"),
        ("|abc\ndef", "99999999999999999999J", "abc| def"),
        ("|abc", "99999999999999999999x", "|"),
        ("|abc", "99999999999999999999rx", "|abc"),
    ]);
}

/// The largest file si is specified for is 10 MB: a count must not make the text grow past 16 MiB
/// (the allocation could abort the process with the unsaved text in it).
#[test]
fn vi_2_a_result_over_16_mib_is_refused_with_a_message() {
    let mut a = app("abc def\n");
    feed(&mut a, "yiw");
    let r = within(20, move || {
        for keys in ["18446744073709551615p", "100000000P", "6000000p"] {
            feed(&mut a, keys);
            assert_eq!(text(&a), "abc def", "{keys} was refused");
            assert_eq!(status(&screen(&mut a)), "too large: 16 MiB at most", "{keys}");
        }
        feed(&mut a, "1000p");
        text(&a).len()
    });
    assert_eq!(r, Some(7 + 3000), "a count that fits still works");
    let mut a = app("abc\n");
    feed(&mut a, "99999999999999999999rx");
    assert_eq!(text(&a), "abc", "`r` with a count past the line does nothing");
    let line = "a".repeat(4_300_000); // `r` of a 4-byte char over all of it would add 17 MB
    let mut a = app(&line);
    feed(&mut a, "4300000r😀");
    assert_eq!(status(&screen(&mut a)), "too large: 16 MiB at most");
    assert!(!a.buf.dirty());
}

#[test]
fn vi_2_dot_after_a_visual_change_does_not_replay_the_typed_text() {
    table(&[
        ("|abc", "vcx<Esc>.", "|xbc"),
        ("|abcd", "xvcQ<Esc>.", "|cd"),
        ("|abc\ndef", "Vcx<Esc>.", "|x\ndef"),
        ("|abcd", "xvd.", "|d"),
        ("|abcd", "vlcX<Esc>x.", "|d"),
    ]);
}

#[test]
fn vi_2_dot_after_undo_repeats_the_change_not_the_undo() {
    table(&[
        ("|abc", "xu.", "|bc"),
        ("|one\ntwo\nthree", "ddjddu.", "|two"),
        ("|abc", "xu<C-r>.", "|c"),
        ("|a b c", "dwuu.", "|b c"),
    ]);
}

#[test]
fn vi_2_j_adds_no_space_before_a_closing_paren() {
    table(&[("|foo(\n)x", "J", "foo(|)x"), ("|a\n) b", "J", "a|) b"), ("|a\n(b", "J", "a| (b")]);
}

#[test]
fn vi_8_taking_nothing_keeps_the_register_and_the_clipboard() {
    for keys in ["x", "Y", "y$", "D", "C<Esc>", "yl", "dl", "dh", "yh"] {
        let mut a = app("foo\n\nbar\n");
        feed(&mut a, "yiwj");
        let fx = feed(&mut a, keys);
        assert!(clipboard(&fx).is_empty(), "{keys}: nothing taken, nothing for the clipboard");
        feed(&mut a, "P");
        assert_eq!(text(&a), "foo\nfoo\nbar", "{keys}: the register still holds `foo`");
    }
}

#[test]
fn vi_8_p_from_an_empty_register_says_so_and_does_not_move() {
    for keys in ["p", "P", "3p", "3P"] {
        let mut a = app("ab\ncd\n");
        feed(&mut a, "j");
        let before = marked(&a);
        feed(&mut a, keys);
        assert_eq!(marked(&a), before, "{keys}");
        assert_eq!(status(&screen(&mut a)), "the register is empty", "{keys}");
    }
}
