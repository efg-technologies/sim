//! VI-6: Insert mode (and the keys of VI-12 inside it). Rows are (before, keys, after); `|` is
//! the cursor.
mod common;
use common::*;
use si::core::app::{Effect, Event};
use si::core::vi::Mode;

fn numbered(n: usize) -> String {
    (1..=n).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n")
}

#[test]
fn vi_6_typing_enter_and_the_indent() {
    table(&[
        ("a|b", "iX<CR>Y<Esc>", "aX\n|Yb"),
        ("  a|b", "A<CR>x<Esc>", "  ab\n  |x"),
        ("\tab|c", "A<CR>x<Esc>", "\tabc\n\t|x"),
        ("  a|b", "i<CR>x<Esc>", "  a\n  |xb"),
        ("    |  x", "i<CR>y<Esc>", "    \n    |y  x"),
        ("|a", "i日本語<Esc>", "日本|語a"),
    ]);
}

#[test]
fn vi_6_backspace_and_delete_join_lines() {
    table(&[
        ("ab|c", "iX<BS><BS><Esc>", "|ac"),
        ("ab\n|cd", "i<BS><Esc>", "a|bcd"),
        ("|ab", "i<BS><Esc>", "|ab"),
        ("a|bc", "i<Del><Esc>", "|ac"),
        ("a|b\ncd", "A<Del><Esc>", "a|bcd"),
        ("a|b", "A<Del><Esc>", "a|b"),
    ]);
}

#[test]
fn vi_6_tab_inserts_the_indent_unit_of_the_file() {
    table(&[
        ("|a", "i<Tab><Esc>", "   | a"),
        ("\tx\n|a", "i<Tab><Esc>", "\tx\n|\ta"),
        ("  x\n|a", "i<Tab><Esc>", "  x\n | a"),
        ("      x\n|a", "i<Tab><Esc>", "      x\n     | a"),
        ("|a", "i<Tab><Tab>x<Esc>", "        |xa"),
    ]);
}

#[test]
fn vi_6_ctrl_w_deletes_the_word_before_the_cursor() {
    table(&[
        ("foo ba|r", "A<C-w><Esc>", "foo| "),
        ("foo.ba|r", "A<C-w><Esc>", "foo|."),
        ("foo  | ", "A<C-w><Esc>", "|"),
        ("foo bar ba|z", "A<C-w><C-w><Esc>", "foo| "),
        ("ab\n|cd", "i<C-w><Esc>", "a|bcd"),
        ("foo |bar", "i<C-w><Esc>", "|bar"),
        ("|foo", "i<C-w><Esc>", "|foo"),
    ]);
}

#[test]
fn vi_6_ctrl_u_deletes_to_the_indent_and_then_to_the_line_start() {
    table(&[
        ("  fo|o", "A<C-u><Esc>", " | "),
        ("  fo|o", "A<C-u><C-u><Esc>", "|"),
        ("  foo b|ar", "i<C-u><Esc>", " | ar"),
        ("|abc", "i<C-u><Esc>", "|abc"),
        ("  |abc", "i<C-u><Esc>", "|abc"),
    ]);
}

#[test]
fn vi_6_esc_and_ctrl_c_leave_and_step_back() {
    table(&[
        ("ab|c", "i<Esc>", "a|bc"),
        ("ab|c", "i<C-c>", "a|bc"),
        ("|ab", "i<Esc>", "|ab"),
        ("ab|c", "A<Esc>", "ab|c"),
    ]);
    assert_eq!(typed("|a", "i").vi.mode, Mode::Insert);
    assert_eq!(typed("|a", "i<Esc>").vi.mode, Mode::Normal);
    assert_eq!(typed("|a", "i<C-c>").vi.mode, Mode::Normal);
}

#[test]
fn vi_6_the_moving_keys_do_not_end_the_undo_unit() {
    table(&[
        ("|ab", "iX<Right>Y<Esc>", "Xa|Yb"),
        ("|ab", "iX<Right>Y<Esc>u", "|ab"),
        ("a|b", "iX<Left>Y<Esc>", "a|YXb"),
        ("a|b", "iX<Left>Y<Esc>u", "a|b"),
        ("a|b", "i<Home>X<Esc>", "|Xab"),
        ("|ab", "i<End>X<Esc>", "ab|X"),
        ("a|b\ncde", "i<Down>X<Esc>", "ab\nc|Xde"),
        ("ab\nc|de", "i<Up>X<Esc>", "a|Xb\ncde"),
        ("abc|d\nx\nabcd", "i<Down><Down>Y<Esc>", "abcd\nx\nabc|Yd"),
        ("abc|d\nx\nabcd", "i<Down><Down>Y<Esc>u", "abc|d\nx\nabcd"),
    ]);
}

#[test]
fn vi_6_page_keys_in_insert_mode_move_by_a_screen() {
    let mut a = app(&numbered(100));
    feed(&mut a, "i<PageDown>");
    assert_eq!(a.buf.line_of(a.buf.cursor), 23);
    feed(&mut a, "<PageDown><PageUp>");
    assert_eq!(a.buf.line_of(a.buf.cursor), 23);
    assert_eq!(a.vi.mode, Mode::Insert);
}

#[test]
fn vi_6_insert_keys_after_an_arrow_forget_nothing() {
    // Typing after Up / Down starts from the column the cursor is really in.
    check("abcd\n|xy", "i<Up>X<Esc>", "|Xabcd\nxy");
}

#[test]
fn vi_6_ctrl_s_saves_and_stays_in_insert_mode() {
    let mut a = app_with(Some("f"), b"a\n", (40, 8));
    feed(&mut a, "ix");
    let fx = feed(&mut a, "<C-s>");
    let writes =
        fx.iter().filter(|e| matches!(e, Effect::Write { bytes, .. } if bytes == b"xa\n")).count();
    assert_eq!((writes, a.vi.mode), (1, Mode::Insert));
    feed(&mut a, "y<Esc>");
    assert_eq!(text(&a), "xya");
    a.update(Event::Saved(Ok((1, None))));
}
