//! VI-19 (the text objects and motions the hands expect). Rows are (before, keys, after); `|` is the cursor;
//! Neovim 0.12.6 (`-u NONE`, `startofline`) gave every `after`, and `core/tests/oracle.rs` checks more.
mod common;
use common::*;

#[test]
fn vi_19_braces_go_by_paragraphs_and_ip_and_ap_take_them() {
    table(&[
        ("|a b\nc d\n\ne f\n  \ng h\ni j", "}", "a b\nc d\n|\ne f\n  \ng h\ni j"),
        ("|a b\nc d\n\ne f\n  \ng h\ni j", "2}", "a b\nc d\n\ne f\n  \ng h\ni |j"),
        ("|a b\nc d\n\ne f\n  \ng h\ni j", "3}", "|a b\nc d\n\ne f\n  \ng h\ni j"),
        ("|a b\nc d\n\ne f\n  \ng h\ni j", "4}", "|a b\nc d\n\ne f\n  \ng h\ni j"),
        ("a b\nc d\n\ne f\n  \ng h\ni| j", "{", "a b\nc d\n|\ne f\n  \ng h\ni j"),
        ("a b\nc d\n\ne f\n  \ng h\ni| j", "2{", "|a b\nc d\n\ne f\n  \ng h\ni j"),
        ("a b\nc d\n\ne f\n  \ng h\ni| j", "3{", "a b\nc d\n\ne f\n  \ng h\ni| j"),
        ("|a b\nc d\n\ne f\n  \ng h\ni j", "d}", "|\ne f\n  \ng h\ni j"),
        ("a b\nc d\n\ne f\n  \ng h\n|i j", "d{", "a b\nc d\n|i j"),
        ("a b\nc d\n|\ne f\n  \ng h\ni j", "}", "a b\nc d\n\ne f\n  \ng h\ni |j"),
        ("a b\nc d\n|\ne f\n  \ng h\ni j", "dip", "a b\nc d\n|e f\n  \ng h\ni j"),
        ("a b\nc d\n|\ne f\n  \ng h\ni j", "dap", "a b\nc d\n | \ng h\ni j"),
        ("a b\nc d\n\n|e f\n  \ng h\ni j", "dap", "a b\nc d\n\n|g h\ni j"),
        ("a b\nc d\n\ne f\n  \n|g h\ni j", "dap", "a b\nc d\n\n|e f"),
        ("a b\nc d\n\ne f\n  \ng h\n|i j", "dap", "a b\nc d\n\n|e f"),
        ("a b\nc d\n\ne f\n|  \ng h\ni j", "dip", "a b\nc d\n\ne f\n|g h\ni j"),
        ("a b\nc d\n\ne f\n|  \ng h\ni j", "dap", "a b\nc d\n\n|e f"),
        ("a b\n|c d\n\ne f\n  \ng h\ni j", "yip", "|a b\nc d\n\ne f\n  \ng h\ni j"),
        ("|a b\nc d\n\ne f\n  \ng h\ni j", "vipd", "|\ne f\n  \ng h\ni j"),
        ("|a b\nc d\n\ne f\n  \ng h\ni j", "vapd", "|e f\n  \ng h\ni j"),
        ("a b\nc d\n|\ne f\n  \ng h\ni j", "vipd", "a b\nc d\n|e f\n  \ng h\ni j"),
    ]);
}

#[test]
fn vi_19_ap_on_the_blank_lines_at_the_end_takes_nothing_and_vip_on_a_blank_line_is_its_lines() {
    table(&[
        ("a\nb\n\n|  ", "dap", "a\nb\n\n|  "),
        ("a\nb\n|\n  ", "dap", "a\nb\n|\n  "),
        ("a\nb\n|\n  ", "vipd", "a\n|b"),
        ("a\n|\nb", "vipd", "a\n|b"),
    ]);
}

#[test]
fn vi_19_backtick_quotes_are_an_object() {
    table(&[
        ("say `h|i there` now", "di`", "say `|` now"),
        ("say `h|i there` now", "da`", "say |now"),
        ("say `h|i there` now", "ci`x<Esc>", "say `|x` now"),
    ]);
}

#[test]
fn vi_19_tilde_and_gu_gu_and_g_tilde_change_the_case() {
    table(&[
        ("a b\nc| d\n\ne f\n  \ng h\ni j", "d}", "a b\n|c\n\ne f\n  \ng h\ni j"),
        ("a b\nc |d\n\ne f\n  \ng h\ni j", "d{", "|d\n\ne f\n  \ng h\ni j"),
        ("a |b\nc d\n\ne f\n  \ng h\ni j", "dip", "|\ne f\n  \ng h\ni j"),
        ("a |b\nc d\n\ne f\n  \ng h\ni j", "dap", "|e f\n  \ng h\ni j"),
        ("a|Bc dEf\nGhI", "~", "ab|c dEf\nGhI"),
        ("a|Bc dEf\nGhI", "3~", "abC |dEf\nGhI"),
        ("a|Bc dEf\nGhI", "9~", "abC De|F\nGhI"),
        ("a|Bc dEf\nGhI", "3~.", "abC De|F\nGhI"),
        ("a|Bc dEf\nGhI", "gUiw", "|ABC dEf\nGhI"),
        ("a|Bc dEf\nGhI", "guiw", "|abc dEf\nGhI"),
        ("a|Bc dEf\nGhI", "g~iw", "|AbC dEf\nGhI"),
        ("aBc d|Ef\nGhI", "gUU", "|ABC DEF\nGhI"),
        ("aBc d|Ef\nGhI", "guu", "|abc def\nGhI"),
        ("aBc d|Ef\nGhI", "g~~", "|AbC DeF\nGhI"),
        ("aBc d|Ef\nGhI", "gUgU", "|ABC DEF\nGhI"),
        ("aBc d|Ef\nGhI", "gUj", "ABC D|EF\nGHI"),
        ("aBc dEf\nG|hI", "gUk", "A|BC DEF\nGHI"),
        ("aBc d|Ef\nGhI", "vjU", "aBc d|EF\nGHI"),
        ("aBc d|Ef\nGhI", "vju", "aBc d|ef\nghi"),
        ("aBc d|Ef\nGhI", "vj~", "aBc d|eF\ngHi"),
        ("aBc d|Ef\nGhI", "VU", "|ABC DEF\nGhI"),
        ("aBc d|Ef\nGhI", "gUw.", "aBc d|EF\nGhI"),
        ("aBc dEf\nG|hI", "2gUU", "aBc dEf\nG|hI"),
        ("|straße", "gUU", "|STRAẞE"),
        ("|straße", "g~~", "|STRAẞE"),
        ("|Été", "g~~", "|éTÉ"),
    ]);
}

#[test]
fn vi_19_gv_selects_the_last_selection_again() {
    table(&[
        ("a|Bc dEf\nGhI", "vly0gvd", "a| dEf\nGhI"),
        ("a|Bc dEf\nGhI", "Vjy0gvd", "|"),
        ("a|Bc dEf\nGhI", "gvd", "a|Bc dEf\nGhI"),
        ("aBc |dEf\nGhI", "vlo<Esc>0gvd", "aBc |f\nGhI"),
    ]);
    let mut a = typed("a|b", "gv");
    assert_eq!(a.msg.as_deref(), Some("no previous selection"));
    // the ends follow the edits as marks do: two lines put above move the selection with its text
    feed(&mut a, "vl<Esc>ggOx<CR>y<Esc>gvd");
    assert_eq!(marked(&a), "x\ny\n|a");
}
