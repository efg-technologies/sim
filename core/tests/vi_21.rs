//! VI-21 (the pairs that typing makes, mini.pairs' defaults). Rows are (before, keys, after); `|` is the cursor.
//! Neovim 0.12.6 with mini.pairs gave every `after` but those of Enter between brackets, where Sim puts the
//! line between them a unit further in (the owner's Neovim indents there: `smartindent`, the languages'
//! indent rules).
mod common;
use common::*;
use sim_core::core::app::Event;

#[test]
fn vi_21_brackets_and_quotes_are_typed_in_pairs_and_a_closing_char_is_stepped_over() {
    table(&[
        ("|x", "a(<Esc>", "x|()"),
        ("|x", "a(a<Esc>", "x(|a)"),
        ("|x", "a(a)<Esc>", "x(a|)"),
        ("|x", "a()<Esc>", "x(|)"),
        ("|x", "a[]{}<Esc>", "x[]{|}"),
        ("|x", "a\"a\"b<Esc>", "x\"a\"|b"),
        ("|x", "a'<Esc>", "x|'"),
        ("|x", "a 'q'<Esc>", "x 'q|'"),
        ("|x", "adon't<Esc>", "xdon'|t"),
        ("|x", "a`a`<Esc>", "x`a|`"),
        ("|x", "a(<BS><Esc>", "|x"),
        ("|x", "a\"<BS><Esc>", "|x"),
        ("|x", "a(a<BS><BS><Esc>", "|x"),
        ("(|x)", "a)<Esc>", "(x|)"),
        ("(|x)", "i)<Esc>", "(|)x)"),
        ("|a)", "a)<Esc>", "a|)"),
        ("|a)", "a(<Esc>", "a|())"),
        ("|x", "a(x<Esc>.", "x(x(|x))"),
        ("|x", "a(<Esc>x", "x|)"),
        ("|ab", "a\"<Esc>", "a|\"\"b"),
        ("|ab", "a'<Esc>", "a|'b"),
        ("a| b", "a'<Esc>", "a |''b"),
        ("|é", "a'<Esc>", "é|''"),
    ]);
}

#[test]
fn vi_21_backspace_takes_an_empty_pair_and_enter_opens_one_over_three_lines() {
    table(&[
        ("|x", "a(<BS><Esc>", "|x"),
        ("|x", "a\"<BS><Esc>", "|x"),
        ("|x", "a(a<BS><BS><Esc>", "|x"),
        // (a line with nothing typed on it keeps no indent, VI-6)
        ("|f", "a(<CR><Esc>", "f(\n|\n)"),
        ("|f", "a(<CR>x<Esc>", "f(\n    |x\n)"),
        ("  |f", "a{<CR>x<Esc>", "  f{\n    |x\n  }"),
        ("|f", "a[<CR><CR>x<Esc>", "f[\n\n    |x\n]"),
        ("|f", "a\"<CR>x<Esc>", "f\"\n|x\""),
        // (Ctrl-w and Ctrl-u take what they always take)
        ("|x", "a(<C-w><Esc>", "|x)"),
    ]);
}

#[test]
fn vi_21_a_paste_is_never_paired_and_dot_types_the_keys_again() {
    let mut a = typed("|x", "a");
    a.update(Event::Paste("f(\"".into()));
    feed(&mut a, "<Esc>");
    assert_eq!(marked(&a), "xf(|\"");
    let mut a = typed("|x", "a(y<Esc>j.");
    assert_eq!(marked(&a), "x(y(|y))");
    feed(&mut a, "u");
    assert_eq!(text(&a), "x(y)");
}

#[test]
fn vi_21_after_a_backslash_nothing_is_paired_and_a_closing_char_is_typed() {
    table(&[
        ("|x\\", "A(<Esc>", "x\\|("),
        ("|x\\", "A\"<Esc>", "x\\|\""),
        ("x|\\)", "a)<Esc>", "x\\|))"),
    ]);
}
