//! VI-3: the operators `d c y > < !` with motions and text objects.
//! Rows are (before, keys, after); `|` is the cursor.
mod common;
use common::*;
use si::core::app::Effect;
use si::core::vi::Mode;

#[test]
fn vi_3_d_with_motions() {
    table(&[
        ("|foo bar", "dw", "|bar"),
        ("|foo bar", "d2w", "|"),
        ("|foo bar baz", "d2w", "|baz"),
        ("foo |bar\nbaz", "dw", "foo| \nbaz"),
        ("foo |bar\nbaz", "d2w", "foo| "),
        ("a |b\nc d", "d2w", "a |d"),
        ("|foo\n\nbar", "dw", "|\n\nbar"),
        ("foo.|bar", "dW", "foo|."),
        ("foo |bar baz", "db", "|bar baz"),
        ("|foo bar", "de", "| bar"),
        ("|foo bar", "d2e", "|"),
        ("f|oo bar", "dE", "f| bar"),
        ("ab |cd", "d0", "|cd"),
        ("  a|b", "d^", "  |b"),
        ("a|b cd", "d$", "|a"),
        ("ab\nc|d\nef", "d2$", "ab\n|c"),
        ("a\nb\n|c", "dgg", "|"),
        ("|a\nb\nc", "dG", "|"),
        ("|a\nb\nc\nd", "d3G", "|d"),
        ("|a\nb\nc", "d2gg", "|c"),
        ("a\n|b\nc\nd", "dj", "a\n|d"),
        ("a\nb\n|c\nd", "dk", "a\n|d"),
        ("a\n|b", "dj", "a\n|b"),
        ("|a\nb", "dk", "|a\nb"),
        ("|a,b", "df,", "|b"),
        ("|a,b", "dt,", "|,b"),
        ("|a,b,c", "d2f,", "|c"),
        ("a,|b", "dF,", "a|b"),
        ("a,b|c", "dT,", "a,|c"),
        ("|(a)b", "d%", "|b"),
        ("(a|)b", "d%", "|b"),
        ("|abcd", "dl", "|bcd"),
        ("ab|cd", "dh", "a|cd"),
        ("ab|cd", "d9l", "a|b"),
        ("|a", "dh", "|a"),
        ("|a b a", "d*", "|a"),
    ]);
}

#[test]
fn vi_3_c_with_motions_and_cw_is_ce() {
    table(&[
        ("|foo bar", "cwX<Esc>", "|X bar"),
        ("f|oo bar", "cwX<Esc>", "f|X bar"),
        ("fo|o bar", "cwX<Esc>", "fo|X bar"),
        ("|foo bar", "c2wX<Esc>", "|X"),
        ("|foo bar", "2cwX<Esc>", "|X"),
        ("foo| bar", "cwX<Esc>", "foo|Xbar"),
        ("|foo.bar", "cwX<Esc>", "|X.bar"),
        ("|foo.bar", "cWX<Esc>", "|X"),
        ("foo |bar", "cbX<Esc>", "|Xbar"),
        ("|foo bar", "ceX<Esc>", "|X bar"),
        ("a|bc", "c$X<Esc>", "a|X"),
        ("a|bc", "c0X<Esc>", "|Xbc"),
        ("|a,b", "ct,X<Esc>", "|X,b"),
        ("a\n|b\nc", "cjX<Esc>", "a\n|X"),
        ("a\n  |b\nc", "ckX<Esc>", "|X\nc"),
        ("  a\n  |b\nc", "ckX<Esc>", "  |X\nc"),
        ("|a\nb", "cGX<Esc>", "|X"),
    ]);
}

#[test]
fn vi_3_y_with_motions_leaves_the_text_and_moves_to_the_start() {
    table(&[
        ("|foo bar", "ywP", "foo| foo bar"),
        ("foo |bar", "ybP", "foo| foo bar"),
        ("|foo bar", "y$P", "foo ba|rfoo bar"),
        ("a|bc", "y0P", "|aabc"),
        ("a\n|b\nc", "ykP", "|a\nb\na\nb\nc"),
        ("|a\nb\nc", "yjjp", "a\nb\n|a\nb\nc"),
    ]);
}

