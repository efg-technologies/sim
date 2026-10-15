//! Tests for src/core/proposal.rs: PROP-3 (hand-written merges), PROP-4 (properties of diff3),
//! PROP-7 (deadline and benchmark) and VIS-2 (word-level differences).

// Expected values are written as `vec![a..b]` on purpose: a list holding one range.
#![allow(clippy::single_range_in_vec_init)]

use proptest::collection::vec;
use proptest::prelude::*;
use si::core::proposal::{Hunk, Mark, Merged, lines, merge3, words as words_by};
use similar::{Algorithm, DiffOp, capture_diff_slices};
use std::time::{Duration, Instant};

/// `words` with no deadline.
fn words(old: &str, new: &str) -> (Vec<std::ops::Range<usize>>, Vec<std::ops::Range<usize>>) {
    words_by(old, new, None)
}

/// The text after resolving each hunk (`accept[i]` for `m.hunks[i]`): accept → theirs, reject → mine.
/// A hunk without an answer is rejected.
fn resolve(m: &Merged, accept: &[bool]) -> String {
    let (ls, mut out, mut at) = (lines(&m.text), String::new(), 0);
    for (i, h) in m.hunks.iter().enumerate() {
        ls[at..h.mine.start].iter().for_each(|l| out.push_str(l));
        if accept.get(i).copied().unwrap_or(false) {
            out.push_str(&h.theirs);
        } else {
            ls[h.mine.clone()].iter().for_each(|l| out.push_str(l));
        }
        at = h.mine.end;
    }
    ls[at..].iter().for_each(|l| out.push_str(l));
    out
}

fn accept_all(m: &Merged) -> String {
    resolve(m, &vec![true; m.hunks.len()])
}

fn reject_all(m: &Merged) -> String {
    resolve(m, &vec![false; m.hunks.len()])
}

fn disk(base: &str, mine: &str, theirs: &str) -> Merged {
    merge3(base, mine, theirs, false, None)
}

fn all(base: &str, mine: &str, theirs: &str) -> Merged {
    merge3(base, mine, theirs, true, None)
}

fn hunk(mine: std::ops::Range<usize>, theirs: &str) -> Hunk {
    Hunk { mine, theirs: theirs.to_string() }
}

fn mark(lines: std::ops::Range<usize>, old: &str) -> Mark {
    Mark { lines, old: old.to_string() }
}

// ---------------------------------------------------------------------------------------------
// lines
// ---------------------------------------------------------------------------------------------

#[test]
fn prop_3_lines_keep_their_newlines() {
    assert!(lines("").is_empty());
    assert_eq!(lines("\n"), ["\n"]);
    assert_eq!(lines("a"), ["a"]);
    assert_eq!(lines("a\n"), ["a\n"]);
    assert_eq!(lines("a\n\nb"), ["a\n", "\n", "b"]);
    assert_eq!(lines("a\nb\n"), ["a\n", "b\n"]);
    // Only LF ends a line (SAFE-3 keeps a lone CR as a character).
    assert_eq!(lines("a\r\nb\rc"), ["a\r\n", "b\rc"]);
}

// ---------------------------------------------------------------------------------------------
// PROP-3: hand-written merges
// ---------------------------------------------------------------------------------------------

#[test]
fn prop_3_no_change() {
    for t in ["", "a\n", "a\nb\n", "a\nb", "\n"] {
        for propose_all in [false, true] {
            let m = merge3(t, t, t, propose_all, None);
            assert_eq!(m, Merged { text: t.to_string(), marks: vec![], hunks: vec![] });
        }
    }
}

#[test]
fn prop_3_only_mine_changed() {
    let m = disk("a\nb\nc\n", "a\nB\nc\n", "a\nb\nc\n");
    assert_eq!(m, Merged { text: "a\nB\nc\n".into(), marks: vec![], hunks: vec![] });
    let m = all("a\nb\nc\n", "a\nB\nc\n", "a\nb\nc\n");
    assert_eq!(m, Merged { text: "a\nB\nc\n".into(), marks: vec![], hunks: vec![] });
}

#[test]
fn prop_3_only_theirs_changed_is_marked_on_disk() {
    let m = disk("a\nb\nc\n", "a\nb\nc\n", "a\nB\nc\n");
    assert_eq!(m.text, "a\nB\nc\n");
    assert_eq!(m.marks, [mark(1..2, "b\n")]);
    assert!(m.hunks.is_empty());
}

#[test]
fn prop_3_only_theirs_changed_is_proposed_for_filters() {
    let m = all("a\nb\nc\n", "a\nb\nc\n", "a\nB\nc\n");
    assert_eq!(m.text, "a\nb\nc\n");
    assert!(m.marks.is_empty());
    assert_eq!(m.hunks, [hunk(1..2, "B\n")]);
    assert_eq!(accept_all(&m), "a\nB\nc\n");
    assert_eq!(reject_all(&m), "a\nb\nc\n");
}

