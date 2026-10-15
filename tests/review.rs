//! PROP-8, SAFE-7, VIS-3, VIS-10 on App: `]c` and `[c`, REVIEW mode and its keys, what accepting
//! and rejecting leave behind, the refusals that keep an open hunk from being lost, and how REVIEW
//! looks. Goldens show one symbol per cell: `.` primary, `-` secondary, `#` change, `x` deleted.
#![allow(clippy::unwrap_used, clippy::single_range_in_vec_init)]
mod common;
use common::*;
use proptest::prelude::*;
use si::core::app::{App, Effect, Event};
use si::core::buffer::hash;
use si::core::vi::Mode;
use si::core::view::{Shape, Style};
use std::sync::Arc;

const KEYS: &str = "y accept · n reject · Y all · N none · Esc back";
const CONFLICTS: &str = "unresolved conflicts with the disk: ]c to review, :w! keeps yours";
const PROPOSALS: &str = "unresolved proposals: ]c to review, :q! discards";

fn disk(a: &mut App, text: &str) -> Vec<Effect> {
    a.update(Event::Disk {
        bytes: Some(text.as_bytes().to_vec()),
        hash: None,
        deadline: far(),
        seq: 0,
    })
}

/// Mine is "A1 b A2 c A3" and the file says "X1 b X2 c X3" (it was "a1 b a2 c a3"): three hunks,
/// one line each, with unchanged lines between them. The cursor is on the first line.
fn three() -> App {
    let mut a = app_with(Some("f"), b"a1\nb\na2\nc\na3\n", (80, 14));
    feed(&mut a, "rAjjrAjjrA");
    disk(&mut a, "X1\nb\nX2\nc\nX3\n");
    feed(&mut a, "gg");
    assert_eq!(a.buf.hunks.len(), 3);
    a
}

fn st(a: &mut App) -> String {
    status(&screen(a)).to_string()
}

fn line(a: &App) -> usize {
    a.buf.line_of(a.buf.cursor)
}

/// The lines of mine of every hunk.
fn mines(a: &App) -> Vec<(usize, usize)> {
    a.buf.hunks.iter().map(|h| (h.mine.start, h.mine.end)).collect()
}

fn writes(fx: &[Effect]) -> usize {
    fx.iter().filter(|e| matches!(e, Effect::Write { .. })).count()
}

fn quit(fx: &[Effect]) -> Option<i32> {
    fx.iter().find_map(|e| if let Effect::Quit(c) = e { Some(*c) } else { None })
}

// ---------------------------------------------------------------------------------------------
// ]c and [c
// ---------------------------------------------------------------------------------------------

#[test]
fn prop_8_bracket_c_goes_to_the_next_and_previous_hunk_and_stops_at_the_ends() {
    let mut a = three();
    for want in [0, 2, 4, 4] {
        feed(&mut a, "]c");
        assert_eq!(line(&a), want, "]c");
        assert_eq!(a.vi.mode, Mode::Review);
    }
    assert_eq!(st(&mut a), "no next change", "the last hunk is the last: REVIEW stays on it");
    assert_eq!(a.vi.mode, Mode::Review);
    for want in [2, 0, 0] {
        feed(&mut a, "[c");
        assert_eq!(line(&a), want, "[c");
    }
    assert_eq!(st(&mut a), "no previous change");
}

#[test]
fn prop_8_without_changes_bracket_c_says_so() {
    let mut a = app_with(Some("f"), b"a\nb\n", (60, 8));
    for (k, said) in [("]c", "no next change"), ("[c", "no previous change")] {
        feed(&mut a, k);
        assert_eq!(st(&mut a), said, "{k}");
        assert_eq!((a.vi.mode, line(&a)), (Mode::Normal, 0));
    }
}

#[test]
fn prop_8_bracket_c_puts_the_cursor_on_the_first_non_blank_and_centers() {
    let text: String = (0..100).map(|i| format!("    line {i}\n")).collect();
    let mut a = app_with(Some("f.rs"), text.as_bytes(), (60, 24));
    feed(&mut a, "60Grx");
    let mut changed = text.replace("    line 59", "    LINE 59");
    changed = changed.replace("    line 59", "    X");
    disk(&mut a, &changed);
    feed(&mut a, "gg]c");
    assert_eq!((line(&a), a.buf.col(), a.vi.mode), (59, 4, Mode::Review));
    assert_eq!(screen(&mut a).cursor.1, 11, "centered, as every jump is (VIS-12)");
    feed(&mut a, "G[c");
    assert_eq!(line(&a), 59);
}

#[test]
fn prop_8_marks_and_hunks_are_visited_in_line_order_and_only_a_hunk_starts_review() {
    let mut a = app_with(Some("f"), b"l1\nl2\nl3\nl4\nl5\nl6\nl7\n", (60, 12));
    feed(&mut a, "3jrX");
    disk(&mut a, "l1\nm2\nl3\nY4\nl5\nm6\nl7\n");
    feed(&mut a, "gg");
    assert_eq!((a.buf.marks.len(), a.buf.hunks.len()), (2, 1));
    // The mark on line 2, the hunk on line 4 (REVIEW), the mark on line 6 (leaves REVIEW).
    for (want, mode) in [(1, Mode::Normal), (3, Mode::Review), (5, Mode::Normal)] {
        feed(&mut a, "]c");
        assert_eq!((line(&a), a.vi.mode), (want, mode));
    }
    assert!(a.buf.marks.is_empty(), "the cursor saw both marks");
    feed(&mut a, "[c");
    assert_eq!((line(&a), a.vi.mode), (3, Mode::Review), "back to the hunk");
    feed(&mut a, "[c");
    assert_eq!(st(&mut a), "no previous change", "the only one");
}

#[test]
fn prop_8_a_hunk_on_the_cursors_line_is_the_next_one_in_normal_mode() {
    let mut a = three();
    assert_eq!(line(&a), 0);
    feed(&mut a, "]c");
    assert_eq!((line(&a), a.vi.mode), (0, Mode::Review), "nothing has decided the hunk under us");
    feed(&mut a, "<Esc>]c");
    assert_eq!((line(&a), a.vi.mode), (0, Mode::Review), "after Esc too");
    feed(&mut a, "<Esc>[c");
    assert_eq!((line(&a), st(&mut a)), (0, "no previous change".to_string()), "[c goes before it");
    feed(&mut a, "]c]c<Esc>[c");
    assert_eq!(line(&a), 0, "from the second hunk, back to the first");
}

