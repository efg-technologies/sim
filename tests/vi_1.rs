//! VI-1 (motions) and VI-12 (the motion keys every editor has, in Normal mode). Rows are
//! (before, keys, after); `|` is the cursor.
mod common;
use common::*;
use si::core::app::App;

fn line(a: &App) -> usize {
    a.buf.line_of(a.buf.cursor)
}

fn numbered(n: usize) -> String {
    (1..=n).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n")
}

#[test]
fn vi_1_h_j_k_l_and_counts() {
    table(&[
        ("a|bc", "h", "|abc"),
        ("|abc", "h", "|abc"),
        ("|abc", "l", "a|bc"),
        ("ab|c", "l", "ab|c"),
        ("|abcd", "2l", "ab|cd"),
        ("|abcd", "9l", "abc|d"),
        ("abc|d", "3h", "|abcd"),
        ("|a\nb\nc\nd", "2j", "a\nb\n|c\nd"),
        ("|a\nb", "5j", "a\n|b"),
        ("a\nb\nc\n|d", "3k", "|a\nb\nc\nd"),
        ("|a", "k", "|a"),
        ("|", "lhjk", "|"),
    ]);
}

#[test]
fn vi_1_j_and_k_keep_the_wanted_column() {
    table(&[
        ("abc|d\nx\nabcd", "jj", "abcd\nx\nabc|d"),
        ("abc|d\n\nabcd", "jk", "abc|d\n\nabcd"),
        ("abcd\nx\nabc|d", "kk", "abc|d\nx\nabcd"),
        ("abc|d\nwxyz\nabcd", "jhj", "abcd\nwxyz\nab|cd"),
        ("abc|d\nx\nabcd", "j0j", "abcd\nx\n|abcd"),
    ]);
}

#[test]
fn vi_1_word_motions() {
    table(&[
        ("|foo bar baz", "w", "foo |bar baz"),
        ("|foo bar baz", "2w", "foo bar |baz"),
        ("|foo bar baz", "9w", "foo bar ba|z"),
        ("|foo.bar", "w", "foo|.bar"),
        ("|foo.bar", "2w", "foo.|bar"),
        ("|foo.bar baz", "W", "foo.bar |baz"),
        ("fo|o\nbar", "w", "foo\n|bar"),
        ("|foo\n\nbar", "w", "foo\n|\nbar"),
        ("|foo\n\nbar", "2w", "foo\n\n|bar"),
        ("|foo  \n  bar", "w", "foo  \n  |bar"),
        ("|a_b c", "w", "a_b |c"),
        ("foo bar |baz", "b", "foo |bar baz"),
        ("foo bar |baz", "2b", "|foo bar baz"),
        ("foo.ba|r", "b", "foo.|bar"),
        ("foo.ba|r", "2b", "foo|.bar"),
        ("foo.bar |baz", "B", "|foo.bar baz"),
        ("foo\n\n|bar", "b", "foo\n|\nbar"),
        ("foo\n\n|bar", "2b", "|foo\n\nbar"),
        ("|foo bar", "e", "fo|o bar"),
        ("fo|o bar", "e", "foo ba|r"),
        ("|foo bar", "2e", "foo ba|r"),
        ("|foo.bar", "e", "fo|o.bar"),
        ("|foo.bar baz", "E", "foo.ba|r baz"),
        ("fo|o\nbar", "e", "foo\nba|r"),
        ("fo|o", "e", "fo|o"),
        ("日本| 語", "w", "日本 |語"),
    ]);
}

#[test]
fn vi_1_line_motions_and_the_keys_that_mean_the_same() {
    table(&[
        ("  fo|o bar", "0", "|  foo bar"),
        ("  fo|o bar", "<Home>", "|  foo bar"),
        ("  fo|o bar", "^", "  |foo bar"),
        ("  fo|o bar", "$", "  foo ba|r"),
        ("  fo|o bar", "<End>", "  foo ba|r"),
        ("a|b\ncd\nef", "2$", "ab\nc|d\nef"),
        ("|\n", "$", "|"),
        ("  | ", "^", "  | "),
        ("a\n  b\n|c", "gg", "|a\n  b\nc"),
        ("|a\nb\n  c", "G", "a\nb\n  |c"),
        ("|a\n  b\nc", "2G", "a\n  |b\nc"),
        ("|a\n  b\nc", "2gg", "a\n  |b\nc"),
        ("|a\nb", "9G", "a\n|b"),
    ]);
}

#[test]
fn vi_1_f_t_and_their_repeats() {
    table(&[
        ("|a,b,c", "f,", "a|,b,c"),
        ("|a,b,c", "2f,", "a,b|,c"),
        ("|a,b,c", "3f,", "|a,b,c"),
        ("a,b,|c", "F,", "a,b|,c"),
        ("a,b,|c", "2F,", "a|,b,c"),
        ("|ab,c", "t,", "a|b,c"),
        ("ab,c|d", "T,", "ab,|cd"),
        ("|a,b,c,d", "f,;", "a,b|,c,d"),
        ("|a,b,c,d", "f,;,", "a|,b,c,d"),
        ("|a,b,c,d", "f,2;", "a,b,c|,d"),
        ("|ab,cd,ef", "t,;", "ab,c|d,ef"),
        ("ab,cd,e|f", "T,;", "ab,|cd,ef"),
        ("|abc", "fz", "|abc"),
        ("|a\nb", "fb", "|a\nb"),
        ("|abc", ";", "|abc"),
    ]);
}