#[test]
fn vi_3_text_objects_words() {
    table(&[
        ("fo|o bar", "diw", "| bar"),
        ("fo|o bar", "daw", "|bar"),
        ("foo ba|r", "daw", "fo|o"),
        ("foo| bar", "diw", "foo|bar"),
        ("foo| bar baz", "daw", "foo| baz"),
        ("a.|b c", "diW", "| c"),
        ("a.|b c", "daW", "|c"),
        ("foo.b|ar", "diw", "foo|."),
        ("fo|o bar", "ciwX<Esc>", "|X bar"),
        ("fo|o bar", "yiwP", "fo|ofoo bar"),
        ("fo|o bar", "caw!<Esc>", "|!bar"),
        ("|", "diw", "|"),
    ]);
}

#[test]
fn vi_3_text_objects_quotes() {
    table(&[
        ("say \"he|llo\" now", "di\"", "say \"|\" now"),
        ("say \"he|llo\" now", "da\"", "say |now"),
        ("|say \"hello\"", "di\"", "say \"|\""),
        ("\"a\" |x \"b\"", "di\"", "\"a\" x \"|\""),
        ("x \"a|b\"", "da\"", "|x"),
        ("it 's|o' ok", "di'", "it '|' ok"),
        ("it 's|o' ok", "da'", "it |ok"),
        ("\"a\\\"|b\" z", "di\"", "\"|\" z"),
        ("|a \"b", "di\"", "|a \"b"),
        ("\"a\n|b\"", "di\"", "\"a\n|b\""),
        ("\"a|\"", "ci\"X<Esc>", "\"|X\""),
    ]);
}

#[test]
fn vi_3_text_objects_brackets() {
    table(&[
        ("f(a, |b)", "di(", "f(|)"),
        ("f(a, |b)", "da(", "|f"),
        ("(a (b|) c)", "di(", "(a (|) c)"),
        ("(a |(b) c)", "di(", "(a (|) c)"),
        ("(a (b) |c)", "di(", "(|)"),
        ("|(ab)", "di(", "(|)"),
        ("(a|b)", "da(", "|"),
        ("x[1|2]", "di[", "x[|]"),
        ("x[1|2]", "da[", "|x"),
        ("{a|b}", "ci{X<Esc>", "{|X}"),
        ("{a|b}", "da{", "|"),
        ("<a|b>", "da<", "|"),
        ("<a|b>", "di<", "<|>"),
        ("f(a|)", "di)", "f(|)"),
        ("a|b", "di(", "a|b"),
        ("(a|b", "di(", "(a|b"),
        ("if (a &&\n b|) {", "di(", "if (|) {"),
        ("f(\n  a|,\n  b\n)", "di(", "f(\n|)"),
        ("f(\n  a|,\n  b\n)", "ci(X<Esc>", "f(\n  |X\n)"),
        ("f(\n  a|,\n  b\n)", "da(", "|f"),
        ("(|)", "di(", "(|)"),
    ]);
}

#[test]
fn vi_3_shift_with_motions() {
    table(&[
        ("|a\nb\nc", ">j", "    |a\n    b\nc"),
        ("a\n|b\nc", ">k", "    |a\n    b\nc"),
        ("|a\nb\nc", ">G", "    |a\n    b\n    c"),
        ("|a b\nc", ">w", "    |a b\nc"),
        ("    a\n    |b\nc", "<k", "|a\nb\nc"),
        ("|a\nb\nc", "2>j", "    |a\n    b\n    c"),
    ]);
}

#[test]
fn vi_3_bang_opens_the_command_line_with_the_line_numbers() {
    for (before, keys, want) in [
        ("|a\nb\nc", "!j", ":1,2!"),
        ("|a\nb\nc", "!!", ":1,1!"),
        ("|a\nb\nc", "3!!", ":1,3!"),
        ("a\n|b\nc", "!G", ":2,3!"),
        ("a\n|b\nc", "!k", ":1,2!"),
        ("|a\nb\nc", "Vj!", ":'<,'>!"),
        ("|a\nb\nc", "v!", ":'<,'>!"),
    ] {
        let mut a = typed(before, keys);
        assert_eq!(a.vi.mode, Mode::Cmd, "{keys}");
        assert_eq!(status(&screen(&mut a)), want, "{keys}");
        let fx = feed(&mut a, "sort<CR>");
        let runs = fx.iter().any(|e| matches!(e, Effect::Spawn { cmd, .. } if cmd == "sort"));
        assert!(runs, "{keys}: the command runs (PROP-5)");
        assert_eq!(
            text(&a),
            before.replace('|', ""),
            "nothing changes until the result is reviewed"
        );
    }
}