#[test]
fn prop_8_inside_review_bracket_c_moves_on_without_deciding_and_a_mark_ends_review() {
    let mut a = three();
    feed(&mut a, "]c]c");
    assert_eq!(st(&mut a), format!("f  3:1  REVIEW 2/3  {KEYS}"));
    assert_eq!(text(&a), "A1\nb\nA2\nc\nA3");
    assert_eq!(a.buf.hunks.len(), 3, "nothing was decided");
    feed(&mut a, "[c");
    assert_eq!(st(&mut a), format!("f  1:1  REVIEW 1/3  {KEYS}"));
    // A change of the file lands on a line between two hunks.
    disk(&mut a, "X1\nB\nX2\nc\nX3\n");
    assert_eq!(a.buf.marks.len(), 1);
    feed(&mut a, "]c");
    assert_eq!((line(&a), a.vi.mode), (1, Mode::Normal), "landing on a mark leaves REVIEW");
    assert_eq!(st(&mut a).split_whitespace().nth(2), Some("NORMAL"));
}

#[test]
fn prop_8_a_bracket_without_its_c_does_nothing_and_review_stays() {
    let mut a = three();
    feed(&mut a, "]c]x[<Esc>");
    assert_eq!((a.vi.mode, line(&a), a.buf.hunks.len()), (Mode::Review, 0, 3));
    assert_eq!(st(&mut a), format!("f  1:1  REVIEW 1/3  {KEYS}"), "also while a ] waits for its c");
    feed(&mut a, "]");
    assert_eq!(st(&mut a), format!("f  1:1  REVIEW 1/3  {KEYS}"));
    feed(&mut a, "c");
    assert_eq!(line(&a), 2);
}

// ---------------------------------------------------------------------------------------------
// REVIEW: the status line, the cursor, and the keys
// ---------------------------------------------------------------------------------------------

#[test]
fn prop_8_review_shows_its_keys_on_the_status_line_cut_to_the_width() {
    let mut a = three();
    feed(&mut a, "]c");
    assert_eq!(st(&mut a), format!("f  1:1  REVIEW 1/3  {KEYS}"));
    for w in [20, 33, 40, 66] {
        a.update(Event::Resize(w, 14));
        let s = st(&mut a);
        let full = format!("f  1:1  REVIEW 1/3  {KEYS}");
        assert_eq!(s, full.chars().take(w as usize).collect::<String>(), "width {w}");
    }
    // The path and the position come first and say where we are.
    a.update(Event::Resize(80, 14));
    feed(&mut a, "]c]c");
    assert_eq!(st(&mut a), format!("f  5:1  REVIEW 3/3  {KEYS}"));
}

#[test]
fn vis_10_review_has_a_steady_underline_cursor() {
    let mut a = three();
    assert_eq!(screen(&mut a).shape, Shape::Block);
    feed(&mut a, "]c");
    assert_eq!(screen(&mut a).shape, Shape::Underline);
    feed(&mut a, "<Esc>");
    assert_eq!(screen(&mut a).shape, Shape::Block);
}

#[test]
fn prop_8_y_accepts_and_n_rejects_and_both_move_on_to_the_next_hunk() {
    let mut a = three();
    feed(&mut a, "]c");
    feed(&mut a, "y");
    assert_eq!(text(&a), "X1\nb\nA2\nc\nA3", "the hunk's lines of mine are replaced by theirs");
    assert_eq!((mines(&a), line(&a)), (vec![(2, 3), (4, 5)], 2));
    assert_eq!(st(&mut a), format!("f  3:1  REVIEW 1/2  {KEYS}"));
    feed(&mut a, "n");
    assert_eq!(text(&a), "X1\nb\nA2\nc\nA3", "a reject keeps mine");
    assert_eq!((mines(&a), line(&a)), (vec![(4, 5)], 4));
    assert_eq!(st(&mut a), format!("f  5:1  REVIEW 1/1  {KEYS}"));
    feed(&mut a, "y");
    assert_eq!(text(&a), "X1\nb\nA2\nc\nX3");
    assert_eq!((a.vi.mode, a.buf.hunks.len()), (Mode::Normal, 0), "REVIEW ends with the last hunk");
    assert_eq!(line(&a), 4);
    assert_eq!(st(&mut a), "1 proposal accepted");
    assert!(a.buf.dirty());
}

#[test]
fn prop_8_after_the_last_hunk_the_next_is_the_first_one_left() {
    let mut a = three();
    feed(&mut a, "]c]c]c");
    assert_eq!(line(&a), 4);
    feed(&mut a, "n");
    assert_eq!((line(&a), mines(&a)), (0, vec![(0, 1), (2, 3)]), "on to the first hunk left");
    assert_eq!(st(&mut a), format!("f  1:1  REVIEW 1/2  {KEYS}"));
    feed(&mut a, "]cy");
    assert_eq!(text(&a), "A1\nb\nX2\nc\nA3");
    assert_eq!((line(&a), mines(&a)), (0, vec![(0, 1)]));
}

#[test]
fn prop_8_a_decision_acts_on_the_hunk_review_is_on_not_the_first() {
    let mut a = three();
    feed(&mut a, "]c]cy");
    assert_eq!(text(&a), "A1\nb\nX2\nc\nA3");
    assert_eq!(mines(&a), vec![(0, 1), (4, 5)]);
}

#[test]
fn prop_8_capital_y_accepts_all_and_capital_n_rejects_all_as_one_undo_unit_each() {
    let mut a = three();
    feed(&mut a, "]cY");
    assert_eq!(text(&a), "X1\nb\nX2\nc\nX3");
    assert_eq!((a.vi.mode, a.buf.hunks.len()), (Mode::Normal, 0));
    assert_eq!(st(&mut a), "3 proposals accepted");
    feed(&mut a, "u");
    assert_eq!((text(&a), a.buf.hunks.len()), ("A1\nb\nA2\nc\nA3".to_string(), 3), "one u");
    feed(&mut a, "<C-r>");
    assert_eq!((text(&a), a.buf.hunks.len()), ("X1\nb\nX2\nc\nX3".to_string(), 0));
    feed(&mut a, "u");
    assert_eq!(a.buf.hunks.len(), 3, "the Y again");
    feed(&mut a, "u");
    assert_eq!(
        (text(&a), a.buf.hunks.len()),
        ("A1\nb\nA2\nc\nA3".to_string(), 0),
        "then the merge"
    );
    let mut a = three();
    feed(&mut a, "]c]cN");
    assert_eq!(text(&a), "A1\nb\nA2\nc\nA3");
    assert_eq!((a.vi.mode, a.buf.hunks.len()), (Mode::Normal, 0));
    assert_eq!(st(&mut a), "3 proposals rejected");
    feed(&mut a, "u");
    assert_eq!((a.buf.hunks.len(), text(&a)), (3, "A1\nb\nA2\nc\nA3".to_string()));
}