#[test]
fn prop_3_changes_that_do_not_overlap_are_both_taken() {
    let (b, m, t) = ("a\nb\nc\nd\ne\n", "A\nb\nc\nd\ne\n", "a\nb\nc\nd\nE\n");
    let d = disk(b, m, t);
    assert_eq!(d.text, "A\nb\nc\nd\nE\n");
    assert_eq!(d.marks, [mark(4..5, "e\n")]);
    assert!(d.hunks.is_empty());
    // For a filter, theirs is still only an opinion; my own edit stays.
    let p = all(b, m, t);
    assert_eq!(p.text, "A\nb\nc\nd\ne\n");
    assert!(p.marks.is_empty());
    assert_eq!(p.hunks, [hunk(4..5, "E\n")]);
    assert_eq!(accept_all(&p), "A\nb\nc\nd\nE\n");
}

#[test]
fn prop_3_overlapping_changes_conflict() {
    for propose_all in [false, true] {
        let m = merge3("a\nb\nc\n", "a\nX\nc\n", "a\nY\nc\n", propose_all, None);
        assert_eq!(m.text, "a\nX\nc\n");
        assert!(m.marks.is_empty());
        assert_eq!(m.hunks, [hunk(1..2, "Y\n")]);
        assert_eq!(accept_all(&m), "a\nY\nc\n");
        assert_eq!(reject_all(&m), "a\nX\nc\n");
    }
}

#[test]
fn prop_3_conflicts_chain_transitively() {
    // mine changes lines 1-2 and 4 (3 stays); theirs changes 2-4. Mine's two changes are joined by theirs.
    let base = "0\n1\n2\n3\n4\n5\n";
    let mine = "0\nM1\n3\nM4\n5\n";
    let theirs = "0\n1\nT\n5\n";
    let m = disk(base, mine, theirs);
    assert_eq!(m.text, mine);
    assert!(m.marks.is_empty());
    // The region runs from base line 1 to 4: theirs' version of it is "1", "T".
    assert_eq!(m.hunks, [hunk(1..4, "1\nT\n")]);
    assert_eq!(accept_all(&m), theirs);
}

#[test]
fn prop_3_two_separate_conflicts_stay_separate() {
    let base = "a\nb\nc\nd\ne\n";
    let m = disk(base, "a\nX\nc\nY\ne\n", "a\nP\nc\nQ\ne\n");
    assert_eq!(m.hunks, [hunk(1..2, "P\n"), hunk(3..4, "Q\n")]);
    assert_eq!(resolve(&m, &[true, false]), "a\nP\nc\nY\ne\n");
    assert_eq!(resolve(&m, &[false, true]), "a\nX\nc\nQ\ne\n");
}

#[test]
fn prop_3_identical_change_on_both_sides_is_taken_once() {
    for propose_all in [false, true] {
        let m = merge3("a\nb\nc\n", "a\nX\nc\n", "a\nX\nc\n", propose_all, None);
        assert_eq!(m, Merged { text: "a\nX\nc\n".into(), marks: vec![], hunks: vec![] });
    }
    // Identical here, in conflict there.
    let m = disk("a\nb\nc\nd\ne\n", "A\nb\nc\nX\ne\n", "A\nb\nc\nY\ne\n");
    assert_eq!(m.text, "A\nb\nc\nX\ne\n");
    assert!(m.marks.is_empty());
    assert_eq!(m.hunks, [hunk(3..4, "Y\n")]);
}

#[test]
fn prop_3_insertions_at_the_same_point() {
    for propose_all in [false, true] {
        let m = merge3("a\nc\n", "a\nb\nc\n", "a\nB\nc\n", propose_all, None);
        assert_eq!(m.text, "a\nb\nc\n");
        assert!(m.marks.is_empty());
        assert_eq!(m.hunks, [hunk(1..2, "B\n")]);
        assert_eq!(accept_all(&m), "a\nB\nc\n");
        // The same insertion on both sides is one insertion.
        let m = merge3("a\nc\n", "a\nb\nc\n", "a\nb\nc\n", propose_all, None);
        assert_eq!(m, Merged { text: "a\nb\nc\n".into(), marks: vec![], hunks: vec![] });
    }
}

#[test]
fn prop_3_insertions_at_different_points_do_not_conflict() {
    let m = disk("a\nb\n", "a\nx\nb\n", "a\nb\ny\n");
    assert_eq!(m.text, "a\nx\nb\ny\n");
    assert_eq!(m.marks, [mark(3..4, "")]);
    assert!(m.hunks.is_empty());
}