#[test]
fn vi_1_percent_matches_brackets_across_lines() {
    table(&[
        ("|(a[b]c)", "%", "(a[b]c|)"),
        ("(a[b]c|)", "%", "|(a[b]c)"),
        ("|(a(b)c)", "%", "(a(b)c|)"),
        ("(a|(b)c)", "%", "(a(b|)c)"),
        ("|{\n  (\n  )\n}", "%", "{\n  (\n  )\n|}"),
        ("{\n  (\n  )\n|}", "%", "|{\n  (\n  )\n}"),
        ("x |y (a)", "%", "x y (a|)"),
        ("|(a", "%", "|(a"),
        ("|abc", "%", "|abc"),
        ("|a)", "%", "|a)"),
        ("|a ] (b)", "%", "|a ] (b)"),
    ]);
}

/// The scan runs over rope chunks, not over one string: brackets far apart, in text of wide chars.
#[test]
fn vi_1_percent_scans_across_many_rope_chunks() {
    let body = "日本語 (x) [y]\n".repeat(2000);
    let end = body.chars().count() + 1;
    let both = format!("({body})");
    let mut a = typed(&format!("|{both}"), "%");
    assert_eq!(a.buf.cursor, end, "forward to the matching bracket");
    feed(&mut a, "%");
    assert_eq!(a.buf.cursor, 0, "and back");
    // A bracket with no partner: the scan reads to the end of the text and the cursor stays.
    let open = format!("({body}");
    assert_eq!(typed(&format!("|{open}"), "%").buf.cursor, 0);
    let a = typed(&format!("{body}|)"), "%");
    assert_eq!(a.buf.cursor, end - 1, "a `)` with no `(` stays where it is");
}

#[test]
fn vi_1_ctrl_d_ctrl_u_and_the_page_keys_move_by_screens() {
    // 80x24: 23 text rows, so half a screen is 11 lines and a screen is 23.
    let mut a = app(&numbered(100));
    for (keys, want) in [
        ("<C-d>", 11),
        ("<C-d>", 22),
        ("<C-u>", 11),
        ("<PageDown>", 34),
        ("<PageDown>", 57),
        ("<PageUp>", 34),
        ("2<C-d>", 56),
        ("2<C-u>", 34),
        ("<PageUp><PageUp><PageUp>", 0),
        ("<C-u>", 0),
        ("G<C-d>", 99),
    ] {
        feed(&mut a, keys);
        assert_eq!(line(&a), want, "{keys}");
    }
}

#[test]
fn vi_1_ctrl_d_keeps_the_column() {
    check("ab|c\nabc\nabc\nabc", "<C-d>", "abc\nabc\nabc\nab|c");
    check("abc\nabc\nabc\nab|c", "<C-u>", "ab|c\nabc\nabc\nabc");
}

#[test]
fn vi_1_zz_centers_the_cursor_line() {
    let mut a = app(&numbered(100));
    feed(&mut a, "40j");
    assert_eq!(a.top, 40 - 14, "40j scrolls just enough to keep 8 rows below");
    feed(&mut a, "zz");
    assert_eq!((a.top, screen(&mut a).cursor.1), (40 - 11, 11));
}

#[test]
fn vi_1_counts_multiply_around_an_operator() {
    table(&[
        ("|a b c d e f g", "2d2w", "|e f g"),
        ("|a b c d e f g", "d2w", "|c d e f g"),
        ("|a b c d e f g", "3dw", "|d e f g"),
        ("|abcdef", "d3l", "|def"),
        ("|a\nb\nc\nd\ne\nf", "2d2j", "|f"),
    ]);
}

#[test]
fn vi_12_arrows_home_end_and_delete_in_normal_mode() {
    table(&[
        ("a|bc", "<Left>", "|abc"),
        ("a|bc", "<Right>", "ab|c"),
        ("a|bc\nxyz", "<Down>", "abc\nx|yz"),
        ("abc\nx|yz", "<Up>", "a|bc\nxyz"),
        ("a|bc\nxyz", "2<Right>", "ab|c\nxyz"),
        ("a|bc", "<Home>", "|abc"),
        ("a|bc", "<End>", "ab|c"),
        ("a|bc", "<Del>", "a|c"),
        ("ab|c", "<Del>", "a|b"),
        ("a|bcd", "2<Del>", "a|d"),
        ("|", "<Del><Left><Right><Up><Down><Home><End>", "|"),
    ]);
}