#[test]
fn prop_8_each_accept_and_each_reject_is_one_undo_unit() {
    let mut a = three();
    feed(&mut a, "]cy");
    feed(&mut a, "u");
    assert_eq!(
        (a.vi.mode, text(&a), a.buf.hunks.len()),
        (Mode::Normal, "A1\nb\nA2\nc\nA3".into(), 3)
    );
    assert_eq!(line(&a), 0, "the cursor is back on the hunk");
    feed(&mut a, "]cn");
    assert_eq!((text(&a), a.buf.hunks.len()), ("A1\nb\nA2\nc\nA3".to_string(), 2));
    feed(&mut a, "u");
    assert_eq!(a.buf.hunks.len(), 3, "undo brings a rejected hunk back");
    assert_eq!(mines(&a), vec![(0, 1), (2, 3), (4, 5)]);
    feed(&mut a, "<C-r>");
    assert_eq!(a.buf.hunks.len(), 2, "and redo rejects it again");
    // Several decisions are several units.
    feed(&mut a, "]cyn");
    assert_eq!(a.buf.hunks.len(), 0);
    feed(&mut a, "u");
    assert_eq!(a.buf.hunks.len(), 1);
    feed(&mut a, "u");
    assert_eq!(a.buf.hunks.len(), 2);
}

#[test]
fn prop_8_a_reject_changes_no_text_so_a_clean_buffer_stays_clean() {
    let mut a = app_with(Some("f"), b"a\nb\n", (80, 8));
    a.buf.hunks = Arc::new(vec![proposal(1..2, "B\n")]); // what a filter would propose (M4)
    assert!(!a.buf.dirty());
    feed(&mut a, "]cn");
    assert_eq!(a.buf.hunks.len(), 0);
    assert!(!a.buf.dirty(), "nothing changed in the text");
    feed(&mut a, "u");
    assert_eq!(a.buf.hunks.len(), 1);
    assert!(!a.buf.dirty());
    feed(&mut a, "]cy");
    assert_eq!(text(&a), "a\nB");
    assert!(a.buf.dirty(), "an accept does");
    feed(&mut a, "u");
    assert!(!a.buf.dirty());
}

#[test]
fn prop_8_any_other_key_leaves_review_and_works_as_in_normal_mode() {
    // From the hunk on line 3: the key's own effect happens, in Normal mode.
    for (keys, mode, at) in [
        ("j", Mode::Normal, 3),
        ("k", Mode::Normal, 1),
        ("G", Mode::Normal, 4),
        ("gg", Mode::Normal, 0),
        ("v", Mode::Visual, 2),
        ("V", Mode::VLine, 2),
        ("i", Mode::Insert, 2),
        ("o", Mode::Insert, 3),
        (":", Mode::Cmd, 2),
        ("/", Mode::Cmd, 2),
        ("<Esc>", Mode::Normal, 2),
        ("<C-c>", Mode::Normal, 2),
        ("<Down>", Mode::Normal, 3),
    ] {
        let mut a = three();
        feed(&mut a, "]c]c");
        feed(&mut a, keys);
        assert_eq!((a.vi.mode, line(&a)), (mode, at), "{keys}");
        assert_eq!(a.buf.hunks.len(), 3, "{keys} decides nothing");
    }
    let mut a = three();
    feed(&mut a, "]cx");
    assert_eq!((a.vi.mode, text(&a)), (Mode::Normal, "1\nb\nA2\nc\nA3".to_string()), "x deletes");
    assert_eq!(a.buf.hunks.len(), 3, "and decides nothing");
    feed(&mut a, "u");
    assert_eq!(text(&a), "A1\nb\nA2\nc\nA3", "u undoes the x");
    let mut a = three();
    feed(&mut a, "]c3");
    assert_eq!(a.vi.mode, Mode::Normal, "a count is a key like another");
    feed(&mut a, "j");
    assert_eq!(line(&a), 3);
    let mut a = three();
    feed(&mut a, "]cyyj");
    assert_eq!(a.vi.mode, Mode::Normal, "y y in REVIEW is two decisions, not a yank");
}

#[test]
fn prop_8_a_paste_ends_review_and_is_text() {
    let mut a = three();
    feed(&mut a, "]c");
    a.update(Event::Paste("yyy".into()));
    assert_eq!(a.vi.mode, Mode::Normal);
    assert_eq!((text(&a), a.buf.hunks.len()), ("yyyA1\nb\nA2\nc\nA3".to_string(), 3));
}

#[test]
fn prop_8_review_ends_when_something_else_settles_the_hunks() {
    // `:w!` is answered while REVIEW is open: the conflicts with the disk are settled.
    let mut a = three();
    feed(&mut a, "]c:w!\n");
    assert_eq!(a.vi.mode, Mode::Normal);
    let mut a = three();
    feed(&mut a, "]c<C-s>");
    assert_eq!(a.vi.mode, Mode::Review, "Ctrl-s is refused: REVIEW stays");
    assert_eq!(st(&mut a), CONFLICTS);
    feed(&mut a, "n");
    assert_eq!(a.buf.hunks.len(), 2, "and its keys work as before");
    let w = write_of(&feed(&mut a, ":w!\n")).unwrap();
    let h = hash(&w);
    a.update(Event::Saved(Ok((h, None)), 0));
    assert_eq!((a.vi.mode, a.buf.hunks.len()), (Mode::Normal, 0));
}

fn write_of(fx: &[Effect]) -> Option<Vec<u8>> {
    fx.iter()
        .find_map(|e| if let Effect::Write { bytes, .. } = e { Some(bytes.clone()) } else { None })
}

fn proposal(mine: std::ops::Range<usize>, theirs: &str) -> si::core::buffer::Hunk {
    si::core::buffer::Hunk { disk: false, ..hunk(mine, theirs, None) }
}

// ---------------------------------------------------------------------------------------------
// Accepting puts theirs in place of the lines of mine, wherever they are
// ---------------------------------------------------------------------------------------------

/// The text of `before` after `]c y` on one hunk over lines `mine`.
fn accepted(before: &str, mine: std::ops::Range<usize>, theirs: &str) -> String {
    let mut a = app_with(Some("f"), before.as_bytes(), (80, 10));
    a.buf.hunks = Arc::new(vec![proposal(mine, theirs)]);
    feed(&mut a, "]cy");
    assert_eq!(a.buf.hunks.len(), 0);
    text(&a)
}

