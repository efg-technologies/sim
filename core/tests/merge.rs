//! Tests for src/core/merge.rs: DISK-3 (hand-written merges), DISK-4 (properties of diff3),
//! DISK-5 (deadline and benchmark) and VIS-2 (word-level differences).

// Expected values are written as `vec![a..b]` on purpose: a list holding one range.
#![allow(clippy::single_range_in_vec_init)]

use proptest::collection::vec;
use proptest::prelude::*;
use sim_core::core::merge::{Mark, Merged, lines, merge3, words as words_by};
use similar::{Algorithm, DiffOp, capture_diff_slices};
use std::time::{Duration, Instant};

/// `words` with no deadline.
fn words(old: &str, new: &str) -> (Vec<std::ops::Range<usize>>, Vec<std::ops::Range<usize>>) {
    words_by(old, new, None)
}

fn disk(base: &str, mine: &str, theirs: &str) -> Merged {
    merge3(base, mine, theirs, None)
}

fn mark(lines: std::ops::Range<usize>, old: &str) -> Mark {
    Mark { lines, old: old.to_string() }
}

// ---------------------------------------------------------------------------------------------
// lines
// ---------------------------------------------------------------------------------------------

#[test]
fn disk_3_lines_keep_their_newlines() {
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
// DISK-3: hand-written merges
// ---------------------------------------------------------------------------------------------

#[test]
fn disk_3_no_change() {
    for t in ["", "a\n", "a\nb\n", "a\nb", "\n"] {
        assert_eq!(disk(t, t, t), Merged { text: t.to_string(), marks: vec![], conflicts: vec![] });
    }
}

#[test]
fn disk_3_only_mine_changed() {
    let m = disk("a\nb\nc\n", "a\nB\nc\n", "a\nb\nc\n");
    assert_eq!(m, Merged { text: "a\nB\nc\n".into(), marks: vec![], conflicts: vec![] });
}

#[test]
fn disk_3_only_theirs_changed_is_marked_on_disk() {
    let m = disk("a\nb\nc\n", "a\nb\nc\n", "a\nB\nc\n");
    assert_eq!(m.text, "a\nB\nc\n");
    assert_eq!(m.marks, [mark(1..2, "b\n")]);
    assert!(m.conflicts.is_empty());
}

#[test]
fn disk_3_changes_that_do_not_overlap_are_both_taken() {
    let (b, m, t) = ("a\nb\nc\nd\ne\n", "A\nb\nc\nd\ne\n", "a\nb\nc\nd\nE\n");
    let d = disk(b, m, t);
    assert_eq!(d.text, "A\nb\nc\nd\nE\n");
    assert_eq!(d.marks, [mark(4..5, "e\n")]);
    assert!(d.conflicts.is_empty());
}

#[test]
fn disk_3_overlapping_changes_conflict() {
    let m = disk("a\nb\nc\n", "a\nX\nc\n", "a\nY\nc\n");
    assert_eq!(m.text, "a\nX\nc\n", "mine stays");
    assert!(m.marks.is_empty());
    assert_eq!(m.conflicts, [1..2]);
}

#[test]
fn disk_3_conflicts_chain_transitively() {
    // mine changes lines 1-2 and 4 (3 stays); theirs changes 2-4. Mine's two changes are joined by theirs.
    let base = "0\n1\n2\n3\n4\n5\n";
    let mine = "0\nM1\n3\nM4\n5\n";
    let theirs = "0\n1\nT\n5\n";
    let m = disk(base, mine, theirs);
    assert_eq!(m.text, mine);
    assert!(m.marks.is_empty());
    // The region runs from base line 1 to 4: mine's lines of it are 1..4 of the merged text.
    assert_eq!(m.conflicts, [1..4]);
}

#[test]
fn disk_3_two_separate_conflicts_stay_separate() {
    let base = "a\nb\nc\nd\ne\n";
    let m = disk(base, "a\nX\nc\nY\ne\n", "a\nP\nc\nQ\ne\n");
    assert_eq!(m.conflicts, [1..2, 3..4]);
}

#[test]
fn disk_3_identical_change_on_both_sides_is_taken_once() {
    let m = disk("a\nb\nc\n", "a\nX\nc\n", "a\nX\nc\n");
    assert_eq!(m, Merged { text: "a\nX\nc\n".into(), marks: vec![], conflicts: vec![] });
    // Identical here, in conflict there.
    let m = disk("a\nb\nc\nd\ne\n", "A\nb\nc\nX\ne\n", "A\nb\nc\nY\ne\n");
    assert_eq!(m.text, "A\nb\nc\nX\ne\n");
    assert!(m.marks.is_empty());
    assert_eq!(m.conflicts, [3..4]);
}

#[test]
fn disk_3_insertions_at_the_same_point() {
    let m = disk("a\nc\n", "a\nb\nc\n", "a\nB\nc\n");
    assert_eq!(m.text, "a\nb\nc\n");
    assert!(m.marks.is_empty());
    assert_eq!(m.conflicts, [1..2]);
    // The same insertion on both sides is one insertion.
    let m = disk("a\nc\n", "a\nb\nc\n", "a\nb\nc\n");
    assert_eq!(m, Merged { text: "a\nb\nc\n".into(), marks: vec![], conflicts: vec![] });
}

#[test]
fn disk_3_insertions_at_different_points_do_not_conflict() {
    let m = disk("a\nb\n", "a\nx\nb\n", "a\nb\ny\n");
    assert_eq!(m.text, "a\nx\nb\ny\n");
    assert_eq!(m.marks, [mark(3..4, "")]);
    assert!(m.conflicts.is_empty());
}

#[test]
fn disk_3_changes_that_merely_touch_do_not_conflict() {
    // Theirs inserts right before the line mine replaced.
    let m = disk("a\nb\nc\n", "a\nB\nc\n", "a\nx\nb\nc\n");
    assert_eq!(m.text, "a\nx\nB\nc\n");
    assert_eq!(m.marks, [mark(1..2, "")]);
    assert!(m.conflicts.is_empty());
    // Theirs inserts right after it.
    let m = disk("a\nb\nc\n", "a\nB\nc\n", "a\nb\nx\nc\n");
    assert_eq!(m.text, "a\nB\nx\nc\n");
    assert_eq!(m.marks, [mark(2..3, "")]);
    assert!(m.conflicts.is_empty());
    // Mine replaces line 1 and theirs replaces line 2: adjacent, no shared line.
    let m = disk("a\nb\nc\nd\n", "a\nB\nc\nd\n", "a\nb\nC\nd\n");
    assert_eq!(m.text, "a\nB\nC\nd\n");
    assert_eq!(m.marks, [mark(2..3, "c\n")]);
    assert!(m.conflicts.is_empty());
}

#[test]
fn disk_3_insertion_inside_a_replaced_region_conflicts() {
    let m = disk("a\nb\nc\nd\n", "a\nX\nd\n", "a\nb\nY\nc\nd\n");
    assert_eq!(m.text, "a\nX\nd\n");
    assert!(m.marks.is_empty());
    assert_eq!(m.conflicts, [1..2]);
}

#[test]
fn disk_3_deletions() {
    let b = "a\nb\nc\n";
    // Mine deletes: nothing to take from theirs.
    let m = disk(b, "a\nc\n", b);
    assert_eq!(m, Merged { text: "a\nc\n".into(), marks: vec![], conflicts: vec![] });
    // Theirs deletes: marked where it was (an empty range).
    let m = disk(b, b, "a\nc\n");
    assert_eq!(m.text, "a\nc\n");
    assert_eq!(m.marks, [mark(1..1, "b\n")]);
    // Both delete the same line.
    let m = disk(b, "a\nc\n", "a\nc\n");
    assert_eq!(m, Merged { text: "a\nc\n".into(), marks: vec![], conflicts: vec![] });
    // Mine deletes, theirs edits the same line: the conflict sits at an empty range.
    let m = disk(b, "a\nc\n", "a\nB\nc\n");
    assert_eq!(m.text, "a\nc\n");
    assert_eq!(m.conflicts, [1..1]);
    // Mine edits, theirs deletes the same line.
    let m = disk(b, "a\nB\nc\n", "a\nc\n");
    assert_eq!(m.text, "a\nB\nc\n");
    assert_eq!(m.conflicts, [1..2]);
}

#[test]
fn disk_3_text_without_a_final_newline() {
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
    assert_eq!(m, Merged { text: "a\nb".into(), marks: vec![], conflicts: vec![] });
    // Both add the same final line without a newline.
    let m = disk("a\n", "a\nb", "a\nb");
    assert_eq!(m, Merged { text: "a\nb".into(), marks: vec![], conflicts: vec![] });
    // Appending to a text without a final newline.
    let m = disk("a\nb", "a\nb", "a\nb\nc\n");
    assert_eq!(m.text, "a\nb\nc\n");
}

#[test]
fn disk_3_a_last_line_without_newline_never_joins_a_line_added_after_it() {
    // Mine ends with "x" (no newline); theirs adds a line after the old last line. Applying both would
    // glue them into "xd", so these overlap.
    let m = disk("a\nb\n", "a\nx", "a\nb\nd\n");
    assert_eq!(m.text, "a\nx");
    assert_eq!(m.conflicts, [1..2]);
    // And the other way round.
    let m = disk("a\nb\n", "a\nb\nd\n", "a\nx");
    assert_eq!(m.text, "a\nb\nd\n");
    assert_eq!(m.conflicts, [1..3]);
}

#[test]
fn disk_3_empty_texts() {
    let m = disk("", "", "");
    assert_eq!(m, Merged { text: String::new(), marks: vec![], conflicts: vec![] });
    // Only mine writes into an empty file.
    assert_eq!(disk("", "a\n", "").text, "a\n");
    // Only theirs does.
    let m = disk("", "", "b\n");
    assert_eq!(m.text, "b\n");
    assert_eq!(m.marks, [mark(0..1, "")]);
    // Both write something different.
    let m = disk("", "a\n", "b\n");
    assert_eq!(m.text, "a\n");
    assert_eq!(m.conflicts, [0..1]);
    // Mine empties the file, theirs edits it.
    let m = disk("a\n", "", "b\n");
    assert_eq!(m.text, "");
    assert_eq!(m.conflicts, [0..0]);
    // Both empty it.
    assert_eq!(disk("a\n", "", "").text, "");
    // Theirs empties it.
    let m = disk("a\nb\n", "a\nb\n", "");
    assert_eq!(m.text, "");
    assert_eq!(m.marks, [mark(0..0, "a\nb\n")]);
}

#[test]
fn disk_3_one_conflict_among_many_marks() {
    let base: String = (0..10).map(|i| format!("l{i}\n")).collect();
    let mut mine: Vec<String> = lines(&base).iter().map(|s| s.to_string()).collect();
    let mut theirs = mine.clone();
    mine[1] = "mine1\n".into();
    theirs[1] = "theirs1\n".into(); // conflict
    theirs[4] = "theirs4\n".into(); // mark
    mine[7] = "mine7\n".into(); // mine only
    theirs[9] = "theirs9\n".into(); // mark
    let m = disk(&base, &mine.concat(), &theirs.concat());
    assert_eq!(m.conflicts, [1..2]);
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
// DISK-4: properties
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
    for c in &m.conflicts {
        prop_assert!(c.start <= c.end && c.end <= n, "conflict out of bounds: {c:?} in {n} lines");
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
    for w in m.conflicts.windows(2) {
        prop_assert!(
            w[0].end <= w[1].start,
            "conflicts overlap or are unsorted: {:?}",
            m.conflicts
        );
    }
    for w in m.marks.windows(2) {
        prop_assert!(
            w[0].lines.end <= w[1].lines.start,
            "marks overlap or are unsorted: {:?}",
            m.marks
        );
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10_000))]

    // 1. If mine is the base, the text is theirs and nothing conflicts.
    #[test]
    fn disk_4_the_text_is_theirs_when_mine_is_the_base((b, _m, t) in trio()) {
        let r = disk(&b, &b, &t);
        prop_assert_eq!(&r.text, &t);
        prop_assert!(r.conflicts.is_empty());
    }

    // 2. If theirs is the base, nothing comes in: the text is mine and there is nothing left out.
    #[test]
    fn disk_4_nothing_comes_in_when_theirs_is_the_base((b, m, _t) in trio()) {
        let r = disk(&b, &m, &b);
        prop_assert_eq!(r, Merged { text: m.clone(), marks: vec![], conflicts: vec![] });
    }

    // 3. If both sides made the same text, there is nothing to take or leave out.
    #[test]
    fn disk_4_identical_sides_leave_mine_untouched((b, m, _t) in trio()) {
        let r = disk(&b, &m, &m);
        prop_assert_eq!(r, Merged { text: m.clone(), marks: vec![], conflicts: vec![] });
    }

    // 4. What conflicts does not depend on whose side is whose: swapping the roles finds as many.
    #[test]
    fn disk_4_the_conflicts_are_the_same_with_the_roles_swapped((b, m, t) in trio()) {
        let (a, s) = (disk(&b, &m, &t), disk(&b, &t, &m));
        prop_assert_eq!(a.conflicts.len(), s.conflicts.len());
    }

    // 5. Nothing is lost. A line "changed" by a side is one in an inserted or replaced part of the
    //    Myers diff from the base to that side. For every distinct line x: (a) the merged text has x
    //    at least as often as mine changed to x; (b) when nothing conflicts, it has x at least as
    //    often as theirs changed to x too. What conflicts is exactly what is left out of theirs, and
    //    SAFE-7 keeps it on the disk.
    #[test]
    fn disk_4_no_line_a_side_changed_is_lost((b, m, t) in trio()) {
        let (bl, ml, tl) = (lines(&b), lines(&m), lines(&t));
        let (cm, ct) = (changed(&bl, &ml), changed(&bl, &tl));
        let r = disk(&b, &m, &t);
        let merged = lines(&r.text);
        for x in cm.iter().chain(&ct) {
            prop_assert!(line_count(&cm, x) <= line_count(&merged, x), "mine's {x:?} lost: {r:?}");
            if r.conflicts.is_empty() {
                prop_assert!(line_count(&ct, x) <= line_count(&merged, x), "theirs' {x:?} lost: {r:?}");
            }
        }
    }

    // 6. A side that changed nothing never conflicts.
    #[test]
    fn disk_4_a_side_that_changed_nothing_conflicts_with_nothing((b, m, t) in trio()) {
        prop_assert!(disk(&b, &m, &b).conflicts.is_empty());
        prop_assert!(disk(&b, &b, &t).conflicts.is_empty());
    }

    // 7. Conflicts and marks are sorted, disjoint and inside the merged text.
    #[test]
    fn disk_4_conflicts_and_marks_are_sorted_disjoint_and_in_bounds((b, m, t) in trio()) {
        check_well_formed(&disk(&b, &m, &t))?;
    }

    // 8. Putting back what every mark replaced gives mine: marks are exactly the changes taken in.
    #[test]
    fn disk_4_undoing_the_marks_gives_mine((b, m, t) in trio()) {
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
    fn disk_4_an_expired_deadline_keeps_the_invariants((b, m, t) in trio()) {
        let r = merge3(&b, &m, &t, Some(Instant::now()));
        check_well_formed(&r)?;
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
}

// ---------------------------------------------------------------------------------------------
// DISK-5: speed
// ---------------------------------------------------------------------------------------------

#[test]
fn disk_5_a_deadline_in_the_past_still_gives_a_merge() {
    let base: String = (0..2000).map(|i| format!("line {i}\n")).collect();
    let mine: String = (0..2000)
        .map(|i| if i % 50 == 3 { format!("mine {i}\n") } else { format!("line {i}\n") })
        .collect();
    let theirs: String = (0..2000)
        .map(|i| if i % 70 == 5 { format!("theirs {i}\n") } else { format!("line {i}\n") })
        .collect();
    let m = merge3(&base, &mine, &theirs, Some(Instant::now()));
    assert!(!m.text.is_empty());
    assert!(!m.conflicts.is_empty() || !m.marks.is_empty());
}

/// 100,000 lines with scattered edits on both sides (replaced, deleted and inserted lines, a few
/// of which collide), merged with a 50 ms deadline. Run: cargo test --release -- --ignored bench --nocapture
#[test]
#[ignore]
fn disk_5_bench_merge_of_100k_lines_finishes_within_the_deadline() {
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
    for _ in 0..5 {
        let start = Instant::now();
        let m = merge3(&base, &mine, &theirs, Some(start + Duration::from_millis(50)));
        let took = start.elapsed();
        assert!(!m.text.is_empty() && (m.conflicts.len() + m.marks.len()) > 50);
        times.push(took);
    }
    for took in &times {
        eprintln!("bench: {took:?}");
    }
    let worst = times.iter().copied().max().unwrap_or_default();
    assert!(worst < Duration::from_millis(50), "merge of {N} lines took {worst:?}");
}

/// The worst case for a diff: every line moved, or the same few lines everywhere. The deadline is
/// shared by both diffs, so the whole merge gives up near 50 ms (a little slack for the final walk).
#[test]
#[ignore]
fn disk_5_bench_hard_inputs_give_up_at_the_deadline() {
    const N: usize = 100_000;
    let text = |f: &dyn Fn(usize) -> usize| -> String {
        (0..N).map(|i| format!("alpha {}\n", f(i))).collect()
    };
    let (ordered, reversed) = (text(&|i| i), text(&|i| N - 1 - i));
    let (few, other_few) = (text(&|i| i % 50), text(&|i| (i * 7919) % 100));
    for (base, mine, theirs) in [(&ordered, &reversed, &few), (&few, &other_few, &ordered)] {
        let start = Instant::now();
        let m = merge3(base, mine, theirs, Some(start + Duration::from_millis(50)));
        let took = start.elapsed();
        eprintln!("bench hard: {took:?}");
        assert!(!m.text.is_empty());
        assert!(took < Duration::from_millis(75), "gave up too late: {took:?}");
    }
}

// ---------------------------------------------------------------------------------------------
// VIS-2: words
// ---------------------------------------------------------------------------------------------

/// The deadline of DISK-5 reaches the word diff too: past it, the answer is coarse but still
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
    assert_eq!(words("名前: alice", "名前: alice2"), (vec![4..9], vec![4..10]));
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
