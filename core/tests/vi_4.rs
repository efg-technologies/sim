//! VI-4: search with `/ ? n N * #`. Rows are (before, keys, after); `|` is the cursor.
mod common;
use common::*;

/// The message the keys left on the status line.
fn says(before: &str, keys: &str) -> String {
    status(&screen(&mut typed(before, keys))).to_string()
}

#[test]
fn vi_4_slash_searches_forward_from_the_cursor() {
    table(&[
        ("|a foo b foo", "/foo<CR>", "a |foo b foo"),
        ("|a foo b foo", "/foo<CR>n", "a foo b |foo"),
        ("a |foo b foo", "/foo<CR>", "a foo b |foo"),
        ("|x\nfoo\nbar\nfoo", "/foo<CR>", "x\n|foo\nbar\nfoo"),
        ("|x\nfoo\nbar\nfoo", "/foo<CR>n", "x\nfoo\nbar\n|foo"),
        ("|x a1c", "/a[0-9]c<CR>", "x |a1c"),
        ("|x\nab\nxab", "/^ab<CR>", "x\n|ab\nxab"),
        ("|ab\nab", "/b$<CR>", "a|b\nab"),
        ("|日本語 本", "/本<CR>", "日|本語 本"),
        ("|a (b)", "/(<CR>", "|a (b)"),
    ]);
}

#[test]
fn vi_4_question_mark_searches_backward_and_n_keeps_the_direction() {
    table(&[
        ("a foo b |foo", "?foo<CR>", "a |foo b foo"),
        ("a foo b foo c |x", "?foo<CR>n", "a |foo b foo c x"),
        ("a foo b foo c |x", "?foo<CR>N", "a |foo b foo c x"),
        ("a |foo b foo", "/foo<CR>N", "a |foo b foo"),
        ("|x foo foo", "/foo<CR>nN", "x |foo foo"),
    ]);
}

#[test]
fn vi_4_a_search_wraps_around_the_file_and_says_so() {
    assert_eq!(says("|a foo b foo", "/foo<CR>n"), "[no name]  1:9  NORMAL");
    assert_eq!(says("|a foo b foo", "/foo<CR>nn"), "search wrapped");
    check("|a foo b foo", "/foo<CR>nn", "a |foo b foo");
    assert_eq!(says("a |foo b foo", "?foo<CR>"), "search wrapped");
    check("a |foo b foo", "?foo<CR>", "a foo b |foo");
    assert_eq!(says("x\nfoo\n|y", "/foo<CR>"), "search wrapped");
    assert_eq!(says("|x\nfoo\ny", "/foo<CR>"), "[no name]  2:1  NORMAL");
    check("a |foo", "/foo<CR>", "a |foo");
    assert_eq!(says("a |foo", "/foo<CR>"), "search wrapped", "the only match is the cursor's own");
    assert_eq!(
        says("|a foo", "/foo<CR>l"),
        "[no name]  1:4  NORMAL",
        "the message goes with the next key"
    );
}

#[test]
fn vi_4_an_empty_pattern_repeats_the_last_one() {
    table(&[
        ("|a foo b foo", "/foo<CR>/<CR>", "a foo b |foo"),
        ("|a foo b foo", "/foo<CR>?<CR>", "a foo b |foo"),
        ("a foo b foo c |x", "?foo<CR>/<CR>", "a |foo b foo c x"),
    ]);
    assert_eq!(says("|a", "/<CR>"), "no previous pattern");
    assert_eq!(says("|a", "n"), "no previous pattern");
}

#[test]
fn vi_4_smartcase() {
    table(&[
        ("|x Foo foo", "/foo<CR>", "x |Foo foo"),
        ("|x foo Foo", "/Foo<CR>", "x foo |Foo"),
        ("|x foo FOO", "/FOO<CR>", "x foo |FOO"),
        ("|x FOO foo", "/foo<CR>", "x |FOO foo"),
        ("|x Foo foo", "/foo<CR>n", "x Foo |foo"),
    ]);
}