#[test]
fn prop_8_accepting_replaces_exactly_the_lines_of_the_hunk() {
    let abc = "a\nb\nc\n";
    for (mine, theirs, want) in [
        (0..1, "X\n", "X\nb\nc"),
        (1..2, "X\nY\n", "a\nX\nY\nc"),
        (2..3, "X\n", "a\nb\nX"),
        (2..3, "X\nY\n", "a\nb\nX\nY"),
        (0..3, "X\n", "X"),
        (0..3, "", ""),
        (1..2, "", "a\nc"),
        (2..3, "", "a\nb"),
        (0..1, "", "b\nc"),
        (1..3, "", "a"),
        (1..3, "X\n", "a\nX"),
        (1..1, "X\n", "a\nX\nb\nc"),
        (0..0, "X\n", "X\na\nb\nc"),
        (3..3, "X\n", "a\nb\nc\nX"),
        (3..3, "X\nY\n", "a\nb\nc\nX\nY"),
        (3..3, "", "a\nb\nc"),
        (1..2, "\n", "a\n\nc"),
        (2..3, "\n", "a\nb\n"),
        (3..3, "\n", "a\nb\nc\n"),
        (0..3, "\n\n", "\n"),
        (1..2, "X\nY\nZ\n", "a\nX\nY\nZ\nc"),
    ] {
        assert_eq!(accepted(abc, mine.clone(), theirs), want, "{mine:?} -> {theirs:?}");
    }
    assert_eq!(accepted("", 0..1, "X\n"), "X");
    assert_eq!(accepted("", 0..0, "X\n"), "X\n");
    assert_eq!(accepted("", 1..1, "X\n"), "\nX");
    // The file keeps its final newline, or none.
    let mut a = app_with(Some("f"), b"a\nb\n", (80, 10));
    a.buf.hunks = Arc::new(vec![proposal(1..2, "X\nY\n")]);
    feed(&mut a, "]cy");
    assert_eq!(a.buf.encode(), b"a\nX\nY\n");
    let mut a = app_with(Some("f"), b"a\nb", (80, 10));
    a.buf.hunks = Arc::new(vec![proposal(1..2, "X\nY\n")]);
    feed(&mut a, "]cy");
    assert_eq!(a.buf.encode(), b"a\nX\nY");
}

#[test]
fn prop_8_accepting_leaves_the_other_hunks_and_marks_where_their_lines_went() {
    let mut a = app_with(Some("f"), b"a\nb\nc\nd\ne\nf\n", (80, 12));
    a.buf.hunks =
        Arc::new(vec![proposal(1..2, "X\nY\nZ\n"), proposal(3..4, "Q\n"), proposal(5..6, "")]);
    feed(&mut a, "]cy");
    assert_eq!(text(&a), "a\nX\nY\nZ\nc\nd\ne\nf");
    assert_eq!(mines(&a), vec![(5, 6), (7, 8)], "the lines after the accepted ones moved by two");
    feed(&mut a, "y");
    assert_eq!(text(&a), "a\nX\nY\nZ\nc\nQ\ne\nf");
    assert_eq!(mines(&a), vec![(7, 8)]);
    feed(&mut a, "y");
    assert_eq!(text(&a), "a\nX\nY\nZ\nc\nQ\ne");
    // An accepted hunk takes the marks on its lines with it (they are decided), others stay.
    let mut a = app_with(Some("f"), b"a\nb\nc\nd\n", (80, 12));
    feed(&mut a, "jrX");
    disk(&mut a, "a\nY\nc\nD\n");
    assert_eq!((a.buf.marks.len(), a.buf.hunks.len()), (1, 1));
    feed(&mut a, "gg]cy");
    assert_eq!(text(&a), "a\nY\nc\nD");
    assert_eq!(a.buf.marks.len(), 1, "the mark on the last line stays until the cursor sees it");
}

#[test]
fn prop_8_accepting_a_hunk_does_not_grow_the_hunk_that_ends_where_it_starts() {
    for decide in ["]c]cyy", "]cY"] {
        let mut a = app_with(Some("f"), b"a\nb\nc\n", (80, 10));
        a.buf.hunks = Arc::new(vec![proposal(0..2, "X\n"), proposal(2..2, "Y\n")]);
        feed(&mut a, decide);
        assert_eq!(
            text(&a),
            "X\nY\nc",
            "{decide}: the text of the first did not swallow the second"
        );
        let mut a = app_with(Some("f"), b"a\nb\nc\n", (80, 10));
        a.buf.hunks = Arc::new(vec![proposal(0..3, "X\n"), proposal(3..3, "\n")]);
        feed(&mut a, decide);
        assert_eq!(text(&a), "X\n", "{decide}: an empty line after the last one");
    }
    // The other way round, the second one first: the first stays where it was (nothing of it moves).
    let mut a = app_with(Some("f"), b"a\nb\nc\nd\n", (80, 10));
    a.buf.hunks =
        Arc::new(vec![proposal(0..1, "X\n"), proposal(1..2, "Y1\nY2\n"), proposal(3..3, "Z\n")]);
    feed(&mut a, "]c]cy");
    assert_eq!(text(&a), "a\nY1\nY2\nc\nd");
    assert_eq!(mines(&a), vec![(0, 1), (4, 4)], "the one before stays, the one after moves by one");
}

proptest! {
    #![proptest_config(ProptestConfig { failure_persistence: None, ..ProptestConfig::with_cases(cases(3000)) })]

    /// Accepting is a splice of lines, and undo gives the text and the hunk back.
    #[test]
    fn prop_8_fuzz_accept_is_a_splice_of_lines_and_undo_undoes_it(
        lines in prop::collection::vec(prop::sample::select(vec!["", "a", "bb", "c d"]), 1..7),
        s in 0usize..8,
        len in 0usize..8,
        theirs in prop::collection::vec(prop::sample::select(vec!["", "x", "yy"]), 0..4),
        nl in any::<bool>(),
    ) {
        let before = lines.join("\n") + if nl { "\n" } else { "" };
        let mut a = app_with(Some("f"), before.as_bytes(), (40, 8));
        let mine = a.buf.text.to_string();
        // (A text of empty lines may end in a line break that is not a line of the buffer.)
        let lines: Vec<&str> = mine.split('\n').collect();
        let n = lines.len();
        prop_assert_eq!(a.buf.lines(), n);
        let (s, e) = (s.min(n), (s + len).min(n));
        let their_text: String = theirs.iter().map(|l| format!("{l}\n")).collect();
        a.buf.hunks = Arc::new(vec![proposal(s..e, &their_text)]);
        feed(&mut a, "]cy");
        let mut want: Vec<&str> = lines[..s].to_vec();
        want.extend(&theirs);
        want.extend(&lines[e..]);
        if want.is_empty() {
            want.push("");
        }
        prop_assert_eq!(text(&a), want.join("\n"));
        prop_assert_eq!(a.buf.hunks.len(), 0);
        feed(&mut a, "u");
        prop_assert_eq!(text(&a), mine);
        prop_assert_eq!(a.buf.hunks.len(), 1);
        prop_assert_eq!((a.buf.hunks[0].mine.clone(), a.buf.hunks[0].theirs.to_string()), (s..e, their_text));
        prop_assert!(!a.buf.dirty());
    }
}

// ---------------------------------------------------------------------------------------------
// SAFE-7: an unresolved hunk is neither saved over nor quit away
// ---------------------------------------------------------------------------------------------