#[test]
fn prop_3_changes_that_merely_touch_do_not_conflict() {
    // Theirs inserts right before the line mine replaced.
    let m = disk("a\nb\nc\n", "a\nB\nc\n", "a\nx\nb\nc\n");
    assert_eq!(m.text, "a\nx\nB\nc\n");
    assert_eq!(m.marks, [mark(1..2, "")]);
    assert!(m.hunks.is_empty());
    // Theirs inserts right after it.
    let m = disk("a\nb\nc\n", "a\nB\nc\n", "a\nb\nx\nc\n");
    assert_eq!(m.text, "a\nB\nx\nc\n");
    assert_eq!(m.marks, [mark(2..3, "")]);
    assert!(m.hunks.is_empty());
    // Mine replaces line 1 and theirs replaces line 2: adjacent, no shared line.
    let m = disk("a\nb\nc\nd\n", "a\nB\nc\nd\n", "a\nb\nC\nd\n");
    assert_eq!(m.text, "a\nB\nC\nd\n");
    assert_eq!(m.marks, [mark(2..3, "c\n")]);
    assert!(m.hunks.is_empty());
}

#[test]
fn prop_3_insertion_inside_a_replaced_region_conflicts() {
    let m = disk("a\nb\nc\nd\n", "a\nX\nd\n", "a\nb\nY\nc\nd\n");
    assert_eq!(m.text, "a\nX\nd\n");
    assert!(m.marks.is_empty());
    assert_eq!(m.hunks, [hunk(1..2, "b\nY\nc\n")]);
    assert_eq!(accept_all(&m), "a\nb\nY\nc\nd\n");
}

#[test]
fn prop_3_deletions() {
    let b = "a\nb\nc\n";
    // Mine deletes: nothing to take from theirs.
    let m = disk(b, "a\nc\n", b);
    assert_eq!(m, Merged { text: "a\nc\n".into(), marks: vec![], hunks: vec![] });
    // Theirs deletes: marked where it was (an empty range), or proposed.
    let m = disk(b, b, "a\nc\n");
    assert_eq!(m.text, "a\nc\n");
    assert_eq!(m.marks, [mark(1..1, "b\n")]);
    let m = all(b, b, "a\nc\n");
    assert_eq!(m.text, b);
    assert_eq!(m.hunks, [hunk(1..2, "")]);
    assert_eq!(accept_all(&m), "a\nc\n");
    // Both delete the same line.
    let m = disk(b, "a\nc\n", "a\nc\n");
    assert_eq!(m, Merged { text: "a\nc\n".into(), marks: vec![], hunks: vec![] });
    // Mine deletes, theirs edits the same line: the hunk sits at an empty range.
    let m = disk(b, "a\nc\n", "a\nB\nc\n");
    assert_eq!(m.text, "a\nc\n");
    assert_eq!(m.hunks, [hunk(1..1, "B\n")]);
    assert_eq!(accept_all(&m), "a\nB\nc\n");
    // Mine edits, theirs deletes the same line.
    let m = disk(b, "a\nB\nc\n", "a\nc\n");
    assert_eq!(m.text, "a\nB\nc\n");
    assert_eq!(m.hunks, [hunk(1..2, "")]);
    assert_eq!(accept_all(&m), "a\nc\n");
    assert_eq!(reject_all(&m), "a\nB\nc\n");
}

#[test]
fn prop_3_text_without_a_final_newline() {
    // Theirs adds the final newline: the last line is a different line.
    let m = disk("a\nb", "a\nb", "a\nb\n");
    assert_eq!(m.text, "a\nb\n");
    assert_eq!(m.marks, [mark(1..2, "b")]);
    // Mine's last line lacks the newline too, in another place.
    let m = disk("a\nb\nc", "a\nb\nC", "A\nb\nc");
    assert_eq!(m.text, "A\nb\nC");
    assert_eq!(m.marks, [mark(0..1, "a\n")]);
    // Both drop the final newline.
    let m = disk("a\nb\n", "a\nb", "a\nb");
    assert_eq!(m, Merged { text: "a\nb".into(), marks: vec![], hunks: vec![] });
    // Both add the same final line without a newline.
    let m = disk("a\n", "a\nb", "a\nb");
    assert_eq!(m, Merged { text: "a\nb".into(), marks: vec![], hunks: vec![] });
    // Appending to a text without a final newline.
    let m = disk("a\nb", "a\nb", "a\nb\nc\n");
    assert_eq!(m.text, "a\nb\nc\n");
    let m = all("a\nb", "a\nb", "a\nb\nc\n");
    assert_eq!(m.text, "a\nb");
    assert_eq!(accept_all(&m), "a\nb\nc\n");
}