/// PERF-4 skips the lines that lack a literal of the pattern. Every pattern here must still find
/// its match: the literal is only what the regex needs, never more.
#[test]
fn vi_4_a_literal_in_the_pattern_never_hides_a_match() {
    table(&[
        ("|x\ncolor", "/colou?r<CR>", "x\n|color"),
        ("|x\nabbc", "/ab+c<CR>", "x\n|abbc"),
        ("|x\nab", "/a{1}b<CR>", "x\n|ab"),
        ("|x\nxxy", "/x{2}y<CR>", "x\n|xxy"),
        ("|x\na.txt", "/\\.txt<CR>", "x\na|.txt"),
        ("|x\na-b", "/a\\-b<CR>", "x\n|a-b"),
        ("|x\na word b", "/\\bword\\b<CR>", "x\na |word b"),
        ("|x\n\tnote", "/\\tnote<CR>", "x\n|\tnote"),
        ("|x\nA", "/\\x41<CR>", "x\n|A"),
        ("|x\nbx", "/[a-c]x<CR>", "x\n|bx"),
        ("|x\nFOO", "/(?i)foo<CR>", "x\n|FOO"),
        ("|x\nbar", "/foo|bar<CR>", "x\n|bar"),
        ("|x\nfoo1", "/foo\\d<CR>", "x\n|foo1"),
        ("|x\nfoo bar", "/foo.*bar<CR>", "x\n|foo bar"),
        ("|x\nabc", "/a.c<CR>", "x\n|abc"),
        ("|x\nfoo", "/^foo$<CR>", "x\n|foo"),
        ("|x\nカーソルx", "/カーソル<CR>", "x\n|カーソルx"),
        ("|x\nあ", "/あ+<CR>", "x\n|あ"),
        ("|x\nFOO", "/foo<CR>", "x\n|FOO"),
        ("|x\nfoo", "/Foo<CR>", "|x\nfoo"),
        ("x\nfoo\n|y", "?foo<CR>", "x\n|foo\ny"),
    ]);
}

/// Where the keys (a search) put the cursor, from `(line, col)`, and what the status line says.
fn found(text: &str, at: (usize, usize), keys: &str, par_min: usize) -> (usize, Option<String>) {
    let mut a = app_with(Some("f.rs"), text.as_bytes(), (80, 24));
    (a.par_min, a.buf.cursor) = (Some(par_min), a.buf.start(at.0) + at.1);
    feed(&mut a, keys);
    (a.buf.cursor, a.msg)
}

/// PERF-4: a big buffer is searched by 8 threads, one chunk of the steps each (after a short head
/// on one thread). The match is the one a single thread finds, wherever it is: in the head, on
/// either side of every chunk boundary, in the last chunk, on the cursor line itself (the very
/// last step), or nowhere; forward, backward, and wrapping.
#[test]
fn vi_4_a_parallel_search_finds_the_same_match_as_a_sequential_one() {
    const N: usize = 300; // 301 steps: a head of 256, then 8 chunks of 6 steps (the last has 3)
    let text = |marks: &[usize]| -> String {
        (0..N)
            .map(|i| match marks.contains(&i) {
                true => format!("line {i} a needle here\n"),
                false => format!("line {i} nothing\n"),
            })
            .collect()
    };
    let same = |text: &str, at: (usize, usize), keys: &str| {
        let (one, many) = (found(text, at, keys, usize::MAX), found(text, at, keys, 0));
        assert_eq!(one, many, "{keys:?} from {at:?}");
        one
    };
    let one: Vec<String> = (0..N).map(|mark| text(&[mark])).collect();
    let two: Vec<String> = [(3, 290), (100, 101), (255, 256), (262, 268), (40, 200), (299, 0)]
        .iter()
        .map(|&(a, b)| text(&[a, b]))
        .collect();
    // The steps around the head, either side of every chunk boundary, and the end.
    let steps =
        [0, 1, 2, N - 1, N].into_iter().chain((0..=8).flat_map(|k| 255 + 6 * k..257 + 6 * k));
    let steps: Vec<usize> = steps.filter(|&i| i <= N).collect();
    // The last cursor is behind the match on its own line, so forward that line is the last step.
    let literal: [(&str, bool); 2] = [("/needle\n", true), ("?needle\n", false)];
    let regex: [(&str, bool); 2] = [("/nee[d]le\n", true), ("?nee[d]le\n", false)];
    // A pattern with a literal skips most lines; one without runs the regex on all of them, which
    // is slow in a debug build, so it gets two cursors only.
    let runs = [(0, 0), (120, 0), (150, 19)]
        .into_iter()
        .flat_map(|at| literal.map(|(keys, fwd)| (at, keys, fwd)))
        .chain(
            [(0, 0), (150, 19)].into_iter().flat_map(|at| regex.map(|(keys, fwd)| (at, keys, fwd))),
        );
    let none = text(&[]);
    for (at, keys, fwd) in runs {
        same(&none, at, keys);
        for &i in &steps {
            let mark = if fwd { (at.0 + i) % N } else { (at.0 + N - i % N) % N };
            let (cursor, _) = same(&one[mark], at, keys);
            let before: usize = one[mark].lines().take(mark).map(|l| l.len() + 1).sum();
            let col = one[mark].lines().nth(mark).and_then(|l| l.find("needle")).unwrap();
            assert_eq!(cursor, before + col, "{keys:?} from {at:?}, the only match on line {mark}");
        }
        // Two matches: the first one in search order wins, not the first or last chunk's.
        for t in &two {
            same(t, at, keys);
        }
    }
}