#[test]
fn safe_7_every_way_to_save_is_refused_while_a_conflict_with_the_disk_is_open() {
    for keys in [":w\n", ":wq\n", ":x\n", "ZZ", "<C-s>", ":w f\n", ":wq f\n", "]c<C-s>"] {
        let mut a = three();
        let fx = feed(&mut a, keys);
        assert_eq!((writes(&fx), quit(&fx)), (0, None), "{keys:?}");
        assert_eq!(st(&mut a), CONFLICTS, "{keys:?}");
        assert_eq!(a.buf.hunks.len(), 3);
    }
    // In Insert mode too, which stays Insert mode.
    let mut a = three();
    let fx = feed(&mut a, "ix<C-s>");
    assert_eq!((writes(&fx), a.vi.mode), (0, Mode::Insert));
    assert_eq!(st(&mut a), CONFLICTS);
}

#[test]
fn safe_7_saving_another_file_is_allowed_it_is_how_to_keep_mine() {
    let mut a = three();
    let fx = feed(&mut a, ":w g\n");
    assert_eq!(writes(&fx), 1);
    assert!(fx.iter().any(|e| matches!(e, Effect::Write { path, bytes, .. } if path == "g" && bytes == b"A1\nb\nA2\nc\nA3\n")));
    a.update(Event::Saved(Ok((0, None)), 0));
    assert_eq!(a.buf.hunks.len(), 3, "the conflicts are with f, and f was not written");
}

#[test]
fn safe_7_w_bang_saves_mine_and_drops_the_disk_hunks_once_it_is_saved() {
    let mut a = three();
    let fx = feed(&mut a, ":w!\n");
    let w = write_of(&fx).unwrap();
    assert!(matches!(
        fx.iter().find(|e| matches!(e, Effect::Write { .. })),
        Some(Effect::Write { expect_hash: None, .. })
    ));
    assert_eq!(w, b"A1\nb\nA2\nc\nA3\n", "mine, whatever the disk holds");
    assert_eq!(a.buf.hunks.len(), 3, "still there while the save is in flight");
    a.update(Event::Saved(Ok((hash(&w), None)), 0));
    assert_eq!(a.buf.hunks.len(), 0);
    assert!(!a.buf.dirty());
    assert_eq!(st(&mut a), "saved f");
    // Now a plain :w works, and ZZ quits.
    let fx = feed(&mut a, "ZZ");
    assert_eq!(quit(&fx), Some(0));
}

#[test]
fn safe_7_a_forced_save_that_fails_drops_nothing() {
    let mut a = three();
    feed(&mut a, ":w!\n");
    a.update(Event::Saved(Err("cannot save f: disk full".into()), 0));
    assert_eq!(a.buf.hunks.len(), 3);
    assert_eq!(st(&mut a), "cannot save f: disk full");
    let fx = feed(&mut a, ":w\n");
    assert_eq!((writes(&fx), st(&mut a)), (0, CONFLICTS.to_string()));
    // A plain save answered by a conflict is a merge, not a drop.
    let mut a = three();
    feed(&mut a, ":w!\n");
    a.update(Event::SaveConflict(Some(b"Z1\nb\nZ2\nc\nZ3\n".to_vec()), far(), 0));
    assert_eq!(a.buf.hunks.len(), 3, "the newer say of the disk replaced the older");
}

#[test]
fn safe_7_proposals_from_filters_stay_after_a_forced_save_and_do_not_stop_a_plain_one() {
    let mut a = app_with(Some("f"), b"a\nb\nc\n", (80, 8));
    feed(&mut a, "x");
    a.buf.hunks = Arc::new(vec![proposal(1..2, "B\n")]);
    let fx = feed(&mut a, ":w\n");
    let w = write_of(&fx).unwrap();
    a.update(Event::Saved(Ok((hash(&w), None)), 0));
    assert_eq!(a.buf.hunks.len(), 1, "a save of mine does not decide a filter's proposal");
    feed(&mut a, "x");
    let fx = feed(&mut a, ":w!\n");
    let w = write_of(&fx).unwrap();
    a.update(Event::Saved(Ok((hash(&w), None)), 0));
    assert_eq!(a.buf.hunks.len(), 1);
    // Disk and filter hunks together: :w! settles the first kind only.
    let mut a = three();
    a.buf.hunks = Arc::new([&a.buf.hunks[..], &[proposal(1..2, "B\n")]].concat());
    let fx = feed(&mut a, ":w!\n");
    let w = write_of(&fx).unwrap();
    a.update(Event::Saved(Ok((hash(&w), None)), 0));
    assert_eq!(a.buf.hunks.iter().map(|h| h.disk).collect::<Vec<_>>(), [false]);
}

#[test]
fn safe_7_quitting_is_refused_while_any_proposal_is_open() {
    for keys in [":q\n", "q", ":x\n", ":wq\n", "ZZ"] {
        let mut a = app_with(Some("f"), b"a\nb\n", (80, 8)); // clean, with a filter's proposal
        a.buf.hunks = Arc::new(vec![proposal(1..2, "B\n")]);
        let fx = feed(&mut a, keys);
        assert_eq!((quit(&fx), writes(&fx)), (None, 0), "{keys:?}");
        assert_eq!(st(&mut a), PROPOSALS, "{keys:?}");
    }
    // With conflicts with the disk, a :q says it too, instead of "no write since last change".
    for keys in [":q\n", "q", "]cq"] {
        let mut a = three();
        let fx = feed(&mut a, keys);
        assert_eq!(quit(&fx), None, "{keys:?}");
        assert_eq!(st(&mut a), PROPOSALS, "{keys:?}");
    }
    // Once they are decided, the buffer is only modified.
    let mut a = three();
    feed(&mut a, "]cN");
    assert_eq!(quit(&feed(&mut a, ":q\n")), None);
    assert_eq!(st(&mut a), "no write since last change (:q! to override)");
}

#[test]
fn safe_7_a_quit_that_waits_for_its_save_does_not_quit_if_a_hunk_came_back_meanwhile() {
    let mut a = three();
    feed(&mut a, "]cN");
    let w = write_of(&feed(&mut a, ":wq\n")).unwrap();
    feed(&mut a, "u"); // the three proposals are back, and the text is the one that is being saved
    let fx = a.update(Event::Saved(Ok((hash(&w), None)), 0));
    assert_eq!(quit(&fx), None);
    assert_eq!(st(&mut a), format!("saved f; {PROPOSALS}"));
    assert_eq!(a.buf.hunks.len(), 3);
    // A forced one quits all the same.
    let mut a = three();
    feed(&mut a, "]cN");
    let w = write_of(&feed(&mut a, ":wq!\n")).unwrap();
    feed(&mut a, "u");
    assert_eq!(quit(&a.update(Event::Saved(Ok((hash(&w), None)), 0))), Some(0));
}

