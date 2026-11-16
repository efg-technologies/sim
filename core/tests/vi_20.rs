//! VI-20 (the surroundings, mini.surround's with the owner's prefix `gs`). Rows are (before, keys, after); `|`
//! is the cursor. Neovim 0.12.6 with mini.surround (its defaults, and the owner's mappings `gsa`, `gsd`,
//! `gsr`) gave every `after`; `core/tests/oracle.rs` (`SIM_ORACLE_MODE=surround`) checks more.
mod common;
use common::*;

#[test]
fn vi_20_gsa_puts_a_surrounding_around_a_motion_an_object_or_the_selection() {
    table(&[
        ("foo |bar baz", "gsaiw(", "foo ( |bar ) baz"),
        ("foo |bar baz", "gsaiw)", "foo (|bar) baz"),
        ("foo |bar baz", "gsaiw[", "foo [ |bar ] baz"),
        ("foo |bar baz", "gsaiw]", "foo [|bar] baz"),
        ("foo |bar baz", "gsaiw{", "foo { |bar } baz"),
        ("foo |bar baz", "gsaiw<", "foo < |bar > baz"),
        ("foo |bar baz", "gsaiwb", "foo (|bar) baz"),
        ("foo |bar baz", "gsaiwq", "foo \"|bar\" baz"),
        ("foo |bar baz", "gsaiw\"", "foo \"|bar\" baz"),
        ("foo |bar baz", "gsaiw'", "foo '|bar' baz"),
        ("foo |bar baz", "gsaiw`", "foo `|bar` baz"),
        ("foo |bar baz", "gsaiw*", "foo *|bar* baz"),
        ("foo |bar baz", "gsaw(", "foo ( |bar  )baz"),
        ("foo |bar baz", "gsa$)", "foo (|bar baz)"),
        ("foo |bar baz", "gsae.", "foo .|bar. baz"),
        ("  fo|o bar\n  baz", "gsaj)", "  (|foo bar\n  baz)"),
        ("  fo|o bar\n  baz  ", "gsaip\"", "  \"|foo bar\n  baz\"  "),
        ("foo |bar baz", "vegsa)", "foo (|bar) baz"),
        ("foo |bar baz\nqux", "Vjgsa\"", "\"|foo bar baz\nqux\""),
        ("[(|a)] (b)", "gsaiw[w.", "[([ a [ |])] ] (b)"),
    ]);
}

#[test]
fn vi_20_gsd_takes_the_surrounding_the_cursor_is_in_away_and_gsr_puts_another_in_its_place() {
    table(&[
        ("f( |a (b) )!", "gsd(", "f|a (b)!"),
        ("f( |a (b) )!", "gsd)", "f| a (b) !"),
        ("f( a (|b) )!", "gsd)", "f( a |b )!"),
        ("f( |a (b) )!", "gsdb", "f| a (b) !"),
        ("x '|aa' y 'bb'", "gsdq", "x |aa y 'bb'"),
        ("x 'aa|' y 'bb'", "gsd'", "x |aa y 'bb'"),
        ("x 'aa' |y 'bb'", "gsd'", "x 'aa| y bb'"),
        ("x _a|_b_ y", "gsd_", "x |ab_ y"),
        ("x _|a_b_ y", "gsd_", "x |ab_ y"),
        ("say \"h|i\" now", "gsr\"'", "say '|hi' now"),
        ("say \"h|i\" now", "gsr\"(", "say ( |hi ) now"),
        ("say \"h|i\" now", "gsr\")", "say (|hi) now"),
        ("say [ |hi ] now", "gsr[)", "say (|hi) now"),
        ("say [ |hi ] now", "gsr])", "say (| hi ) now"),
        ("a (b\n  |c\n) d", "gsd(", "a |b\n  c d"),
        ("a (b\n  |c\n) d", "gsd)", "a |b\n  c\n d"),
        ("a (\n  |b\n) d", "gsd(", "a |b d"),
        ("a |b c", "gsd(", "a |b c"),
        ("(a|) (b)", "gsd).", "|a (b)"),
        ("x (| ) y", "gsd(", "x | y"),
        ("x (|[) y]", "gsdb", "x |[ y]"),
    ]);
}

#[test]
fn vi_20_a_surrounding_that_asks_for_a_name_or_that_is_not_there_changes_nothing_and_says_so() {
    for keys in ["gsaiwt", "gsaiwf", "gsaiw?", "gsdt", "gsr(f"] {
        let a = typed("say (|hi) now", keys);
        assert_eq!(marked(&a), "say (|hi) now", "{keys}");
        assert_eq!(
            a.msg.as_deref(),
            Some("a surrounding that asks for a name (t f ?) is not built"),
            "{keys}"
        );
    }
    let a = typed("a |b c", "gsd(");
    assert_eq!(a.msg.as_deref(), Some("no surrounding ( found within 20 lines"));
    assert!(!a.buf.dirty());
}

#[test]
fn vi_20_one_surrounding_is_one_undo_step_and_fills_no_register() {
    let mut a = typed("|foo bar", "yiwwgsaiw(");
    assert_eq!(marked(&a), "foo ( |bar )");
    feed(&mut a, "u");
    assert_eq!(text(&a), "foo bar");
    feed(&mut a, "gsaiw(gsr()0p");
    assert_eq!(text(&a), "ffoooo (bar)");
    feed(&mut a, "uu");
    assert_eq!(text(&a), "foo ( bar )");
}

#[test]
fn vi_20_the_char_of_a_surrounding_is_typed_as_it_is_and_esc_gives_it_up() {
    // (VI-10: a full-width char is the surrounding, not read as half-width)
    let a = typed("say |hi", "gsaiw\u{ff08}");
    assert_eq!(marked(&a), "say \u{ff08}|hi\u{ff08}");
    let a = typed("say |hi", "gsaiw<Esc>x");
    assert_eq!(marked(&a), "say |i");
}
