//! VIS-7: wrapping applies kinsoku. `、。，．）」』】〕〉》ー…‥・！？` never start a row, `（「『【〔〈《`
//! never end one, a full-width character counts as 2 columns, and a row that cannot satisfy the rule
//! breaks at the edge. The property test throws random mixes of CJK, ASCII words, punctuation from
//! both sets, spaces, tabs and emoji at `wrap`. (The widths of emoji are tests/view.rs.)
#![allow(clippy::unwrap_used)]
mod common;
use common::*;
use proptest::prelude::*;
use si::core::view::{width, wrap};
use unicode_width::UnicodeWidthChar;

// Written out again here, from SPEC, so that the test does not trust the code's own lists.
const NOT_START: &str = "、。，．）」』】〕〉》ー…‥・！？";
const NOT_END: &str = "（「『【〔〈《";

/// The rows of `line` when wrapped at `max` columns.
fn rows(line: &str, max: usize) -> Vec<String> {
    let cs: Vec<char> = line.chars().collect();
    let starts = wrap(&cs, max);
    let ends = starts.iter().skip(1).copied().chain([cs.len()]);
    starts.iter().zip(ends).map(|(&s, e)| cs[s..e].iter().collect()).collect()
}

/// The columns a row takes, counted the way the screen draws it.
fn cols(row: &str) -> usize {
    let cs: Vec<char> = row.chars().collect();
    (0..cs.len()).fold(0, |col, i| col + width(&cs, i, col))
}

#[test]
fn vis_7_a_closing_mark_does_not_start_a_row() {
    // Five wide characters fill ten columns: the comma-like mark would start the next row.
    assert_eq!(rows("あいうえお、かき", 10), ["あいうえ", "お、かき"]);
    assert_eq!(rows("あいうえお。かき", 10), ["あいうえ", "お。かき"]);
    // Two marks in a row go together with the character they follow: one step back is enough.
    assert_eq!(rows("あいうえお」。かき", 10), ["あいうえ", "お」。かき"]);
    assert_eq!(rows("あいうえおかき」。くけ", 14), ["あいうえおか", "き」。くけ"]);
    // A long-vowel mark and an ellipsis too (the ellipsis is one column wide).
    assert_eq!(rows("コーヒーを飲むーー", 10), ["コーヒーを", "飲むーー"]);
    assert_eq!(rows("あいうえ…かき", 8), ["あいう", "え…かき"]);
}

#[test]
fn vis_7_an_opening_bracket_does_not_end_a_row() {
    assert_eq!(rows("あいうえ「かき", 10), ["あいうえ", "「かき"]);
    assert_eq!(rows("あいう（「えお", 10), ["あいう", "（「えお"]);
    assert_eq!(rows("あいうえお【かき", 12), ["あいうえお", "【かき"]);
}

#[test]
fn vis_7_every_character_of_both_sets_is_held_to_its_rule() {
    // "あいうえ" fills eight columns; the next character is where the rule matters.
    for c in NOT_START.chars() {
        let line = format!("あいうえ{c}おかき");
        let rs = rows(&line, 8);
        assert!(rs.iter().skip(1).all(|r| !r.starts_with(c)), "{c}: {rs:?}");
        assert_eq!(rs.concat(), line);
    }
    // The bracket is the last character that fits.
    for c in NOT_END.chars() {
        let wide = c.width() == Some(2);
        let line = format!("{}{c}おかき", if wide { "あいう" } else { "あいうえ" });
        let rs = rows(&line, 8);
        assert!(rs.iter().all(|r| !r.ends_with(c)), "{c}: {rs:?}");
        assert_eq!(rs.concat(), line);
    }
}

#[test]
fn vis_7_a_full_width_character_counts_as_two_columns() {
    assert_eq!((cols("あ"), cols("a"), cols("ａ"), cols("ア"), cols("、")), (2, 1, 2, 2, 2));
    // Five columns hold two full-width characters, or five ASCII ones.
    assert_eq!(rows("あいうえお", 5), ["あい", "うえ", "お"]);
    assert_eq!(rows("abcdefghij", 5), ["abcde", "fghij"]);
    assert_eq!(rows("aあいうえ", 5), ["aあい", "うえ"]);
}