#[test]
fn prop_3_a_last_line_without_newline_never_joins_a_line_added_after_it() {
    // Mine ends with "x" (no newline); theirs adds a line after the old last line. Applying both would
    // glue them into "xd", so these overlap.
    let m = disk("a\nb\n", "a\nx", "a\nb\nd\n");
    assert_eq!(m.text, "a\nx");
    assert_eq!(m.hunks, [hunk(1..2, "b\nd\n")]);
    assert_eq!(accept_all(&m), "a\nb\nd\n");
    // And the other way round.
    let m = disk("a\nb\n", "a\nb\nd\n", "a\nx");
    assert_eq!(m.text, "a\nb\nd\n");
    assert_eq!(m.hunks, [hunk(1..3, "x")]);
    assert_eq!(accept_all(&m), "a\nx");
}

#[test]
fn prop_3_empty_texts() {
    let m = disk("", "", "");
    assert_eq!(m, Merged { text: String::new(), marks: vec![], hunks: vec![] });
    // Only mine writes into an empty file.
    assert_eq!(disk("", "a\n", "").text, "a\n");
    // Only theirs does.
    let m = disk("", "", "b\n");
    assert_eq!(m.text, "b\n");
    assert_eq!(m.marks, [mark(0..1, "")]);
    let m = all("", "", "b\n");
    assert_eq!(m.text, "");
    assert_eq!(m.hunks, [hunk(0..0, "b\n")]);
    assert_eq!(accept_all(&m), "b\n");
    // Both write something different.
    let m = disk("", "a\n", "b\n");
    assert_eq!(m.text, "a\n");
    assert_eq!(m.hunks, [hunk(0..1, "b\n")]);
    // Mine empties the file, theirs edits it.
    let m = disk("a\n", "", "b\n");
    assert_eq!(m.text, "");
    assert_eq!(m.hunks, [hunk(0..0, "b\n")]);
    // Both empty it.
    assert_eq!(disk("a\n", "", "").text, "");
    // Theirs empties it.
    let m = disk("a\nb\n", "a\nb\n", "");
    assert_eq!(m.text, "");
    assert_eq!(m.marks, [mark(0..0, "a\nb\n")]);
}

#[test]
fn prop_3_resolve_treats_a_missing_answer_as_reject() {
    let m = disk("a\nb\nc\nd\ne\n", "a\nX\nc\nY\ne\n", "a\nP\nc\nQ\ne\n");
    assert_eq!(resolve(&m, &[]), m.text);
    assert_eq!(resolve(&m, &[true]), "a\nP\nc\nY\ne\n");
}

#[test]
fn prop_3_one_conflict_among_many_marks() {
    let base: String = (0..10).map(|i| format!("l{i}\n")).collect();
    let mut mine: Vec<String> = lines(&base).iter().map(|s| s.to_string()).collect();
    let mut theirs = mine.clone();
    mine[1] = "mine1\n".into();
    theirs[1] = "theirs1\n".into(); // conflict
    theirs[4] = "theirs4\n".into(); // mark
    mine[7] = "mine7\n".into(); // mine only
    theirs[9] = "theirs9\n".into(); // mark
    let m = disk(&base, &mine.concat(), &theirs.concat());
    assert_eq!(m.hunks, [hunk(1..2, "theirs1\n")]);
    assert_eq!(m.marks, [mark(4..5, "l4\n"), mark(9..10, "l9\n")]);
    let expected: String = mine
        .iter()
        .enumerate()
        .map(|(i, l)| match i {
            4 => "theirs4\n",
            9 => "theirs9\n",
            _ => l.as_str(),
        })
        .collect();
    assert_eq!(m.text, expected);
}

// ---------------------------------------------------------------------------------------------
// PROP-4: properties
// ---------------------------------------------------------------------------------------------

const LINES: [&str; 5] = ["a\n", "b\n", "c\n", "\n", "d d\n"];

fn build(idx: &[usize], final_newline: bool) -> String {
    let mut s: String = idx.iter().map(|&i| LINES[i]).collect();
    if !final_newline && s.ends_with('\n') {
        s.pop();
    }
    s
}

/// Applies up to a few edits (insert, delete, replace one line) to a list of line indexes.
fn edit(base: &[usize], edits: &[(usize, u8, usize)]) -> Vec<usize> {
    let mut v = base.to_vec();
    for &(pos, kind, sym) in edits {
        match kind % 3 {
            0 => v.insert(pos % (v.len() + 1), sym),
            1 if !v.is_empty() => {
                v.remove(pos % v.len());
            }
            2 if !v.is_empty() => {
                let at = pos % v.len();
                v[at] = sym;
            }
            _ => {}
        }
    }
    v
}

type Edits = Vec<(usize, u8, usize)>;

fn edits() -> impl Strategy<Value = Edits> {
    vec((0..16usize, 0..3u8, 0..LINES.len()), 0..4)
}