#[test]
fn safe_7_q_bang_zq_and_cq_discard_the_proposals() {
    for (keys, code) in [(":q!\n", 0), ("ZQ", 0), (":cq\n", 1), ("]cZQ", 0), (":x!\n", 0)] {
        let mut a = three();
        let fx = feed(&mut a, keys);
        // `:x!` writes (forced) and quits once that is saved.
        if keys == ":x!\n" {
            let w = write_of(&fx).unwrap();
            let fx = a.update(Event::Saved(Ok((hash(&w), None)), 0));
            assert_eq!(quit(&fx), Some(0), "{keys:?}");
        } else {
            assert_eq!(quit(&fx), Some(code), "{keys:?}");
        }
    }
    let mut a = app_with(Some("f"), b"a\nb\n", (80, 8));
    a.buf.hunks = Arc::new(vec![proposal(1..2, "B\n")]);
    assert_eq!(quit(&feed(&mut a, ":q!\n")), Some(0));
    let mut a = three();
    let fx = feed(&mut a, ":wq!\n");
    let w = write_of(&fx).unwrap();
    assert_eq!(quit(&fx), None);
    assert_eq!(quit(&a.update(Event::Saved(Ok((hash(&w), None)), 0))), Some(0));
}

// ---------------------------------------------------------------------------------------------
// VIS-3 in REVIEW: every hunk inline
// ---------------------------------------------------------------------------------------------

/// Mine edited lines 3 and 5 of an eight-line file, and so did the file, differently; it also
/// changed line 1, which is no conflict (a mark).
fn two_hunks() -> App {
    let base = "// setup\nlet a = 1;\nlet b = compute(a);\nlet c = 3;\nlet d = other(c);\nprint(b, d);\n// done\nend();\n";
    let mut a = app_with(Some("f.rs"), base.as_bytes(), (40, 12));
    feed(&mut a, ":3s/a\\)/a, 2)/\n:5s/c\\)/c, 4)/\n");
    let file = "// setup!\nlet a = 1;\nlet b = calc(a);\nlet c = 3;\nlet d = else(c);\nprint(b, d);\n// done\nend();\n";
    disk(&mut a, file);
    assert_eq!((a.buf.marks.len(), a.buf.hunks.len()), (1, 2));
    a // the cursor is on line 5, and has not seen the mark on line 1
}

/// The columns of row `r` in a style.
fn cols(f: &si::core::view::Frame, r: usize, style: Style) -> Vec<u16> {
    let mut v: Vec<u16> =
        f.rows[r].spans.iter().filter(|s| s.1 == style).flat_map(|s| s.0.clone()).collect();
    v.sort();
    v
}

#[test]
fn vis_3_review_shows_every_hunk_inline_golden() {
    let mut a = two_hunks();
    feed(&mut a, "[c");
    assert_eq!((a.vi.mode, line(&a)), (Mode::Review, 2));
    let f = screen(&mut a);
    golden("vis_3_review_inline", &dump(&f));
    // Line 3: `compute` struck through (the secondary color, struck), `calc` on the change
    // background, `(a` as it is, `, 2` struck: words of mine that would go and words of theirs that come.
    assert_eq!(f.rows[2].text, "~  3 let b = computecalc(a, 2);");
    assert_eq!(cols(&f, 2, Style::Deleted), [13, 14, 15, 16, 17, 18, 19, 26, 27, 28]);
    assert_eq!(cols(&f, 2, Style::Change), [0, 20, 21, 22, 23]);
    // Every hunk, not only the one under the cursor.
    assert_eq!(f.rows[4].text, "~  5 let d = otherelse(c, 4);");
    assert_eq!(cols(&f, 4, Style::Deleted).len(), 5 + 3);
    // The lines nothing changed in are secondary, all of the row; the cursor line's number is not.
    for r in [1, 3, 5, 6, 7] {
        let sec = cols(&f, r, Style::Secondary);
        for (c, ch) in f.rows[r].text.chars().enumerate() {
            assert!(ch == ' ' || sec.contains(&(c as u16)), "row {r} col {c}: {:?}", f.rows[r]);
        }
    }
    assert!(
        !cols(&f, 2, Style::Secondary).contains(&2),
        "the number of the cursor line is primary"
    );
    // The mark outside the hunks keeps the look it had (a gutter symbol and its changed words).
    assert_eq!(f.rows[0].text, "~  1 // setup!");
    assert_eq!(cols(&f, 0, Style::Change), [0, 13]);
    assert!(
        cols(&f, 0, Style::Secondary).contains(&2) && !cols(&f, 0, Style::Secondary).contains(&6)
    );
    assert_eq!(
        st_of(&f),
        format!("f.rs  3:1  REVIEW 1/2  {KEYS}").chars().take(40).collect::<String>()
    );
}

fn st_of(f: &si::core::view::Frame) -> String {
    status(f).to_string()
}

#[test]
fn vis_3_review_changes_only_the_look_of_hunks_and_unchanged_lines() {
    let mut a = two_hunks();
    feed(&mut a, "[c");
    let during = screen(&mut a);
    feed(&mut a, "<Esc>");
    let after = screen(&mut a);
    // Outside REVIEW a hunk is drawn as it was (M3a): the words of mine it would replace.
    assert_eq!(after.rows[2].text, "~  3 let b = compute(a, 2);");
    assert_eq!(
        cols(&after, 2, Style::Change),
        [0, 13, 14, 15, 16, 17, 18, 19, 22, 23, 24].to_vec()
    );
    assert!(cols(&after, 3, Style::Secondary).len() <= 3, "no dimming outside REVIEW");
    assert_ne!(during.rows[2], after.rows[2]);
}

#[test]
fn vis_3_review_rows_of_theirs_alone_are_extra_rows_without_a_number_golden() {
    let base = "alpha\nbeta\ngamma\ndelta\nepsilon\n";
    let mut a = app_with(Some("f"), base.as_bytes(), (40, 12));
    feed(&mut a, ":2s/$/ mine/\n:4s/$/ mine/\n");
    // The file adds a line after the changed `beta`, and deletes the changed `delta`.
    disk(&mut a, "alpha\nbeta theirs\nnew line\ngamma\nepsilon\n");
    assert_eq!(
        a.buf.hunks.iter().map(|h| h.theirs.to_string()).collect::<Vec<_>>(),
        ["beta theirs\nnew line\n", ""]
    );
    feed(&mut a, "[c");
    assert_eq!(line(&a), 1);
    let f = screen(&mut a);
    golden("vis_3_review_extra_rows", &dump(&f));
    // `beta mine` -> `beta theirs`, then the line only theirs has: a `+`, no number, all changed.
    assert_eq!(f.rows[1].text, "~  2 beta minetheirs");
    assert_eq!(f.rows[2].text, "+    new line");
    assert_eq!(cols(&f, 2, Style::Change), [0, 5, 6, 7, 8, 9, 10, 11, 12]);
    assert!(cols(&f, 2, Style::Secondary).is_empty(), "no number to dim");
    // The line that would be deleted: struck through, the number and the `~` of a line of mine.
    assert_eq!(f.rows[4].text, "~  4 delta mine");
    assert_eq!(cols(&f, 4, Style::Deleted), (5..15).collect::<Vec<u16>>());
    // The cursor is on the first row of the hunk, and the screen scrolls with extra rows too.
    assert_eq!(f.cursor, (5, 1));
}