#[test]
fn vis_7_a_row_that_cannot_satisfy_the_rule_breaks_at_the_edge() {
    // Nothing but marks that cannot start a row: there is no better place than the edge.
    assert_eq!(rows(&"、".repeat(13), 10), ["、、、、、", "、、、、、", "、、、"]);
    assert_eq!(rows(&"（".repeat(7), 6), ["（（（", "（（（", "（"]);
    assert_eq!(rows("、。，．、。", 8), ["、。，．", "、。"]);
    // It also happens when the only way out would be an empty row.
    for line in ["あ、、あ", "「「あ", "あ、「「、"] {
        let rs = rows(line, 4);
        assert_eq!(rs.concat(), line);
        assert!(rs.iter().all(|r| cols(r) <= 4), "{line}: {rs:?}");
    }
}

#[test]
fn vis_7_an_ascii_word_goes_down_whole_before_a_closing_mark() {
    // The break after the space is the one that keeps the mark off the start of the row.
    assert_eq!(rows("hello world。", 11), ["hello ", "world。"]);
    // Without a space to break at, the break moves back by the one character the rule needs.
    assert_eq!(rows("abcdefghij。", 10), ["abcdefghi", "j。"]);
}

#[test]
fn vis_7_the_screen_wraps_with_kinsoku() {
    // A 20-column screen has 15 for the text after the 5 of the gutter: 7 wide characters fit, and
    // the eighth, a full stop, would start the second row.
    let mut a = app_with(Some("f.txt"), "あいうえおかき。くけこ\n".as_bytes(), (20, 6));
    let f = screen(&mut a);
    let text = |i: usize| f.rows[i].text.chars().skip(5).collect::<String>();
    assert_eq!((text(0), text(1)), ("あいうえおか".to_string(), "き。くけこ".to_string()));
    assert!(f.rows[1].text.starts_with("     "), "the continuation row has no number");
    // The cursor sits on the character that moved down.
    feed(&mut a, "$");
    assert_eq!(screen(&mut a).cursor.1, 1);
}

/// The wrap of M1 to M4, kept as it was: text without kinsoku characters must wrap exactly as before.
fn wrap_before_kinsoku(cs: &[char], max: usize) -> Vec<usize> {
    let wide = |c: char| c.width() == Some(2);
    let (mut starts, mut s, mut i, mut col, mut brk) = (vec![0], 0, 0, 0, 0);
    while i < cs.len() {
        let w = width(cs, i, col);
        if col + w > max && i > s {
            s = if brk > s { brk } else { i };
            starts.push(s);
            (i, col, brk) = (s, 0, 0);
            continue;
        }
        col += w;
        i += 1;
        if matches!(cs[i - 1], ' ' | '\t') || (i < cs.len() && wide(cs[i - 1]) && wide(cs[i])) {
            brk = i;
        }
    }
    starts
}

/// A piece of text (CJK, an ASCII word, a blank, an emoji, ASCII punctuation) followed by up to
/// `runs` characters from the two kinsoku sets.
fn unit(runs: usize) -> impl Strategy<Value = String> {
    let plain = prop_oneof![
        4 => "[あ-んア-ン一-龥]{1,4}",
        4 => "[a-zA-Z0-9]{1,9}",
        3 => Just(" ".to_string()),
        1 => Just("\t".to_string()),
        1 => prop::sample::select(vec!["😀", "👨‍👩‍👧", "🎉", "❤\u{fe0f}"]).prop_map(String::from),
        1 => prop::sample::select(vec![",", ".", "!", "(", ")", "-", "ａ", "ＡＢ"]).prop_map(String::from),
    ];
    let marks: Vec<char> = NOT_START.chars().chain(NOT_END.chars()).collect();
    let after = prop::collection::vec(prop::sample::select(marks), 0..=runs);
    (plain, after).prop_map(|(p, m)| p + &m.into_iter().collect::<String>())
}