/// (base, mine, theirs): mostly two edited copies of a base, so that changes overlap and touch often;
/// sometimes three unrelated texts. Each text may or may not end with a newline.
fn trio() -> impl Strategy<Value = (String, String, String)> {
    let line = 0..LINES.len();
    let edited = (vec(line.clone(), 0..9), edits(), edits(), any::<[bool; 3]>()).prop_map(
        |(b, em, et, nl)| {
            (build(&b, nl[0]), build(&edit(&b, &em), nl[1]), build(&edit(&b, &et), nl[2]))
        },
    );
    let unrelated =
        (vec(line.clone(), 0..7), vec(line.clone(), 0..7), vec(line, 0..7), any::<[bool; 3]>())
            .prop_map(|(b, m, t, nl)| (build(&b, nl[0]), build(&m, nl[1]), build(&t, nl[2])));
    prop_oneof![4 => edited, 1 => unrelated]
}

fn line_count(ls: &[&str], x: &str) -> usize {
    ls.iter().filter(|l| **l == x).count()
}

/// The lines of `side` that the diff from `base` marks as inserted or replaced (Myers, like merge3).
fn changed<'a>(base: &[&'a str], side: &[&'a str]) -> Vec<&'a str> {
    capture_diff_slices(Algorithm::Myers, base, side)
        .iter()
        .filter(|op| !matches!(op, DiffOp::Equal { .. }))
        .flat_map(|op| side[op.new_range()].iter().copied())
        .collect()
}

fn check_well_formed(m: &Merged) -> Result<(), TestCaseError> {
    let n = lines(&m.text).len();
    for h in &m.hunks {
        prop_assert!(
            h.mine.start <= h.mine.end && h.mine.end <= n,
            "hunk out of bounds: {h:?} in {n} lines"
        );
    }
    for k in &m.marks {
        prop_assert!(
            k.lines.start <= k.lines.end && k.lines.end <= n,
            "mark out of bounds: {k:?} in {n} lines"
        );
        prop_assert!(
            !(k.lines.is_empty() && k.old.is_empty()),
            "a mark that changed nothing: {k:?}"
        );
    }
    for w in m.hunks.windows(2) {
        prop_assert!(
            w[0].mine.end <= w[1].mine.start,
            "hunks overlap or are unsorted: {:?}",
            m.hunks
        );
    }
    for w in m.marks.windows(2) {
        prop_assert!(
            w[0].lines.end <= w[1].lines.start,
            "marks overlap or are unsorted: {:?}",
            m.marks
        );
    }
    // A hunk is only worth showing when accepting it changes something.
    for h in &m.hunks {
        let mine: String = lines(&m.text)[h.mine.clone()].concat();
        prop_assert!(mine != h.theirs, "a hunk that proposes nothing: {h:?}");
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10_000))]

    // 1. Every theirs change is a proposal: rejecting every hunk gives mine.
    #[test]
    fn prop_4_propose_all_reject_all_gives_mine((b, m, t) in trio()) {
        let r = all(&b, &m, &t);
        prop_assert_eq!(&r.text, &m);
        prop_assert!(r.marks.is_empty());
        prop_assert_eq!(reject_all(&r), m);
    }

    // 2. If mine is the base, accepting every hunk gives theirs.
    #[test]
    fn prop_4_accept_all_gives_theirs_when_mine_is_the_base((b, _m, t) in trio()) {
        for propose_all in [false, true] {
            let r = merge3(&b, &b, &t, propose_all, None);
            prop_assert_eq!(accept_all(&r), t.clone());
            prop_assert_eq!(reject_all(&r), r.text.clone());
        }
    }

    // 3. If theirs is the base, nothing comes in: the text is mine and there is nothing to decide.
    #[test]
    fn prop_4_nothing_comes_in_when_theirs_is_the_base((b, m, _t) in trio()) {
        for propose_all in [false, true] {
            let r = merge3(&b, &m, &b, propose_all, None);
            prop_assert_eq!(r, Merged { text: m.clone(), marks: vec![], hunks: vec![] });
        }
    }

    // 4. On disk, if both sides made the same text, there is nothing to take or decide.
    #[test]
    fn prop_4_identical_sides_leave_mine_untouched((b, m, _t) in trio()) {
        for propose_all in [false, true] {
            let r = merge3(&b, &m, &m, propose_all, None);
            prop_assert_eq!(r, Merged { text: m.clone(), marks: vec![], hunks: vec![] });
        }
    }

    // 5. The merge is symmetric: accepting everything equals rejecting everything with the roles swapped.
    #[test]
    fn prop_4_accept_all_equals_reject_all_with_roles_swapped((b, m, t) in trio()) {
        let a = disk(&b, &m, &t);
        let s = disk(&b, &t, &m);
        prop_assert_eq!(accept_all(&a), reject_all(&s));
        prop_assert_eq!(reject_all(&a), accept_all(&s));
        prop_assert_eq!(a.hunks.len(), s.hunks.len());
    }

    // 6. Accepting every hunk gives the same text whether theirs' changes were applied or proposed.
    #[test]
    fn prop_4_accept_all_is_the_same_in_both_modes((b, m, t) in trio()) {
        prop_assert_eq!(accept_all(&disk(&b, &m, &t)), accept_all(&all(&b, &m, &t)));
    }

    // 7. Nothing is lost. A line "changed" by a side is one in an inserted or replaced part of the
    //    Myers diff from the base to that side. For every distinct line x:
    //    (a) the merged text has x at least as often as mine changed to x;
    //    (b) the merged text and the `theirs` of all hunks together have x at least as often as theirs
    //        changed to x.
    #[test]
    fn prop_4_no_line_a_side_changed_is_lost((b, m, t) in trio()) {
        let (bl, ml, tl) = (lines(&b), lines(&m), lines(&t));
        let (cm, ct) = (changed(&bl, &ml), changed(&bl, &tl));
        for propose_all in [false, true] {
            let r = merge3(&b, &m, &t, propose_all, None);
            let merged = lines(&r.text);
            let proposed: Vec<&str> = r.hunks.iter().flat_map(|h| lines(&h.theirs)).collect();
            for x in cm.iter().chain(&ct) {
                prop_assert!(line_count(&cm, x) <= line_count(&merged, x),
                    "mine's {x:?} lost (propose_all {propose_all}): {r:?}");
                prop_assert!(line_count(&ct, x) <= line_count(&merged, x) + line_count(&proposed, x),
                    "theirs' {x:?} lost (propose_all {propose_all}): {r:?}");
            }
        }
    }

    // 8. Hunks and marks are sorted, disjoint and inside the merged text.
    #[test]
    fn prop_4_hunks_and_marks_are_sorted_disjoint_and_in_bounds((b, m, t) in trio()) {
        for propose_all in [false, true] {
            check_well_formed(&merge3(&b, &m, &t, propose_all, None))?;
        }
    }

    // 9. Putting back what every mark replaced gives mine: marks are exactly the changes taken in.
    #[test]
    fn prop_4_undoing_the_marks_gives_mine((b, m, t) in trio()) {
        let r = disk(&b, &m, &t);
        let ls = lines(&r.text);
        let (mut out, mut at) = (String::new(), 0);
        for k in &r.marks {
            out += &ls[at..k.lines.start].concat();
            out += &k.old;
            at = k.lines.end;
        }
        out += &ls[at..].concat();
        prop_assert_eq!(out, m);
    }

    // The deadline only changes how good the diff is, never the invariants.
    #[test]
    fn prop_4_an_expired_deadline_keeps_the_invariants((b, m, t) in trio()) {
        let past = Some(Instant::now());
        let r = merge3(&b, &m, &t, true, past);
        prop_assert_eq!(reject_all(&r), m.clone());
        check_well_formed(&r)?;
        let d = merge3(&b, &m, &t, false, past);
        prop_assert_eq!(accept_all(&d), accept_all(&r));
        check_well_formed(&d)?;
    }
}