#[test]
fn vi_12_the_page_keys_in_normal_mode() {
    let mut a = app(&numbered(100));
    feed(&mut a, "<PageDown>");
    assert_eq!(line(&a), 23);
    feed(&mut a, "<PageUp>");
    assert_eq!(line(&a), 0);
}

/// On the empty last line the cursor is at the end of the text: there is no word ahead, and
/// nothing reaches back for the line break before it.
#[test]
fn vi_1_de_and_cw_on_the_empty_last_line_keep_the_line_break() {
    for keys in ["de", "dE", "dw", "dW", "yw", "ye", "db"] {
        let mut a = app("foo\n\n");
        feed(&mut a, &format!("G{keys}"));
        let want = if keys == "db" { "" } else { "foo\n" }; // nvim: `db` takes the line above
        assert_eq!(text(&a), want, "{keys}");
    }
    for keys in ["cwx", "cex", "cWx", "cEx"] {
        let mut a = app("foo\n\n");
        feed(&mut a, &format!("G{keys}<Esc>"));
        assert_eq!(text(&a), "foo\nx", "{keys}");
    }
}

#[test]
fn vi_1_a_huge_count_on_a_word_motion_returns() {
    for k in ["w", "W", "b", "B", "e", "E", "dw", "cw", "de", "db", "cb", "ce", "yw"] {
        let keys = format!("$18446744073709551615{k}<Esc>");
        let done = within(5, move || feed(&mut app("foo bar\nbaz\n"), &keys).len());
        assert!(done.is_some(), "{k} with a huge count spins");
    }
    // Stopping when a step makes no progress must not break the counted `cw` on the last char.
    table(&[
        ("|foo\n ", "2dw", "|"),
        ("a|b", "c2wX<Esc>", "a|X"),
        ("ab c|d", "3dw", "ab |c"),
        ("|foo bar", "99dw", "|"),
        ("foo ba|r", "99w", "foo ba|r"),
        ("foo ba|r", "99b", "|foo bar"),
    ]);
}

/// Han, Hiragana, Katakana and other wide characters are classes of their own. Every row is what
/// `nvim --headless` gives (vim's utf_class).
#[test]
fn vi_1_word_motions_split_japanese_text_where_the_script_changes() {
    table(&[
        ("|日本語のテスト abc", "dw", "|のテスト abc"),
        ("|日本語のテスト abc", "2dw", "|テスト abc"),
        ("|日本語のテスト abc", "3dw", "|abc"),
        ("|日本語のテスト abc", "de", "|のテスト abc"),
        ("|abc日本語def", "dw", "|日本語def"),
        ("|日本語のテスト abc", "w", "日本語|のテスト abc"),
        ("日本語のテスト |abc", "b", "日本語の|テスト abc"),
        ("日本語の|テスト abc", "b", "日本語|のテスト abc"),
        ("|ひらがなカタカナ", "w", "ひらがな|カタカナ"),
        ("|カタカナーひらがな", "w", "カタカナー|ひらがな"),
        ("|日本語、のテスト", "w", "日本語|、のテスト"),
        ("|한국어日本語", "w", "한국어|日本語"),
        ("|ＡＢＣabc", "w", "ＡＢＣ|abc"),
        ("|日本語のテスト abc", "W", "日本語のテスト |abc"),
        ("日|本語のテスト", "diw", "|のテスト"),
        ("日本語|のテスト", "daw", "日本語|テスト"),
        ("日本語の|テスト", "ciwX<Esc>", "日本語の|X"),
    ]);
}

/// vim's `:help exclusive` rules 1 and 2. Every row is what `nvim --headless` gives.
#[test]
fn vi_1_an_exclusive_motion_that_ends_in_column_one_stops_at_the_line_before() {
    table(&[
        // Rule 2: from the first non-blank or before, the motion is linewise.
        ("a\n|\nb", "dw", "a\n|b"),
        ("a\n|\nb", "dW", "a\n|b"),
        ("a\n|\n  b", "dw", "a\n  |b"),
        ("a\n|\nb", "cwX<Esc>", "a\n|X\nb"),
        ("foo\n|bar", "db", "|bar"),
        ("  foo\n|bar", "db", "|bar"),
        // Rule 1: otherwise it ends at the end of the line before.
        ("baz_1 one,two\n|bar", "db", "baz_1 one|,\nbar"),
        ("x foo\n|bar", "db", "x| \nbar"),
        ("a b\n|c", "d2b", "|c"),
        // `dw` on a last word still stops at the end of the line.
        ("a |b\nc", "dw", "a| \nc"),
        ("|a\nb", "d2w", "|"),
    ]);
    let mut a = app("a.b\n(a, b) foo(bar)\n\n");
    a.buf.cursor = text(&a).chars().count(); // the empty last line
    feed(&mut a, "dB");
    assert_eq!(text(&a), "a.b\n(a, b) \n");
    let mut a = app("a\n\nb");
    feed(&mut a, "jyw");
    assert_eq!(a.buf.cursor, 2);
    feed(&mut a, "P");
    assert_eq!(text(&a), "a\n\n\nb", "the empty line was yanked as a line");
}