#[test]
fn vis_3_review_an_empty_hunk_shows_its_rows_before_the_line_after_it() {
    let mut a = app_with(Some("f"), b"one\ntwo\nthree\n", (40, 10));
    a.buf.hunks = Arc::new(vec![proposal(1..1, "A\nB\n"), proposal(3..3, "Z\n")]);
    feed(&mut a, "]c");
    let f = screen(&mut a);
    let texts: Vec<&str> = f.rows[..6].iter().map(|r| r.text.as_str()).collect();
    assert_eq!(texts, ["   1 one", "+    A", "+    B", "   2 two", "   3 three", "+    Z"]);
    assert_eq!(f.cursor, (5, 1), "on the first row of the hunk");
    feed(&mut a, "]c");
    let f = screen(&mut a);
    assert_eq!(f.cursor.1, 4, "the second hunk is after the last line: the cursor is on that line");
    assert_eq!(line(&a), 2);
}

#[test]
fn vis_3_review_a_hunk_without_a_word_diff_is_all_struck_and_all_new() {
    let mut a = app_with(Some("f"), b"one\ntwo\nthree\n", (40, 10));
    let mut h = proposal(1..2, "TWO\nTOO\n");
    h.words = None;
    a.buf.hunks = Arc::new(vec![h]);
    feed(&mut a, "]c");
    let f = screen(&mut a);
    let texts: Vec<&str> = f.rows[..4].iter().map(|r| r.text.as_str()).collect();
    assert_eq!(texts, ["   1 one", "~  2 two", "+    TWO", "+    TOO"]);
    assert_eq!(cols(&f, 1, Style::Deleted), [5, 6, 7]);
    assert_eq!(cols(&f, 2, Style::Change), [0, 5, 6, 7]);
    assert_eq!(cols(&f, 3, Style::Change), [0, 5, 6, 7]);
}

// ---------------------------------------------------------------------------------------------
// Fuzz: random keys, rewrites by the disk and answers of the save thread
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
enum Step {
    Keys(&'static str),
    Disk(String),
    /// The save thread answers the write in flight, if any.
    Saved,
    Resize(u16, u16),
}

fn file_text() -> impl Strategy<Value = String> {
    prop::collection::vec(prop::sample::select(vec!["a", "b", "c", "dd", "", "a b", "日本"]), 1..7)
        .prop_map(|ls| ls.iter().map(|l| format!("{l}\n")).collect())
}

fn steps() -> impl Strategy<Value = Vec<Step>> {
    let keys = prop::sample::select(vec![
        "]c", "]c", "[c", "[c", "y", "y", "n", "n", "Y", "N", "u", "<C-r>", "<Esc>", "x", "dd",
        "j", "k", "gg", "G", "]cy", "]cn", "]cY", "]cN", "]c]cy", "]c]cn", "ix<Esc>", "ox<Esc>",
        "rQ", "p", "yy", ".", "J", ":w\n", ":w!\n", "<C-s>", ":wq\n", ":x\n", ":q\n", "q", "ZZ",
        ":seen\n", "v", "V", "d", "]", "[", "3", "zz", "/a\n",
    ])
    .prop_map(Step::Keys);
    let step = prop_oneof![
        12 => keys,
        5 => file_text().prop_map(Step::Disk),
        2 => Just(Step::Saved),
        1 => (6u16..60, 3u16..12).prop_map(|(w, h)| Step::Resize(w, h)),
    ];
    // Some edits of mine and a rewrite by the disk come first, so that there is something to review.
    let start = (
        prop::sample::subsequence(vec!["x", "jx", "jjrQ", "ox<Esc>", "Gx", "ggdd"], 1..4),
        file_text(),
    )
        .prop_map(|(ks, t)| {
            ks.into_iter().map(Step::Keys).chain([Step::Disk(t)]).collect::<Vec<_>>()
        });
    (start, prop::collection::vec(step, 1..30)).prop_map(|(a, b)| [a, b].concat())
}

fn session(base: &str, steps: &[Step], size: (u16, u16)) -> Result<(), String> {
    use unicode_width::UnicodeWidthStr;
    let mut a = app_with(Some("f"), base.as_bytes(), size);
    let first = text(&a);
    let mut write: Option<Vec<u8>> = None;
    // `]cy` is `]c` and then `y`: the second is checked on its own.
    let split = |s: &Step| match s {
        Step::Keys(k) if k.len() > 2 && k.starts_with("]c") && !k.contains('\n') => {
            vec![Step::Keys("]c"), Step::Keys(&k[2..])]
        }
        s => vec![s.clone()],
    };
    for step in &steps.iter().flat_map(split).collect::<Vec<_>>() {
        let (pre_text, pre_hunks, pre_mode) = (text(&a), a.buf.hunks.to_vec(), a.vi.mode);
        let (had_disk_hunk, waiting) = (pre_hunks.iter().any(|h| h.disk), a.vi.waiting());
        let fx = match step {
            Step::Keys(k) => feed(&mut a, k),
            Step::Disk(t) => disk(&mut a, t),
            Step::Saved => match write.take() {
                Some(b) => a.update(Event::Saved(Ok((hash(&b), None)), 0)),
                None => vec![],
            },
            Step::Resize(w, h) => a.update(Event::Resize(*w, *h)),
        };
        for e in &fx {
            match e {
                Effect::Write { bytes, .. } => write = Some(bytes.clone()),
                Effect::Quit(0) if !a.buf.hunks.is_empty() => {
                    return Err(format!("{step:?}: quit with {} hunks open", a.buf.hunks.len()));
                }
                _ => {}
            }
        }
        if let Step::Keys(k @ (":w\n" | "<C-s>" | ":wq\n" | ":x\n" | "ZZ")) = step
            && had_disk_hunk
            && fx.iter().any(|e| matches!(e, Effect::Write { .. }))
        {
            return Err(format!("{k:?} wrote over the disk while a conflict with it was open"));
        }
        // The screen: fits, and says what mode we are in.
        let f = screen(&mut a);
        let (w, h) = a.size;
        if f.rows.len() != usize::from(h) || f.rows.iter().any(|r| r.text.width() > usize::from(w))
        {
            return Err(format!("{step:?}: the frame does not fit {w}x{h}: {:?}", f.rows));
        }
        if f.cursor.0 >= w || f.cursor.1 >= h {
            return Err(format!("{step:?}: cursor {:?} outside {w}x{h}", f.cursor));
        }
        let n = a.buf.hunks.len();
        if a.vi.mode == Mode::Review {
            let s = status(&f).to_string();
            let tail = s.split("REVIEW ").nth(1).and_then(|t| t.split_whitespace().next());
            let ok = tail
                .and_then(|t| t.split_once('/'))
                .and_then(|(i, m)| Some((i.parse::<usize>().ok()?, m.parse::<usize>().ok()?)));
            // (A short screen cuts the status line, and a message replaces it until the next key.)
            if w >= 40 && a.msg.is_none() && !ok.is_some_and(|(i, m)| 1 <= i && i <= m && m == n) {
                return Err(format!("{step:?}: REVIEW status {s:?} with {n} hunks"));
            }
            if n == 0 {
                return Err(format!("{step:?}: REVIEW with no hunk"));
            }
        }
        // A decision is exactly what it says.
        if let Step::Keys(k @ ("y" | "n" | "Y" | "N")) = step
            && pre_mode == Mode::Review
            && !waiting
        // a `]` waits for its `c`: the y is swallowed
        {
            check_decision(k, &pre_text, &pre_hunks, &text(&a), &a.buf.hunks)?;
        }
    }
    // Undo everything: the text we began with, and nothing left open.
    feed(&mut a, "<Esc><Esc>");
    for _ in 0..200 {
        feed(&mut a, "u");
    }
    if text(&a) != first || !a.buf.hunks.is_empty() || !a.buf.marks.is_empty() {
        return Err(format!(
            "undo all: {:?} with {} hunks, wanted {first:?}",
            text(&a),
            a.buf.hunks.len()
        ));
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig { failure_persistence: None, ..ProptestConfig::with_cases(cases(800)) })]

