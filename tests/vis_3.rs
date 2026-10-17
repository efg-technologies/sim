//! VIS-2, VIS-3 on the screen: the gutter symbols `+` `~` `-`, the change style on changed words,
//! and what a hunk looks like outside REVIEW (REVIEW itself is tests/review.rs). Goldens show one
//! symbol per cell: `.` primary, `-` secondary, `#` change, `_` underline.
mod common;
use common::*;
use si::core::app::{App, Event};
use si::core::view::{Frame, Style};

fn named(path: &str, text: &str, size: (u16, u16)) -> App {
    app_with(Some(path), text.as_bytes(), size)
}

fn disk(a: &mut App, text: &str) {
    a.update(Event::Disk {
        bytes: Some(text.as_bytes().to_vec()),
        hash: None,
        deadline: far(),
        seq: 0,
    });
}

/// The columns of row `r` painted with the change style.
fn changed(f: &Frame, r: usize) -> Vec<u16> {
    let mut cols: Vec<u16> =
        f.rows[r].spans.iter().filter(|s| s.1 == Style::Change).flat_map(|s| s.0.clone()).collect();
    cols.sort();
    cols
}

#[test]
fn vis_3_the_gutter_shows_added_changed_and_deleted_lines_golden() {
    let mut a = named("f.rs", "alpha\nbeta gamma\ndelta\nepsilon\nzeta\neta\n", (30, 9));
    disk(&mut a, "alpha\nbeta GAMMA\ndelta\nzeta\neta\ntheta\niota\n");
    let f = screen(&mut a);
    golden("vis_3_gutter_symbols", &dump(&f));
    let symbols: Vec<char> = (0..6).map(|r| f.rows[r].text.chars().next().unwrap()).collect();
    assert_eq!(symbols, [' ', '~', ' ', '-', ' ', '+']);
}

#[test]
fn vis_3_a_mark_goes_when_the_cursor_enters_its_line() {
    let mut a = named("f.rs", "a\nb\nc\n", (30, 6));
    disk(&mut a, "a\nB\nc\nd\n");
    let first = |f: &Frame, r: usize| f.rows[r].text.chars().next().unwrap();
    let f = screen(&mut a);
    assert_eq!((first(&f, 1), first(&f, 3)), ('~', '+'));
    feed(&mut a, "j");
    let f = screen(&mut a);
    assert_eq!((first(&f, 1), first(&f, 3)), (' ', '+'), "the line the cursor is on shows nothing");
}

#[test]
fn vis_3_a_deletion_at_the_end_marks_the_last_line() {
    let mut a = named("f.rs", "a\nb\nc\n", (30, 6));
    disk(&mut a, "a\nb\n");
    let f = screen(&mut a);
    assert_eq!(
        f.rows[1].text.chars().next(),
        Some('-'),
        "the line before the deletion: the last line"
    );
    assert_eq!(a.buf.marks.len(), 1);
}

#[test]
fn vis_2_changed_words_are_painted_with_the_change_style_and_the_rest_is_not() {
    let mut a = named("f.rs", "let x = foo(1, 2);\nplain\n", (40, 6));
    disk(&mut a, "let x = bar(1, 22);\nplain\n");
    let f = screen(&mut a);
    // Text starts at column 5: `bar` is columns 13..16 and `22` is 20..22.
    let text = &f.rows[0].text;
    assert_eq!(text, "~  1 let x = bar(1, 22);");
    assert_eq!(changed(&f, 0), [0, 13, 14, 15, 20, 21], "the marker, `bar`, and `22`");
    assert!(changed(&f, 1).is_empty(), "an unchanged line has nothing");
}

#[test]
fn vis_2_an_added_line_is_painted_all_through() {
    let mut a = named("f.rs", "a\nb\n", (30, 6));
    disk(&mut a, "a\nnew line\nb\n");
    let f = screen(&mut a);
    assert_eq!(changed(&f, 1), [0, 5, 6, 7, 8, 9, 10, 11, 12], "the marker and every char");
}

