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
    assert_eq!(status(&screen(&mut a)).get(..4), Some("f  1"), "yanking is fine");
    assert!(matches!(a.update(Event::Tick(0)).len(), 0));
}