// ---------------------------------------------------------------------------------------------
// PROP-7: speed
// ---------------------------------------------------------------------------------------------

#[test]
fn prop_7_a_deadline_in_the_past_still_gives_a_merge() {
    let base: String = (0..2000).map(|i| format!("line {i}\n")).collect();
    let mine: String = (0..2000)
        .map(|i| if i % 50 == 3 { format!("mine {i}\n") } else { format!("line {i}\n") })
        .collect();
    let theirs: String = (0..2000)
        .map(|i| if i % 70 == 5 { format!("theirs {i}\n") } else { format!("line {i}\n") })
        .collect();
    let m = merge3(&base, &mine, &theirs, false, Some(Instant::now()));
    assert!(!m.text.is_empty());
    assert!(!m.hunks.is_empty() || !m.marks.is_empty());
    let m = merge3(&base, &mine, &theirs, true, Some(Instant::now()));
    assert_eq!(reject_all(&m), mine);
}

/// 100,000 lines with scattered edits on both sides (replaced, deleted and inserted lines, a few
/// of which collide), merged with a 50 ms deadline. Run: cargo test --release -- --ignored bench --nocapture
#[test]
#[ignore]
fn prop_7_bench_merge_of_100k_lines_finishes_within_the_deadline() {
    const N: usize = 100_000;
    let line = |i: usize| format!("    let value_{i} = compute({i}, \"some text on the line\");\n");
    let side = |step: usize, offset: usize, tag: &str| -> String {
        let mut s = String::new();
        for i in 0..N {
            if i % step == offset {
                match (i / step) % 3 {
                    0 => s += &format!("    let {tag}_{i} = changed({i});\n"),
                    1 => {}
                    _ => {
                        s += &line(i);
                        s += &format!("    // added by {tag} at {i}\n");
                    }
                }
            } else {
                s += &line(i);
            }
        }
        s
    };
    let base: String = (0..N).map(line).collect();
    let mine = side(1000, 0, "mine");
    let theirs = side(997, 0, "theirs");
    let mut times = Vec::new();
    for propose_all in [false, true] {
        for _ in 0..5 {
            let start = Instant::now();
            let m =
                merge3(&base, &mine, &theirs, propose_all, Some(start + Duration::from_millis(50)));
            let took = start.elapsed();
            assert!(!m.text.is_empty() && (m.hunks.len() + m.marks.len()) > 50);
            times.push((propose_all, took));
        }
    }
    for (propose_all, took) in &times {
        eprintln!("bench propose_all={propose_all}: {took:?}");
    }
    let worst = times.iter().map(|t| t.1).max().unwrap_or_default();
    assert!(worst < Duration::from_millis(50), "merge of {N} lines took {worst:?}");
}