proptest! {
    #![proptest_config(ProptestConfig { cases: cases(2000), failure_persistence: None, ..ProptestConfig::default() })]

    /// Every row fits, the rows put together are the line, and no row starts with a character that
    /// cannot start one or ends with one that cannot end one, unless the row is nothing but such
    /// characters. (The text may start with one, and end with an opening bracket: nothing can help
    /// that.) Runs of those characters are at most 2 long and the rows at least 16 columns wide,
    /// where the rule can always be kept; longer runs in narrower rows are the test above.
    #[test]
    fn vis_7_prop_rows_fit_rejoin_and_keep_kinsoku(
        units in prop::collection::vec(unit(2), 0..40),
        lead in prop::collection::vec(prop::sample::select(NOT_START.chars().collect::<Vec<_>>()), 0..=2),
        max in 16usize..40,
    ) {
        let line = lead.into_iter().collect::<String>() + &units.concat();
        let cs: Vec<char> = line.chars().collect();
        let starts = wrap(&cs, max);
        prop_assert_eq!(starts[0], 0);
        prop_assert!(starts.windows(2).all(|w| w[0] < w[1]) && starts.last().is_none_or(|&s| s <= cs.len()));
        let marks = |c: &char| NOT_START.contains(*c) || NOT_END.contains(*c);
        let mut joined = String::new();
        for (k, &s) in starts.iter().enumerate() {
            let e = starts.get(k + 1).copied().unwrap_or(cs.len());
            let row = &cs[s..e];
            joined.extend(row);
            let text: String = row.iter().collect();
            prop_assert!(cols(&text) <= max || row.len() == 1, "row {k} is {} columns: {text:?}", cols(&text));
            if row.is_empty() || row.iter().all(marks) {
                continue;
            }
            prop_assert!(k == 0 || !NOT_START.contains(row[0]), "row {k} starts with {:?}: {line:?} at {max}", row[0]);
            prop_assert!(k + 1 == starts.len() || !NOT_END.contains(row[row.len() - 1]), "row {k} ends with {:?}: {line:?} at {max}", row[row.len() - 1]);
        }
        prop_assert_eq!(joined, line);
    }

    /// Wrapping never panics and never loops, whatever the text and however narrow the screen: the
    /// rows are not empty, they follow one another, and each fits unless it is one wide character
    /// (a screen narrower than a wide character is tests/view.rs and the fuzz of VIS-6).
    #[test]
    fn vis_7_prop_wrap_is_total_on_any_text(
        cs in prop::collection::vec(
            prop::sample::select(vec![
                'a', ' ', '\t', 'あ', '、', '（', '」', '\u{200d}', '\u{fe0f}', '😀', '\x1b', '\u{202e}', 'ー', '…',
            ]),
            0..60,
        ),
        max in 2usize..30,
    ) {
        let starts = wrap(&cs, max);
        prop_assert_eq!(starts[0], 0);
        prop_assert!(starts.windows(2).all(|w| w[0] < w[1]) && starts.last().is_none_or(|&s| s < cs.len().max(1)));
        for (k, &s) in starts.iter().enumerate() {
            let e = starts.get(k + 1).copied().unwrap_or(cs.len());
            let text: String = cs[s..e].iter().collect();
            prop_assert!(cols(&text) <= max || e - s == 1, "row {k} is {} columns at {max}: {text:?}", cols(&text));
        }
    }

    /// Text without a character of either set wraps as it did before kinsoku.
    #[test]
    fn vis_7_prop_text_without_marks_wraps_as_before(
        units in prop::collection::vec(unit(0), 0..40),
        max in 2usize..40,
    ) {
        let cs: Vec<char> = units.concat().chars().collect();
        prop_assert_eq!(wrap(&cs, max), wrap_before_kinsoku(&cs, max));
    }
}