    /// Whatever the keys, the rewrites and the answers: REVIEW is only open while a hunk is, a
    /// decision does what it says to the text and the hunks, nothing is saved over a conflict or
    /// quit away with a hunk open, the frame fits, and undo takes it all back.
    #[test]
    fn prop_8_fuzz_decisions_are_exact_and_review_stays_consistent(
        base in file_text(), steps in steps(), w in 6u16..70, h in 3u16..12,
    ) {
        if let Err(e) = session(&base, &steps, (w, h)) {
            prop_assert!(false, "{e}\nbase {base:?}\nsteps {steps:?} size {w}x{h}");
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Benchmarks (release builds): REVIEW on a big file
// ---------------------------------------------------------------------------------------------

/// 100,000 lines, every 30th one edited by me and rewritten differently by the disk, and every
/// other 30th one changed by the disk alone (a mark): thousands of hunks and marks.
fn big() -> App {
    let base: String = (0..100_000).map(|i| format!("line {i} of the file\n")).collect();
    let lines: Vec<&str> = base.lines().collect();
    let mut a = app_with(Some("big.rs"), base.as_bytes(), (80, 24));
    let edit = |i: usize, w: &str| lines[i].replace("line", w);
    let mine: Vec<String> =
        (0..100_000).map(|i| if i % 30 == 0 { edit(i, "mine") } else { lines[i].into() }).collect();
    a.buf.replace(0..usize::MAX, &mine.join("\n"));
    let theirs: String = (0..100_000)
        .map(|i| match i % 30 {
            0 => format!("{}\n", edit(i, "theirs")),
            15 => format!("{}\n", edit(i, "other")),
            _ => format!("{}\n", lines[i]),
        })
        .collect();
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(500);
    a.update(Event::Disk { bytes: Some(theirs.into_bytes()), hash: None, deadline, seq: 0 });
    a
}

#[test]
#[ignore]
fn prop_8_bench_review_keys_on_100k_lines_with_thousands_of_hunks() {
    use std::time::Instant;
    release_only();
    let mut a = big();
    eprintln!("bench review: {} hunks, {} marks", a.buf.hunks.len(), a.buf.marks.len());
    assert!(a.buf.hunks.len() > 3000);
    let worst = |name: &str, keys: &str, n: usize, a: &mut App| {
        let mut times: Vec<u128> = (0..n)
            .map(|_| {
                let t = Instant::now();
                feed(a, keys);
                t.elapsed().as_micros()
            })
            .collect();
        times.sort();
        eprintln!("bench review: {name}: median {} us, worst {} us", times[n / 2], times[n - 1]);
        times[n - 1]
    };
    feed(&mut a, "gg");
    let j = worst("]c", "]c", 200, &mut a);
    let k = worst("[c", "[c", 100, &mut a);
    let y = worst("y", "y", 200, &mut a);
    let n = worst("n", "n", 200, &mut a);
    assert!(
        j.max(k).max(y).max(n) < 3000,
        "a REVIEW key takes about a millisecond: {j} {k} {y} {n}"
    );
    let t = Instant::now();
    feed(&mut a, "Y");
    eprintln!("bench review: Y on {} hunks: {:?}", 3333 - 200 - 200, t.elapsed());
    assert!(t.elapsed().as_millis() < 500, "{:?}", t.elapsed());
}

#[test]
#[ignore]
fn prop_8_bench_review_one_hunk_as_big_as_the_word_diff_allows_and_one_as_big_as_the_file() {
    use std::time::Instant;
    release_only();
    // A block of 4,000 lines whose words all changed (the word diff is made), and the whole file.
    for (n, words) in [(4_000usize, true), (100_000, false)] {
        let base: String = (0..n).map(|i| format!("alpha {i} beta gamma\n")).collect();
        let mut a = app_with(Some("big.rs"), base.as_bytes(), (80, 24));
        let mine = (0..n).map(|i| format!("alpha {i} MINE gamma")).collect::<Vec<_>>().join("\n");
        a.buf.replace(0..usize::MAX, &mine);
        let theirs: String = (0..n).map(|i| format!("alpha {i} THEIRS gamma\n")).collect();
        let deadline = Instant::now() + std::time::Duration::from_millis(500);
        a.update(Event::Disk { bytes: Some(theirs.into_bytes()), hash: None, deadline, seq: 0 });
        assert_eq!(a.buf.hunks.len(), 1);
        assert_eq!(a.buf.hunks[0].words.is_some(), words, "{n} lines");
        feed(&mut a, "gg]c");
        let mut times: Vec<u128> = (0..100)
            .map(|_| {
                let t = Instant::now();
                feed(&mut a, "]c");
                t.elapsed().as_micros()
            })
            .collect();
        times.sort();
        eprintln!(
            "bench review: one hunk of {n} lines (word diff: {words}): median {} us, worst {} us",
            times[50], times[99]
        );
        assert!(times[99] < 3000);
    }
}