#[test]
fn vis_2_wide_and_wrapped_text_is_painted_where_it_is() {
    let mut a = named("f.rs", "日本語です\n", (12, 6));
    disk(&mut a, "日本人です\n");
    let f = screen(&mut a);
    assert_eq!(changed(&f, 0), [0, 9, 10], "the third char is two columns wide");
    // A changed word that wraps onto a second row is painted there too.
    let mut a = named("f.rs", "aaaa bbbb cccc\n", (12, 6));
    disk(&mut a, "aaaa bbbb dddd\n");
    let f = screen(&mut a);
    assert_eq!(f.rows[2].text.trim(), "dddd");
    assert_eq!(changed(&f, 2), [5, 6, 7, 8]);
    assert_eq!(
        (changed(&f, 0), changed(&f, 1)),
        (vec![0], vec![]),
        "only the marker on the first row"
    );
}

#[test]
fn vis_3_a_conflict_marks_the_whole_line_of_mine_with_a_tilde_golden() {
    let mut a = named("f.rs", "one\ntwo words here\nthree\n", (30, 7));
    feed(&mut a, "jwcwMINE\x1bgg");
    disk(&mut a, "one\ntwo THEIRS here\nthree\n");
    let f = screen(&mut a);
    golden("vis_3_conflict_marks_the_line", &dump(&f));
    assert_eq!(f.rows[1].text, "~  2 two MINE here");
    assert_eq!(
        changed(&f, 1),
        (0..18).filter(|c| !(1..5).contains(c)).collect::<Vec<u16>>(),
        "the marker and the whole line: the disk's words are not in the buffer to be told apart"
    );
    assert!(changed(&f, 0).is_empty() && changed(&f, 2).is_empty());
}

#[test]
fn vis_3_a_conflict_where_mine_has_no_line_marks_the_line_after_it() {
    let mut a = named("f.rs", "one\ntwo\nthree\n", (30, 7));
    feed(&mut a, "jddgg"); // mine drops line 2 while the disk changes it
    disk(&mut a, "one\ntwo!\nthree\n");
    let f = screen(&mut a);
    assert_eq!(f.rows[1].text.chars().next(), Some('-'));
    assert_eq!(changed(&f, 1), [0], "just the marker: there is nothing of mine to paint");
}

#[test]
fn vis_9_the_status_line_counts_changes_on_disk_and_says_when_one_conflicts() {
    let mut a = named("f", "a\nb\nc\nd\ne\n", (100, 4));
    assert_eq!(status(&screen(&mut a)), "f  1:1  NORMAL");
    disk(&mut a, "a\nB\nc\nd\nE\n");
    assert_eq!(status(&screen(&mut a)), "f  1:1  NORMAL  2 changes on disk");
    disk(&mut a, "a\nB\nc\nd\nE\nF\n");
    assert_eq!(
        status(&screen(&mut a)),
        "f  1:1  NORMAL  2 changes on disk",
        "adjacent lines are one change"
    );
    feed(&mut a, "jjrXjjrY");
    assert_eq!(status(&screen(&mut a)), "f  5:1  NORMAL  modified · 1 change on disk");
    let mut a = named("f", "a\nb\n", (100, 4));
    feed(&mut a, "rX");
    disk(&mut a, "A\nb\n");
    assert_eq!(
        status(&screen(&mut a)),
        "f  1:1  NORMAL  modified · 1 change on disk · conflicts with the disk"
    );
    a.update(Event::Disk { bytes: None, hash: None, deadline: far(), seq: 0 });
    assert_eq!(
        status(&screen(&mut a)),
        "f  1:1  NORMAL  modified · deleted on disk · 1 change on disk · conflicts with the disk"
    );
}

#[test]
fn safe_8_a_hidden_char_in_a_changed_word_is_hidden_all_the_same_and_the_change_stays_beside_it() {
    // someone else writes a right-to-left override into a word of the file: the Trojan Source case
    let mut a = named("f.rs", "alpha\nbeta gamma\ndelta\n", (30, 6));
    disk(&mut a, "alpha\nbeta GA\u{202e}MMA\ndelta\n");
    let f = screen(&mut a);
    let row = &f.rows[1];
    let at = row.text.find("<U+202E>").unwrap() as u16;
    let hidden: Vec<_> =
        row.spans.iter().filter(|s| s.1 == Style::Hidden).map(|s| s.0.clone()).collect();
    assert_eq!(hidden, vec![at..at + 8], "{row:?}: Hidden wins over Change");
    let change = changed(&f, 1);
    assert!(
        change.contains(&(at - 1)) && change.contains(&(at + 8)),
        "the words beside it: {change:?}"
    );
    assert!(!change.iter().any(|c| (at..at + 8).contains(c)), "and none under it");
    assert!(change.contains(&0), "the gutter's symbol");
}