/// The worst case for a diff: every line moved, or the same few lines everywhere. The deadline is
/// shared by both diffs, so the whole merge gives up near 50 ms (a little slack for the final walk).
#[test]
#[ignore]
fn prop_7_bench_hard_inputs_give_up_at_the_deadline() {
    const N: usize = 100_000;
    let text = |f: &dyn Fn(usize) -> usize| -> String {
        (0..N).map(|i| format!("alpha {}\n", f(i))).collect()
    };
    let (ordered, reversed) = (text(&|i| i), text(&|i| N - 1 - i));
    let (few, other_few) = (text(&|i| i % 50), text(&|i| (i * 7919) % 100));
    for (base, mine, theirs) in [(&ordered, &reversed, &few), (&few, &other_few, &ordered)] {
        for propose_all in [false, true] {
            let start = Instant::now();
            let m =
                merge3(base, mine, theirs, propose_all, Some(start + Duration::from_millis(50)));
            let took = start.elapsed();
            eprintln!("bench hard propose_all={propose_all}: {took:?}");
            assert_eq!(reject_all(&m), m.text);
            assert!(took < Duration::from_millis(75), "gave up too late: {took:?}");
        }
    }
}

// ---------------------------------------------------------------------------------------------
// VIS-2: words
// ---------------------------------------------------------------------------------------------

/// The deadline of PROP-7 reaches the word diff too: past it, the answer is coarse but still
/// sound (what is not marked is the same text on both sides).
#[test]
fn vis_2_words_after_the_deadline_are_coarse_but_sound() {
    let old: String = (0..3000).map(|i| format!("word{i} ")).collect();
    let new: String = (0..3000)
        .map(|i| if i % 7 == 3 { format!("new{i} ") } else { format!("word{i} ") })
        .collect();
    let (o, n) = words_by(&old, &new, Some(Instant::now()));
    assert_eq!(outside(&old, &o), outside(&new, &n));
    let (fine_o, _) = words(&old, &new);
    let marked = |r: &[std::ops::Range<usize>]| r.iter().map(|r| r.len()).sum::<usize>();
    assert!(marked(&o) >= marked(&fine_o), "giving up never marks less");
}

#[test]
fn vis_2_words_ascii() {
    assert_eq!(words("foo bar", "foo baz"), (vec![4..7], vec![4..7]));
    assert_eq!(words("foo bar", "foo bar"), (vec![], vec![]));
    assert_eq!(words("let x = 1;", "let y = 1;"), (vec![4..5], vec![4..5]));
    assert_eq!(words("one two three", "one 2 three"), (vec![4..7], vec![4..5]));
    // A word and a space added or removed: only one side marks anything, and the rest is the same text.
    let (o, n) = words("a c", "a b c");
    assert!(o.is_empty() && n.iter().map(|r| r.len()).sum::<usize>() == 2);
    let (o, n) = words("a b c", "a c");
    assert!(n.is_empty() && o.iter().map(|r| r.len()).sum::<usize>() == 2);
}

#[test]
fn vis_2_words_underscore_belongs_to_the_word() {
    assert_eq!(words("snake_case", "snake_cases"), (vec![0..10], vec![0..11]));
    assert_eq!(words("a_b c", "a_c c"), (vec![0..3], vec![0..3]));
    assert_eq!(words("x_1 y", "x_1 z"), (vec![4..5], vec![4..5]));
    // Digits are part of a word.
    assert_eq!(words("v10 end", "v11 end"), (vec![0..3], vec![0..3]));
}

#[test]
fn vis_2_words_every_symbol_is_its_own_token() {
    assert_eq!(words("f(x)", "f(y)"), (vec![2..3], vec![2..3]));
    assert_eq!(words("a.b", "a,b"), (vec![1..2], vec![1..2]));
    // "->" is two tokens, so changing one of them marks one character.
    assert_eq!(words("a->b", "a=>b"), (vec![1..2], vec![1..2]));
    // One "+" of "++" removed: exactly one character is marked, whichever of the two the diff picks.
    let (o, n) = words("a++b", "a+b");
    assert_eq!(o.iter().map(|r| r.len()).sum::<usize>(), 1);
    assert!(o[0].start == 1 || o[0].start == 2);
    assert!(n.is_empty());
}