#[test]
fn vi_4_star_and_hash_search_the_whole_word_case_sensitively() {
    table(&[
        ("|foo xfoo foo", "*", "foo xfoo |foo"),
        ("foo xfoo |foo", "#", "|foo xfoo foo"),
        ("|foo Foo foo", "*", "foo Foo |foo"),
        ("f|oo bar foo", "*", "foo bar |foo"),
        ("foo bar f|oo", "#", "|foo bar foo"),
        ("|(foo) foo", "*", "(foo) |foo"),
        ("|foo.bar foo", "*", "foo.bar |foo"),
        ("|a.b a.b", "*", "a.b |a.b"),
        ("|foo foo foo", "*n", "foo foo |foo"),
        ("|foo foo foo", "*N", "|foo foo foo"),
        ("foo foo |foo", "#n", "|foo foo foo"),
        ("foo foo |foo", "#N", "foo foo |foo"),
        ("|日本 日本", "*", "日本 |日本"),
        ("|a_1 a_1", "*", "a_1 |a_1"),
    ]);
    assert_eq!(says("|  ", "*"), "no word under the cursor");
}

#[test]
fn vi_4_a_bad_pattern_or_no_match_says_so_and_stays_put() {
    assert!(says("|abc", "/(<CR>").starts_with("bad pattern"));
    assert!(says("|abc", "/[<CR>").starts_with("bad pattern"));
    assert_eq!(says("|abc", "/zzz<CR>"), "not found: zzz");
    assert_eq!(says("|abc", "?zzz<CR>"), "not found: zzz");
    check("a|bc", "/zzz<CR>", "a|bc");
    check("a|bc", "/(<CR>", "a|bc");
    assert_eq!(says("|abc", "*"), "search wrapped", "the only match is the word itself");
}

#[test]
fn vi_4_the_search_keys_are_motions_for_operators_and_visual_mode() {
    table(&[
        ("|a b a c a", "/a<CR>dn", "a b |a"),
        ("|a b a", "d*", "|a"),
        ("a b |a", "d#", "|a"),
        ("|a foo", "/foo<CR>0vnd", "|oo"),
    ]);
}

#[test]
fn vi_4_the_command_line_of_a_search_is_edited_like_any_other() {
    check("|a foo b", "/fxo<Left><BS>o<CR>", "a |foo b");
    let mut a = typed("|abc", "/ab");
    assert_eq!(status(&screen(&mut a)), "/ab");
    feed(&mut a, "<Esc>");
    assert_eq!(status(&screen(&mut a)), "[no name]  1:1  NORMAL");
}

#[test]
fn vi_4_a_search_leaves_the_normal_cursor_on_a_char() {
    for pat in ["$", "x*", "^", "\\s*$"] {
        let mut a = app("ab\ncd\n");
        feed(&mut a, &format!("/{pat}\n"));
        let b = &a.buf;
        let l = b.line_of(b.cursor);
        assert!(b.cursor < b.end(l) || b.line_len(l) == 0, "/{pat}: {:?}", marked(&a));
    }
    let mut a = app("ab\ncd\n");
    feed(&mut a, "/$\n");
    assert_eq!(marked(&a), "a|b\ncd", "Normal mode rests on the last char, not on the line break");
    feed(&mut a, "aZ<Esc>");
    assert_eq!(text(&a), "abZ\ncd");
}

#[test]
fn vi_4_smartcase_ignores_escapes() {
    for (text, pat) in [
        ("x\na-FOO\n", "\\Wfoo"),
        ("x\nfoo-\n", "foo\\D"),
        ("x\nj\n", "\\x4A"),
        ("x\nj\n", "\\x{4A}"),
        ("x\nAB\n", "\\x{61}b"),
        ("x\nJ\n", "\\u004A"),
    ] {
        let mut a = app_with(None, text.as_bytes(), (80, 24));
        feed(&mut a, &format!("/{pat}\n"));
        assert_eq!(a.buf.line_of(a.buf.cursor), 1, "/{pat} must find {text:?}");
    }
    // An uppercase letter outside an escape still makes the search case-sensitive.
    let mut a = app("x\nfoo\nFoo\n");
    feed(&mut a, "/\\wFoo\n");
    assert_eq!(a.buf.line_of(a.buf.cursor), 0, "no match: the cursor stays");
    assert_eq!(status(&screen(&mut a)), "not found: \\wFoo");
}