#[test]
fn vis_2_words_whitespace_runs_are_one_token() {
    assert_eq!(words("a b", "a  b"), (vec![1..2], vec![1..3]));
    assert_eq!(words("a  b", "a b"), (vec![1..3], vec![1..2]));
    assert_eq!(words("a\tb", "a b"), (vec![1..2], vec![1..2]));
    assert_eq!(words("ab", "a b"), (vec![0..2], vec![0..3]));
    // A newline is whitespace as well.
    assert_eq!(words("a\nb\n", "a\nc\n"), (vec![2..3], vec![2..3]));
    assert_eq!(words("a b", "a\nb"), (vec![1..2], vec![1..2]));
}

#[test]
fn vis_2_words_each_cjk_character_is_a_word() {
    assert_eq!(words("日本語です", "日本人です"), (vec![2..3], vec![2..3]));
    assert_eq!(words("日本語", "日本語"), (vec![], vec![]));
    // Insertion and deletion of one character.
    assert_eq!(words("日本", "日本語"), (vec![], vec![2..3]));
    assert_eq!(words("日本語", "日語"), (vec![1..2], vec![]));
    // Full-width letters and punctuation have width 2 and stand alone.
    assert_eq!(words("ＡＢ", "ＡＣ"), (vec![1..2], vec![1..2]));
    assert_eq!(words("あ、い", "あ。い"), (vec![1..2], vec![1..2]));
    // A full-width space is one token on its own, too.
    assert_eq!(words("あ　い", "あい"), (vec![1..2], vec![]));
}

#[test]
fn vis_2_words_mixed_scripts_and_char_indexes() {
    // The ranges count chars, not bytes: "日本語" takes 9 bytes.
    assert_eq!(words("日本語x", "日本語y"), (vec![3..4], vec![3..4]));
    assert_eq!(words("é x", "é y"), (vec![2..3], vec![2..3]));
    assert_eq!(words("abc日本def", "abc日本語def"), (vec![], vec![5..6]));
    assert_eq!(words("abc日本def", "abc日本ghi"), (vec![5..8], vec![5..8]));
    assert_eq!(words("名前: tsuyoshi", "名前: tsuyoshi2"), (vec![4..12], vec![4..13]));
    // Accented letters and other alphabets are letters, so they stay in the word.
    assert_eq!(words("café au lait", "cafe au lait"), (vec![0..4], vec![0..4]));
    assert_eq!(words("привет мир", "привет всем"), (vec![7..10], vec![7..11]));
    // An emoji has width 2.
    assert_eq!(words("ok 😀", "ok 😎"), (vec![3..4], vec![3..4]));
}

#[test]
fn vis_2_words_empty_inputs() {
    assert_eq!(words("", ""), (vec![], vec![]));
    assert_eq!(words("", "abc"), (vec![], vec![0..3]));
    assert_eq!(words("abc", ""), (vec![0..3], vec![]));
    assert_eq!(words("", " "), (vec![], vec![0..1]));
    assert_eq!(words("日本", ""), (vec![0..2], vec![]));
}

/// The text outside the ranges, as chars.
fn outside(s: &str, ranges: &[std::ops::Range<usize>]) -> Vec<char> {
    let cs: Vec<char> = s.chars().collect();
    (0..cs.len()).filter(|i| !ranges.iter().any(|r| r.contains(i))).map(|i| cs[i]).collect()
}

fn word_text() -> impl Strategy<Value = String> {
    vec(
        prop::sample::select(vec![
            'a', 'b', '_', '1', ' ', '\n', '.', '(', '日', '本', 'Ａ', 'é', '😀',
        ]),
        0..14,
    )
    .prop_map(|v| v.into_iter().collect())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2_000))]

    // What is not marked is the same text on both sides, and ranges are sorted, disjoint, non-empty, in bounds.
    #[test]
    fn vis_2_words_unmarked_text_is_identical(old in word_text(), new in word_text()) {
        let (o, n) = words(&old, &new);
        prop_assert_eq!(outside(&old, &o), outside(&new, &n));
        for (s, ranges) in [(&old, &o), (&new, &n)] {
            let len = s.chars().count();
            for r in ranges.iter() {
                prop_assert!(r.start < r.end && r.end <= len, "bad range {r:?} for {s:?}");
            }
            for w in ranges.windows(2) {
                prop_assert!(w[0].end <= w[1].start);
            }
        }
        if old == new {
            prop_assert!(o.is_empty() && n.is_empty());
        } else {
            prop_assert!(!o.is_empty() || !n.is_empty());
        }
    }
}
