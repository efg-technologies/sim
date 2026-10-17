//! DISK-1, DISK-2, DISK-5, DISK-6, SAFE-5 on App: what the buffer does when the file changes on
//! disk (reload, merge, marks, hunks), and how a hunk or a mark survives edits. The merge itself
//! (diff3) is tests/proposal.rs; the watcher is tests/watch.rs; the screen is `vis_*` below.
// Helpers may unwrap; expected values are written as `vec![a..b]` on purpose: a list holding one range.
#![allow(clippy::unwrap_used, clippy::single_range_in_vec_init)]
mod common;
use common::*;
use proptest::prelude::*;
use sim_core::core::app::{App, Effect, Event};
use sim_core::core::buffer::{ABSENT, Buffer, hash};
use std::time::{Duration, Instant};

fn named(text: &str) -> App {
    app_with(Some("f"), text.as_bytes(), (130, 12))
}

/// The file now holds `text`.
fn disk(a: &mut App, text: &str) -> Vec<Effect> {
    a.update(Event::Disk {
        bytes: Some(text.as_bytes().to_vec()),
        hash: None,
        deadline: far(),
        seq: 0,
    })
}

fn msg(a: &mut App) -> String {
    status(&screen(a)).to_string()
}

fn write(fx: &[Effect]) -> Option<(&[u8], Option<u64>)> {
    fx.iter().find_map(|e| match e {
        Effect::Write { bytes, expect_hash, .. } => Some((bytes.as_slice(), *expect_hash)),
        _ => None,
    })
}

fn quits(fx: &[Effect]) -> bool {
    fx.iter().any(|e| matches!(e, Effect::Quit(_)))
}

/// (line, symbol) of every mark.
fn marks(a: &App) -> Vec<(usize, char)> {
    a.buf.marks.iter().map(|m| (m.line, m.kind)).collect()
}

/// The text of the buffer with the cursor's line and column.
fn at(a: &App) -> (String, usize, usize) {
    (text(a), a.buf.line_of(a.buf.cursor), a.buf.col())
}

// ---------------------------------------------------------------------------------------------
// DISK-2: a clean buffer reloads and marks what changed
// ---------------------------------------------------------------------------------------------

#[test]
fn disk_2_a_clean_buffer_reloads_and_marks_added_changed_and_deleted_lines() {
    let mut a = named("alpha\nbeta gamma\ndelta\nepsilon\nzeta\n");
    feed(&mut a, "jj");
    let fx = disk(&mut a, "alpha\nbeta GAMMA\ndelta\nzeta\neta\n");
    assert_eq!(text(&a), "alpha\nbeta GAMMA\ndelta\nzeta\neta");
    assert!(!a.buf.dirty() && !a.diverged(), "a reload is clean");
    assert_eq!(marks(&a), [(1, '~'), (3, '-'), (4, '+')]);
    let w = |i: usize| words(&a.buf.marks[i]);
    assert_eq!((w(0), w(1), w(2)), (Some(vec![5..10]), Some(vec![]), None), "None: the whole line");
    assert_eq!(a.buf.base.to_string(), text(&a), "the disk text is the new base");
    assert_eq!(status(last_frame(&fx)), "f  3:1  NORMAL  2 changes on disk");
    assert_eq!(at(&a).1, 2, "the cursor still points at `delta`");
}

#[test]
fn disk_2_a_run_of_added_lines_is_one_change_and_every_line_is_marked() {
    let mut a = named("a\nz\n");
    let fx = disk(&mut a, "a\nb\nc\nd\nz\n");
    assert_eq!(marks(&a), [(1, '+'), (2, '+'), (3, '+')]);
    assert_eq!(status(last_frame(&fx)), "f  1:1  NORMAL  1 change on disk");
}

#[test]
fn disk_2_a_reload_is_one_undo_step_and_marks_come_back_with_redo() {
    let mut a = named("one\ntwo\nthree\n");
    disk(&mut a, "one\nTWO\nthree\nfour\n");
    let after = at(&a);
    assert_eq!(marks(&a), [(1, '~'), (3, '+')]);
    feed(&mut a, "u");
    assert_eq!(
        (text(&a).as_str(), a.buf.dirty()),
        ("one\ntwo\nthree", true),
        "the old text, which is not the file's"
    );
    assert!(a.buf.marks.is_empty(), "the marks belong to the step");
    assert_eq!(msg(&mut a), "f  1:1  NORMAL  modified");
    feed(&mut a, "u");
    assert_eq!(msg(&mut a), "nothing to undo", "it was one step");
    feed(&mut a, "<C-r>");
    assert_eq!((text(&a).as_str(), marks(&a)), ("one\nTWO\nthree\nfour", vec![(1, '~'), (3, '+')]));
    assert_eq!(at(&a).0, after.0);
    assert!(!a.buf.dirty(), "redo is the disk text again");
}

#[test]
fn disk_2_a_dirty_buffer_takes_in_what_does_not_conflict_and_keeps_the_cursor_on_its_text() {
    let mut a = named("a1\nb2\nc3\nd4\ne5\n");
    feed(&mut a, "j$a mine\x1bl");
    assert_eq!(at(&a), ("a1\nb2 mine\nc3\nd4\ne5".into(), 1, 6));
    // The file gained two lines above and changed line 4.
    disk(&mut a, "x\ny\na1\nb2\nc3\nD4\ne5\n");
    assert_eq!(text(&a), "x\ny\na1\nb2 mine\nc3\nD4\ne5", "both edits are there");
    assert_eq!(marks(&a), [(0, '+'), (1, '+'), (5, '~')], "only the disk's are marked");
    assert_eq!((at(&a).1, at(&a).2), (3, 6), "the cursor is still on `mine`, same column");
    assert!(!a.diverged() && a.buf.dirty());
    assert_eq!(a.buf.base.to_string(), "x\ny\na1\nb2\nc3\nD4\ne5", "the disk is the new base");
    assert_eq!(msg(&mut a), "f  4:7  NORMAL  modified · 2 changes on disk");
    // One undo step takes the merge back, and leaves the user's own edit.
    feed(&mut a, "u");
    assert_eq!(text(&a), "a1\nb2 mine\nc3\nd4\ne5");
    assert!(a.buf.marks.is_empty());
    feed(&mut a, "<C-r>");
    assert_eq!(text(&a), "x\ny\na1\nb2 mine\nc3\nD4\ne5");
    assert_eq!(marks(&a), [(0, '+'), (1, '+'), (5, '~')]);
}

#[test]
fn disk_2_a_merge_that_leaves_the_buffer_equal_to_the_disk_is_clean() {
    let mut a = named("one\ntwo\n");
    feed(&mut a, "jcwTWO\x1b");
    assert!(a.buf.dirty());
    disk(&mut a, "one\nTWO\n");
    assert_eq!(text(&a), "one\nTWO");
    assert!(!a.buf.dirty(), "the same text as the file");
    assert!(a.buf.marks.is_empty(), "nothing came in");
}

#[test]
fn disk_2_a_conflict_leaves_mine_marks_its_lines_and_diverges_the_buffer() {
    let mut a = named("one\ntwo words here\nthree\nfour\n");
    feed(&mut a, "jwcwMINE\x1b");
    let fx = disk(&mut a, "one\ntwo THEIRS here\nthree\nFOUR\n");
    assert_eq!(
        text(&a),
        "one\ntwo MINE here\nthree\nFOUR",
        "mine in the buffer, the rest taken in"
    );
    assert_eq!(
        marks(&a),
        [(1, '~'), (3, '~')],
        "the conflict's line, and the line the disk changed"
    );
    assert_eq!(words(&a.buf.marks[0]), None, "the whole line: the disk's words are not here");
    assert!(a.diverged());
    assert_eq!(
        status(last_frame(&fx)),
        "f  2:8  NORMAL  modified · 2 changes on disk · conflicts with the disk"
    );
    // Nothing is lost: undo puts the text back and redo brings the marks back, and the disk's
    // version is kept all the while.
    feed(&mut a, "u");
    assert!(a.buf.marks.is_empty() && a.diverged());
    feed(&mut a, "<C-r>");
    assert_eq!(marks(&a), [(3, '~')], "and the one on the cursor's line is seen, as ever");
}

#[test]
fn disk_2_several_conflicts_are_marked_line_by_line() {
    let mut a = named("a\nb\nc\nd\ne\nf\n");
    feed(&mut a, "rAjjrCjjrE");
    disk(&mut a, "a2\nb\nc2\nd\ne2\nf\n");
    assert_eq!(text(&a), "A\nb\nC\nd\nE\nf");
    assert_eq!(marks(&a), [(0, '~'), (2, '~'), (4, '~')]);
    assert_eq!(
        msg(&mut a),
        "f  5:1  NORMAL  modified · 3 changes on disk · conflicts with the disk"
    );
}

#[test]
fn disk_2_a_deleted_line_that_mine_edited_is_a_conflict_marked_on_mine() {
    let mut a = named("one\ntwo\nthree\n");
    feed(&mut a, "jx");
    disk(&mut a, "one\nthree\n");
    assert_eq!(text(&a), "one\nwo\nthree", "mine is kept");
    assert_eq!(marks(&a), [(1, '~')]);
    assert!(a.diverged());
}

#[test]
fn disk_2_a_second_change_moves_the_marks_of_the_first() {
    let mut a = named("a\nb\nc\nd\ne\n");
    feed(&mut a, "jjrC");
    disk(&mut a, "a\nb\nc2\nd\ne2\n");
    assert_eq!(marks(&a), [(2, '~'), (4, '~')]);
    disk(&mut a, "n1\nn2\na\nb\nc2\nd\ne2\n");
    assert_eq!(text(&a), "n1\nn2\na\nb\nC\nd\ne2");
    assert_eq!(
        marks(&a),
        [(0, '+'), (1, '+'), (4, '~'), (6, '~')],
        "the old marks moved down two lines"
    );
    // A change of the very line that carries a mark replaces the mark.
    disk(&mut a, "n1\nn2\na\nb\nc2\nd\ne3\n");
    assert_eq!(marks(&a), [(0, '+'), (1, '+'), (4, '~'), (6, '~')]);
    assert_eq!(words(&a.buf.marks[3]), Some(vec![0..2]));
    assert!(a.diverged(), "the first conflict is still there");
}

#[test]
fn disk_2_the_format_of_the_file_on_disk_is_adopted() {
    let mut a = named("one\ntwo\n");
    disk(&mut a, "\u{feff}one\r\ntwo\r\nthree");
    assert_eq!(text(&a), "one\ntwo\nthree");
    assert_eq!(
        a.buf.encode(),
        "\u{feff}one\r\ntwo\r\nthree".as_bytes(),
        "saved as the file now is"
    );
    assert!(!a.buf.dirty());
}

#[test]
fn disk_2_a_file_that_is_no_longer_text_is_not_merged() {
    let mut a = named("one\ntwo\n");
    feed(&mut a, "x");
    let before = at(&a);
    let fx = a.update(Event::Disk {
        bytes: Some(b"one\xff\x00two".to_vec()),
        hash: None,
        deadline: far(),
        seq: 0,
    });
    assert!(status(last_frame(&fx)).starts_with("changed on disk, not as text: not merged"));
    assert_eq!(at(&a), before);
    assert!(a.buf.marks.is_empty() && a.buf.dirty());
    // The save is still guarded: the disk was not taken as the base, and what it holds is kept
    // (SAFE-7) before a forced save writes over it.
    assert!(a.diverged());
    assert!(write(&feed(&mut a, ":w\n")).is_none());
    let fx = feed(&mut a, ":w!\n");
    let kept = fx.iter().find_map(|e| match e {
        Effect::Keep { bytes, .. } => Some(bytes.clone()),
        _ => None,
    });
    assert_eq!(kept.as_deref(), Some(&b"one\xff\x00two"[..]));
}

#[test]
fn disk_2_a_read_only_buffer_says_so_and_stays() {
    let mut a = app_with(Some("f"), b"a\xff\n", (60, 8));
    let before = text(&a);
    let fx = disk(&mut a, "new\n");
    assert!(status(last_frame(&fx)).starts_with("changed on disk, not as text"));
    assert_eq!(text(&a), before);
}

// ---------------------------------------------------------------------------------------------
// DISK-1: our own saves are not changes; a deleted file; asking for a watch
// ---------------------------------------------------------------------------------------------

#[test]
fn disk_1_an_event_with_the_hash_we_know_is_ignored() {
    let mut a = named("one\ntwo\n");
    let before = a.buf.version();
    disk(&mut a, "one\ntwo\n");
    assert_eq!((a.buf.version(), a.buf.marks.is_empty()), (before, true), "the base itself");
    feed(&mut a, "x");
    let v = a.buf.version();
    disk(&mut a, "one\ntwo\n");
    assert_eq!(a.buf.version(), v, "not even a dirty buffer is touched by it");
    assert_eq!(text(&a), "ne\ntwo");
}

#[test]
fn disk_1_a_disk_event_brings_the_hash_of_its_content_so_that_no_key_hashes_it() {
    let mut a = named("one\ntwo\n");
    let before = (text(&a), a.buf.version());
    // The hash is the reader's word: content that hashes to what App knows is not looked at again.
    let same = hash(b"one\ntwo\n");
    a.update(Event::Disk {
        bytes: Some(b"quite different\n".to_vec()),
        hash: Some(same),
        deadline: far(),
        seq: 0,
    });
    assert_eq!((text(&a), a.buf.version()), before, "the given hash says it is the file we know");
    // A different hash is a change, and becomes what is known: the same again is ignored.
    let new = b"one\nTWO\n".to_vec();
    a.update(Event::Disk {
        bytes: Some(new.clone()),
        hash: Some(hash(&new)),
        deadline: far(),
        seq: 0,
    });
    assert_eq!(marks(&a), [(1, '~')]);
    let v = a.buf.version();
    a.update(Event::Disk {
        bytes: Some(new.clone()),
        hash: Some(hash(&new)),
        deadline: far(),
        seq: 0,
    });
    assert_eq!(a.buf.version(), v);
    // Without one, App makes it: the same result.
    let mut b = named("one\ntwo\n");
    disk(&mut b, "one\nTWO\n");
    assert_eq!((text(&b), marks(&b)), (text(&a), marks(&a)));
    // A file that is gone has no content to hash.
    a.update(Event::Disk { bytes: None, hash: None, deadline: far(), seq: 0 });
    assert!(a.gone);
}

#[test]
fn disk_1_our_own_save_is_ignored_while_it_is_in_flight_and_after() {
    let mut a = named("one\ntwo\n");
    feed(&mut a, "x");
    let fx = feed(&mut a, ":w\n");
    let (bytes, _) = write(&fx).unwrap();
    let (bytes, saved) = (bytes.to_vec(), hash(bytes));
    // The watcher can be quicker than the thread that saved.
    a.update(Event::Disk { bytes: Some(bytes.clone()), hash: None, deadline: far(), seq: 0 });
    assert_eq!((text(&a).as_str(), a.buf.marks.is_empty(), a.diverged()), ("ne\ntwo", true, false));
    feed(&mut a, "ix\x1b"); // edited while it runs
    a.update(Event::Saved(Ok((saved, None)), 0));
    a.update(Event::Disk { bytes: Some(bytes), hash: None, deadline: far(), seq: 0 });
    assert_eq!(text(&a), "xne\ntwo", "and when the answer came first");
    assert!(a.buf.marks.is_empty() && !a.diverged());
    assert!(a.buf.dirty(), "only the saved text is clean");
}

#[test]
fn disk_1_a_deleted_file_keeps_its_text_and_counts_as_modified() {
    let mut a = named("one\ntwo\n");
    assert!(!a.buf.dirty());
    let fx = a.update(Event::Disk { bytes: None, hash: None, deadline: far(), seq: 0 });
    assert_eq!(text(&a), "one\ntwo");
    assert!(a.buf.dirty());
    assert_eq!(status(last_frame(&fx)), "f  1:1  NORMAL  modified · deleted on disk");
    let fx = feed(&mut a, ":q\n");
    assert!(!quits(&fx), "`:q` is refused: the text is only here");
    assert_eq!(msg(&mut a), "no write since last change (:q! to override)");
    // Telling it twice changes nothing.
    a.update(Event::Disk { bytes: None, hash: None, deadline: far(), seq: 0 });
    // The next save recreates the file: it expects no file.
    let fx = feed(&mut a, ":w\n");
    assert_eq!(write(&fx).unwrap(), (&b"one\ntwo\n"[..], Some(ABSENT)));
    a.update(Event::Saved(Ok((hash(b"one\ntwo\n"), None)), 0));
    assert!(!a.buf.dirty() && !a.gone);
    assert_eq!(msg(&mut a), "saved f");
    let fx = feed(&mut a, ":w\n");
    assert_eq!(write(&fx).unwrap().1, Some(hash(b"one\ntwo\n")), "and the file is known again");
}

#[test]
fn disk_1_a_file_created_again_is_merged() {
    let mut a = named("one\ntwo\n");
    feed(&mut a, "x");
    a.update(Event::Disk { bytes: None, hash: None, deadline: far(), seq: 0 });
    disk(&mut a, "one\ntwo\nthree\n");
    assert_eq!(text(&a), "ne\ntwo\nthree", "mine and the new file");
    assert!(!a.gone && a.buf.dirty());
    assert_eq!(marks(&a), [(2, '+')]);
    let fx = feed(&mut a, ":w\n");
    assert_eq!(write(&fx).unwrap().1, Some(hash(b"one\ntwo\nthree\n")));
    // Clean and deleted, then created again.
    let mut a = named("one\n");
    a.update(Event::Disk { bytes: None, hash: None, deadline: far(), seq: 0 });
    disk(&mut a, "one\nnew\n");
    assert_eq!((text(&a).as_str(), a.buf.dirty(), a.gone), ("one\nnew", false, false));
}

#[test]
fn disk_1_a_buffer_without_a_file_ignores_the_disk() {
    let mut a = app("one\n");
    disk(&mut a, "other\n");
    assert_eq!(text(&a), "one");
}

#[test]
fn disk_1_the_app_asks_to_watch_the_file_it_has() {
    let watched = |fx: &[Effect]| {
        fx.iter().find_map(|e| if let Effect::Watch(p) = e { Some(p.clone()) } else { None })
    };
    let mut a = named("one\n");
    feed(&mut a, ":e other.txt\n");
    let fx = a.update(Event::Opened("other.txt".into(), Ok(Some(b"x\n".to_vec())), 0));
    assert_eq!(watched(&fx).as_deref(), Some("other.txt"));
    // An unnamed buffer takes the name of its first save, and is watched from then on.
    let mut a = app("");
    feed(&mut a, "ihi\x1b:w new.txt\n");
    let fx = a.update(Event::Saved(Ok((hash(b"hi\n"), None)), 0));
    assert_eq!(watched(&fx).as_deref(), Some("new.txt"));
    // The shell silences the watcher when an `:e` begins to read (DISK-1), so every answer, also a
    // refused one, is followed by a watch of the file the buffer has: here the one it keeps.
    let mut a = named("one\n");
    feed(&mut a, ":e other.txt\nx");
    let fx = a.update(Event::Opened("other.txt".into(), Ok(Some(b"x\n".to_vec())), 0));
    assert_eq!(watched(&fx).as_deref(), Some("f"), "edited meanwhile");
    let mut a = named("one\n");
    feed(&mut a, ":e other.txt\n");
    a.update(Event::Disk { bytes: Some(b"new\n".to_vec()), hash: None, deadline: far(), seq: 9 });
    let fx = a.update(Event::Opened("f".into(), Ok(Some(b"one\n".to_vec())), 1));
    assert_eq!(
        watched(&fx).as_deref(),
        Some("f"),
        "a read of the same file older than a newer one"
    );
    // An unnamed buffer has nothing to watch, and an answer nobody asked for is followed by a watch too.
    let mut a = app("");
    feed(&mut a, ":e other.txt\nihi\x1b");
    let fx = a.update(Event::Opened("other.txt".into(), Ok(Some(b"x\n".to_vec())), 0));
    assert_eq!(watched(&fx), None);
    let mut a = named("one\n");
    let fx = a.update(Event::Opened("other.txt".into(), Ok(Some(b"x\n".to_vec())), 0));
    assert_eq!(watched(&fx).as_deref(), Some("f"));
}

// ---------------------------------------------------------------------------------------------
// A newer say of the disk on the same lines replaces the older (DISK-2)
// ---------------------------------------------------------------------------------------------

// ---------------------------------------------------------------------------------------------
// The view follows a merge like the cursor
// ---------------------------------------------------------------------------------------------

#[test]
fn disk_2_the_view_does_not_jump_when_lines_come_in_above_it() {
    let text: String = (0..200).map(|i| format!("line {i}\n")).collect();
    let mut a = app_with(Some("f"), text.as_bytes(), (40, 30));
    feed(&mut a, "100Gzz");
    let shown = |a: &mut App| -> Vec<String> {
        screen(a).rows[..29].iter().map(|r| r.text.chars().skip(5).collect()).collect()
    };
    let before = shown(&mut a);
    assert!(before.iter().any(|l| l == "line 99"), "{before:?}");
    // Ten lines come in at the top, and a change lands further down.
    let new = format!("{}{}", "new\n".repeat(10), text.replace("line 150\n", "LINE 150\n"));
    disk(&mut a, &new);
    assert_eq!(a.buf.line_of(a.buf.cursor), 109, "the cursor kept its text");
    assert_eq!(shown(&mut a), before, "and so did the view: the same lines on the same rows");
    // Lines that go away above do the same.
    let gone = new.replacen("new\n", "", 4);
    feed(&mut a, "160Gzz");
    let before = shown(&mut a);
    disk(&mut a, &gone);
    assert_eq!(shown(&mut a), before);
}

// ---------------------------------------------------------------------------------------------
// SAFE-5: a save that finds the disk changed merges it and writes nothing
// ---------------------------------------------------------------------------------------------

#[test]
fn safe_5_a_save_that_finds_another_file_merges_and_does_not_save() {
    let mut a = named("one\ntwo\nthree\n");
    feed(&mut a, "rO");
    let fx = feed(&mut a, ":wq\n");
    assert!(write(&fx).is_some());
    let fx = a.update(Event::SaveConflict(Some(b"one\ntwo\nTHREE\n".to_vec()), far(), 0));
    assert!(write(&fx).is_none() && !quits(&fx), "nothing is saved again, and :wq does not quit");
    assert_eq!(text(&a), "One\ntwo\nTHREE");
    assert_eq!(marks(&a), [(2, '~')]);
    assert_eq!(
        status(last_frame(&fx)),
        "not saved: the file changed on disk; merged its changes (:w to save)"
    );
    assert!(a.buf.dirty());
    // The next `:wq` is checked against what was merged, and then it quits.
    let fx = feed(&mut a, ":wq\n");
    let (bytes, expect) = write(&fx).unwrap();
    assert_eq!((bytes, expect), (&b"One\ntwo\nTHREE\n"[..], Some(hash(b"one\ntwo\nTHREE\n"))));
    let fx = a.update(Event::Saved(Ok((hash(bytes), None)), 0));
    assert!(quits(&fx));
}

#[test]
fn safe_5_a_save_conflict_for_content_already_merged_makes_no_second_undo_step() {
    // The watcher's read comes first, the save's answer second, with the same content.
    let ways = ":w! keeps yours · :e! takes the disk's · the other is copied first";
    for (mine, theirs, said) in [
        (
            "rO",
            "one\ntwo\nTHREE\n",
            "not saved: the file changed on disk; merged its changes (:w to save)".to_string(),
        ),
        (
            "rO",
            "OTHER\ntwo\nthree\n",
            format!("not saved: the disk changed and conflicts · {ways}"),
        ),
    ] {
        let mut a = named("one\ntwo\nthree\n");
        feed(&mut a, mine);
        feed(&mut a, ":w\n");
        disk(&mut a, theirs);
        let (merged, version, marks_) = (text(&a), a.buf.version(), marks(&a));
        let m2 = marks_.clone();
        let fx = a.update(Event::SaveConflict(Some(theirs.as_bytes().to_vec()), far(), 0));
        assert!(write(&fx).is_none() && !quits(&fx));
        assert_eq!(status(last_frame(&fx)), said);
        assert_eq!(a.buf.version(), version, "nothing new happened to the text");
        assert_eq!((text(&a), marks(&a)), (merged.clone(), marks_));
        feed(&mut a, "u");
        assert_ne!(
            (text(&a), marks(&a)),
            (merged, m2),
            "one u takes the merge back, not a step that did nothing"
        );
        feed(&mut a, "u");
        assert_eq!(text(&a), "one\ntwo\nthree");
    }
}

#[test]
fn safe_5_a_save_that_finds_no_file_says_so_and_the_next_one_creates_it() {
    let mut a = named("one\n");
    feed(&mut a, "x:w\n");
    let fx = a.update(Event::SaveConflict(None, far(), 0));
    assert_eq!(
        status(last_frame(&fx)),
        "not saved: the file was deleted on disk (:w to create it again)"
    );
    assert!(a.gone && a.buf.dirty());
    let fx = feed(&mut a, ":w\n");
    assert_eq!(write(&fx).unwrap(), (&b"ne\n"[..], Some(ABSENT)));
}

#[test]
fn safe_5_the_disk_content_goes_through_the_same_merge_as_a_watched_change() {
    // Same inputs, same buffer: through Disk, or through SaveConflict.
    let build = || {
        let mut a = named("a b c\nd\ne f g\n");
        feed(&mut a, "wrX");
        a
    };
    let new = "a b c\nd\nE f g\n";
    let (mut x, mut y) = (build(), build());
    disk(&mut x, new);
    feed(&mut y, ":w\n");
    y.update(Event::SaveConflict(Some(new.as_bytes().to_vec()), far(), 0));
    assert_eq!((text(&x), marks(&x), x.diverged()), (text(&y), marks(&y), y.diverged()));
    assert_eq!(text(&x), "a X c\nd\nE f g");
}

#[test]
fn safe_5_a_conflict_for_another_file_name_is_not_a_merge() {
    let mut a = named("one\n");
    feed(&mut a, ":w other\n");
    a.update(Event::SaveConflict(Some(b"something else\n".to_vec()), far(), 0));
    assert_eq!(msg(&mut a), "not saved: other exists (:w! to overwrite)");
    assert_eq!(text(&a), "one");
}

// ---------------------------------------------------------------------------------------------
// DISK-6: a mark stays until the cursor enters its line
// ---------------------------------------------------------------------------------------------

fn six() -> App {
    let mut a = named("a\nb\nc\nd\ne\nf\n");
    disk(&mut a, "a\nB\nc\nd\nE\nf\n");
    assert_eq!(marks(&a), [(1, '~'), (4, '~')]);
    a
}

#[test]
fn disk_6_a_mark_stays_until_the_cursor_enters_its_line() {
    let mut a = six();
    feed(&mut a, "l$"); // moving along the line does not enter another one
    assert_eq!(marks(&a), [(1, '~'), (4, '~')]);
    feed(&mut a, "j"); // onto line 1
    assert_eq!(marks(&a), [(4, '~')]);
    feed(&mut a, "kj");
    assert_eq!(marks(&a), [(4, '~')], "seen once: gone for good");
    feed(&mut a, "G"); // every kind of motion counts
    assert_eq!(marks(&a), [(4, '~')], "line 5 is not the last line");
    feed(&mut a, "k");
    assert!(a.buf.marks.is_empty());
    assert_eq!(msg(&mut a), "f  5:1  NORMAL");
}

#[test]
fn disk_6_the_lines_a_merge_marked_as_written_are_listed_for_the_host() {
    let mut a = named("alpha\nbeta gamma\ndelta\nepsilon\nzeta\n");
    assert_eq!(a.wrote, Vec::<usize>::new());
    disk(&mut a, "alpha\nbeta GAMMA\ndelta\nzeta\neta\n");
    assert_eq!(marks(&a), [(1, '~'), (3, '-'), (4, '+')]);
    assert_eq!(a.wrote, [1, 4], "the changed line and the added one; a deletion point has no text");
    // the host reads and clears the list; a write of the same text lists nothing new
    a.wrote.clear();
    disk(&mut a, "alpha\nbeta GAMMA\ndelta\nzeta\neta\n");
    assert_eq!(a.wrote, Vec::<usize>::new());
    // a line of a conflict is listed: the disk wrote it too, though mine stays
    let mut a = named("a\nb\nc\nd\ne\nf\n");
    feed(&mut a, "jrXjjjrY");
    disk(&mut a, "a\nB\nc\nd\nE\nf\n");
    assert_eq!((marks(&a), a.wrote.clone()), (vec![(1, '~'), (4, '~')], vec![1, 4]));
}

#[test]
fn disk_6_a_search_a_jump_and_a_count_enter_lines_too() {
    for keys in ["/E\n", ":5\n", "4j", "5G"] {
        let mut a = six();
        feed(&mut a, keys);
        assert_eq!(marks(&a), [(1, '~')], "{keys:?}");
    }
}

#[test]
fn disk_6_a_mark_under_the_cursor_when_the_change_arrives_stays_until_the_next_key() {
    let mut a = named("a\nb\nc\n");
    feed(&mut a, "j");
    disk(&mut a, "a\nB\nc\n");
    assert_eq!(marks(&a), [(1, '~')], "the change came while the cursor was there: still shown");
    feed(&mut a, "l");
    assert!(a.buf.marks.is_empty(), "and the next key counts as having looked");
}

#[test]
fn disk_6_marks_move_with_edits_above_them() {
    let mut a = six();
    feed(&mut a, "ggOnew\x1b");
    assert_eq!(marks(&a), [(2, '~'), (5, '~')], "a line opened above");
    feed(&mut a, "dd");
    assert_eq!(marks(&a), [(1, '~'), (4, '~')], "and deleted again");
    feed(&mut a, "Gdd");
    assert_eq!(
        marks(&a),
        [(1, '~')],
        "the last line deleted: the cursor lands on the marked one above"
    );
    feed(&mut a, "ggJ");
    assert_eq!(text(&a), "a B\nc\nd\nE");
    assert!(a.buf.marks.is_empty(), "a join changes the line it lands in: that mark goes");
    let mut a = six();
    feed(&mut a, "ggyyP");
    assert_eq!(marks(&a), [(2, '~'), (5, '~')], "text put above");
    feed(&mut a, "ggA more\x1b");
    assert_eq!(marks(&a), [(2, '~'), (5, '~')], "an edit that adds no line leaves the others");
}

#[test]
fn disk_6_an_edit_drops_the_marks_of_the_lines_it_changes_and_no_others() {
    // Straight at the buffer: the cursor is not involved.
    let mut a = six();
    let b = &mut a.buf;
    let at = b.start(4) + 1; // inside line 4
    b.replace(at..at, "x");
    assert_eq!(b.marks.iter().map(|m| m.line).collect::<Vec<_>>(), [1]);
    let at = b.start(0);
    b.replace(at..at, "first\nsecond\n"); // two lines above
    assert_eq!(b.marks.iter().map(|m| m.line).collect::<Vec<_>>(), [3]);
    let (s, e) = (b.start(2), b.start(5)); // lines 2, 3 and 4 go: the marked line 3 with them
    b.replace(s..e, "");
    assert!(b.marks.is_empty());
}

#[test]
fn disk_6_seen_clears_all_marks_and_only_marks() {
    let mut a = named("a\nb\nc\n");
    feed(&mut a, "jrX");
    disk(&mut a, "a\nb\nC2\nD\n");
    assert_eq!(a.buf.marks.len(), 2);
    let mut b = six();
    feed(&mut b, ":seen\n");
    assert!(b.buf.marks.is_empty());
    assert_eq!(msg(&mut b), "f  1:1  NORMAL");
    // The conflict with the disk stays: the disk still holds what mine lacks.
    let mut c = named("a\nb\n");
    feed(&mut c, "rX");
    disk(&mut c, "A\nb\nc\n");
    assert_eq!((c.buf.marks.len(), c.diverged()), (2, true));
    feed(&mut c, ":seen\n");
    assert_eq!((c.buf.marks.len(), c.diverged()), (0, true));
}

// ---------------------------------------------------------------------------------------------
// Properties: edits never misplace a mark
// ---------------------------------------------------------------------------------------------

/// A buffer of `n` distinct lines `L0` .. `L{n-1}` with a mark on every line.
fn marked(n: usize) -> Buffer {
    let src: String = (0..n).map(|i| format!("L{i}\n")).collect();
    let mut b = Buffer::decode(src.as_bytes());
    b.marks = (0..n).map(|line| mark(line, '~', vec![0..1])).collect::<Vec<_>>().into();
    b
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10_000))]

    /// DISK-2 / DISK-6. Whatever the edit: marks stay sorted, one per line, inside the text, and
    /// each is on a line with the text it was on (an untouched line); a line the edit did not touch
    /// keeps its mark.
    #[test]
    fn disk_6_edits_never_misplace_a_mark(
        n in 1usize..8,
        from in 0usize..40,
        len in 0usize..30,
        with in prop::sample::select(vec!["", "x", "\n", "x\n", "\nx", "a\nb", "\n\n", "y\nz\n"]),
    ) {
        let mut b = marked(n);
        let before = b.text.to_string();
        let total = b.text.len_chars();
        let (s, e) = (from.min(total), (from + len).min(total));
        b.replace(s..e, with);
        let after = b.text.to_string();
        let ls: Vec<&str> = after.split('\n').collect();
        let mut last = None;
        for m in b.marks.iter() {
            prop_assert!(last.is_none_or(|l| l < m.line) && m.line < b.lines(), "sorted, unique, in range: {:?}", b.marks);
            last = Some(m.line);
            // The line a mark is on is one of the old lines, unchanged: its text is `L<k>` exactly.
            prop_assert!(ls[m.line].starts_with('L') && ls[m.line][1..].parse::<usize>().is_ok(), "{:?} on {:?} after {:?} -> {:?}", m, ls[m.line], before, after);
        }
        // Completeness: an old line that is still a whole line of the text, and that was neither
        // the first nor the last line of the edit, kept its mark.
        let (first, lastl) = (before[..s].matches('\n').count(), before[..e].matches('\n').count());
        for k in 0..n {
            let untouched = k < first || k > lastl;
            let kept = b.marks.iter().any(|m| ls[m.line] == format!("L{k}"));
            prop_assert!(!untouched || kept, "line {k} was not touched by {s}..{e} -> {with:?} in {before:?}: {after:?} {:?}", b.marks);
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(5_000))]

    /// DISK-6 exactly, for edits that replace whole lines `a..b` by `k` new ones: every line that
    /// stays keeps its mark at its new place, and no other line has one.
    #[test]
    fn disk_6_replacing_whole_lines_keeps_exactly_the_marks_of_the_lines_that_stay(
        n in 1usize..8, a in 0usize..8, b in 0usize..8, k in 0usize..3,
    ) {
        let (a, b) = (a.min(n), b.min(n));
        let (a, b) = (a.min(b), a.max(b));
        prop_assume!(!(a == b && k == 0));
        let mut buf = marked(n);
        let len = buf.text.len_chars();
        let news = |sep: &str| (0..k).map(|_| "N").collect::<Vec<_>>().join(sep);
        let (range, with) = if b < n {
            (buf.start(a)..buf.start(b), news("\n") + if k > 0 { "\n" } else { "" })
        } else if a == b {
            (len..len, "\n".to_string() + &news("\n"))
        } else if k == 0 && a > 0 {
            (buf.end(a - 1)..len, String::new())
        } else {
            (buf.start(a)..len, news("\n"))
        };
        buf.replace(range, &with);
        let want: Vec<usize> = (0..n).filter(|&i| i < a || i >= b).map(|i| if i < a { i } else { i - (b - a) + k }).collect();
        let got: Vec<usize> = buf.marks.iter().map(|m| m.line).collect();
        prop_assert_eq!(got, want, "lines {}..{} of {} replaced by {}", a, b, n, k);
    }
}

// ---------------------------------------------------------------------------------------------
// Messages from threads
// ---------------------------------------------------------------------------------------------

#[test]
fn safe_6_a_notice_from_the_journal_thread_reaches_the_status_line() {
    let mut a = named("one\n");
    let fx = a.update(Event::Notice("cannot write snapshots: Permission denied".into()));
    assert_eq!(status(last_frame(&fx)), "cannot write snapshots: Permission denied");
    feed(&mut a, "l");
    assert_eq!(msg(&mut a), "f  1:2  NORMAL", "and the next key clears it, like any message");
}

// ---------------------------------------------------------------------------------------------
// DISK-5: taking in a change on a big file
// ---------------------------------------------------------------------------------------------

/// 100,000 lines; mine edits one line in a thousand, the disk one in 997 (some collide). Run:
/// cargo test --release -- --ignored bench --nocapture
#[test]
#[ignore]
fn disk_5_bench_taking_in_a_change_to_100k_lines_stays_within_the_deadline() {
    release_only();
    const N: usize = 100_000;
    let line = |i: usize| format!("    let value_{i} = compute({i}, \"some text on the line\");\n");
    let side = |step: usize, tag: &str| -> String {
        (0..N)
            .map(|i| match (i % step == 0, (i / step) % 3) {
                (false, _) => line(i),
                (true, 0) => format!("    let {tag}_{i} = changed({i});\n"),
                (true, 1) => String::new(),
                (true, _) => line(i) + &format!("    // added by {tag} at {i}\n"),
            })
            .collect()
    };
    let base: String = (0..N).map(line).collect();
    let (mine, theirs) = (side(1000, "mine"), side(997, "theirs"));
    let rewrite: String = (0..N).map(|i| format!("    rewritten_{i}();\n")).collect();
    let mut worst = Duration::ZERO;
    for (what, disk_text, dirty) in [
        ("clean reload", &theirs, false),
        ("dirty merge", &theirs, true),
        ("rewrite", &rewrite, false),
        ("dirty rewrite", &rewrite, true),
    ] {
        for _ in 0..3 {
            let mut a = app_with(Some("big.rs"), base.as_bytes(), (80, 24));
            if dirty {
                a.buf.replace(0..usize::MAX, &mine);
            }
            let bytes = disk_text.as_bytes().to_vec();
            let start = Instant::now();
            let fx = a.update(Event::Disk {
                bytes: Some(bytes),
                hash: None,
                deadline: start + Duration::from_millis(50),
                seq: 0,
            });
            let took = start.elapsed();
            assert!(last_frame(&fx).rows.len() == 24 && a.buf.lines() > 90_000);
            eprintln!("bench {what}: {took:?} ({} marks)", a.buf.marks.len());
            worst = worst.max(took);
        }
    }
    assert!(worst < Duration::from_millis(50), "taking in a change took {worst:?}");
}

/// Several changes come in one after the other (a formatter that runs every second, `git pull`
/// then a rebase): taking in each must not cost more because of what the earlier ones left behind.
/// Mine changes every line, and so does every round of the disk: the worst input for the diff, which
/// gives up at the deadline (like `hard_inputs` below, the rest must be quick: under 80 ms).
#[test]
#[ignore]
fn disk_5_bench_a_second_change_on_top_of_100k_marks_stays_within_the_deadline() {
    release_only();
    const N: usize = 100_000;
    let rewrite = |tag: &str| -> String {
        (0..N)
            .map(|i| format!("    let {tag}_{i} = compute({i}, \"some text on the line\");\n"))
            .collect()
    };
    let mut a = app_with(Some("big.rs"), rewrite("value").as_bytes(), (80, 24));
    a.buf.replace(0..usize::MAX, &rewrite("mine"));
    let mut worst = Duration::ZERO;
    for round in 0..4 {
        let start = Instant::now();
        let bytes = Some(rewrite(&format!("round{round}")).into_bytes());
        a.update(Event::Disk {
            bytes,
            hash: None,
            deadline: start + Duration::from_millis(50),
            seq: 0,
        });
        let took = start.elapsed();
        eprintln!("round {round}: {took:?}, {} marks", a.buf.marks.len());
        worst = worst.max(took);
    }
    assert!(worst < Duration::from_millis(80), "taking in a change took {worst:?}");
}

// ---------------------------------------------------------------------------------------------
// A random mix of editing and changes on disk
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
enum Step {
    Keys(&'static str),
    Disk(String),
    Gone,
    Conflict(String),
}

fn file_text() -> impl Strategy<Value = String> {
    prop::collection::vec(prop::sample::select(vec!["a", "b", "c", "dd", "", "a b"]), 1..7)
        .prop_map(|ls| ls.iter().map(|l| format!("{l}\n")).collect())
}

fn steps() -> impl Strategy<Value = Vec<Step>> {
    let keys = prop::sample::select(vec![
        "x",
        "dd",
        "ddp",
        "ia\x1b",
        "Ob\x1b",
        "ox\x1b",
        "jx",
        "kx",
        "J",
        "yyP",
        ">>",
        "Gdd",
        "ggdG",
        "cwz\x1b",
        "V",
        "v",
        "j",
        "k",
        "G",
        "gg",
        "Vjd",
        "$",
        ":s/a/A/\n",
        "ggVGy",
        "<C-r>",
    ])
    .prop_map(Step::Keys);
    let step = prop_oneof![
        6 => keys,
        4 => file_text().prop_map(Step::Disk),
        1 => Just(Step::Gone),
        1 => file_text().prop_map(Step::Conflict),
    ];
    prop::collection::vec(step, 1..14)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(3_000))]

    /// Whatever happens in between: no panic, marks stay in range and in order, a line the disk
    /// gained is in the buffer or kept (the buffer is diverged), a merge leaves the buffer clean
    /// exactly when it equals the file, and undoing everything gives the first text back.
    #[test]
    fn disk_2_random_keys_and_changes_on_disk_keep_the_buffer_consistent(base in file_text(), steps in steps()) {
        let mut a = named(&base);
        let first = text(&a);
        let mut known = hash(base.as_bytes());
        let mut seen: std::collections::HashSet<String> = base.lines().map(String::from).collect();
        for step in steps {
            match &step {
                Step::Keys(k) => drop(feed(&mut a, k)),
                Step::Disk(t) | Step::Conflict(t) => {
                    let fresh = hash(t.as_bytes()) != known;
                    if matches!(step, Step::Disk(_)) {
                        disk(&mut a, t);
                    } else {
                        // A half-typed command is cancelled first; `:w!` asks for a save even when the
                        // conflicts of an earlier one are open (the answer below is made up anyway).
                        feed(&mut a, "\x1b\x1b:w!\n");
                        a.update(Event::Kept(Ok("kept".into()))); // (the copy of SAFE-7, if one was asked for)
                        a.update(Event::SaveConflict(Some(t.as_bytes().to_vec()), far(), 0));
                    }
                    known = hash(t.as_bytes());
                    // (Content that App has taken in already is not merged again, even in a save's
                    // answer: that is what "fresh" leaves out.)
                    if fresh {
                        let merged = a.buf.text.to_string() + "\n";
                        prop_assert_eq!(a.buf.dirty(), merged != *t, "clean iff it is the file's text");
                    }
                }
                Step::Gone => {
                    a.update(Event::Disk { bytes: None, hash: None, deadline: far(), seq: 0 });
                    known = 0;
                }
            }
            let b = &a.buf;
            let mut last = None;
            for m in b.marks.iter() {
                prop_assert!(last.is_none_or(|l| l < m.line) && m.line < b.lines(), "{:?}", b.marks);
                last = Some(m.line);
            }
            // A merge never drops a line the disk gained: it is in the buffer, or the buffer is
            // diverged and the disk's version is kept (SAFE-7).
            if let Step::Disk(t) | Step::Conflict(t) = &step {
                let mine = text(&a);
                let have: Vec<&str> = mine.split('\n').collect();
                let new: Vec<&str> = t.lines().filter(|l| !seen.contains(*l)).collect();
                for l in new.iter().filter(|l| !l.is_empty()) {
                    prop_assert!(have.contains(l) || a.diverged() || a.gone, "{l:?} of the disk is neither here nor kept: {mine:?}");
                }
                seen.extend(t.lines().map(String::from));
            }
            prop_assert!(b.cursor <= b.text.len_chars());
            drop(screen(&mut a));
        }
        feed(&mut a, "\x1b");
        for _ in 0..60 {
            feed(&mut a, "u");
        }
        prop_assert_eq!(text(&a), first, "undo takes every step back");
        prop_assert!(a.buf.marks.is_empty());
    }
}

/// The worst case for a diff: every line moved, or the same few lines everywhere. The merge gives
/// up at the deadline and the rest of the work is linear, so the key finishes soon after it.
#[test]
#[ignore]
fn disk_5_bench_hard_inputs_through_update_give_up_at_the_deadline() {
    release_only();
    const N: usize = 100_000;
    let text = |f: &dyn Fn(usize) -> usize| -> String {
        (0..N).map(|i| format!("alpha {}\n", f(i))).collect()
    };
    let (ordered, reversed) = (text(&|i| i), text(&|i| N - 1 - i));
    let (few, other_few) = (text(&|i| i % 50), text(&|i| (i * 7919) % 100));
    for (base, mine, disk_text) in
        [(&ordered, None, &reversed), (&few, None, &ordered), (&few, Some(&other_few), &ordered)]
    {
        let mut a = app_with(Some("big.rs"), base.as_bytes(), (80, 24));
        if let Some(m) = mine {
            a.buf.replace(0..usize::MAX, m);
        }
        let start = Instant::now();
        let bytes = Some(disk_text.as_bytes().to_vec());
        a.update(Event::Disk {
            bytes,
            hash: None,
            deadline: start + Duration::from_millis(50),
            seq: 0,
        });
        let took = start.elapsed();
        eprintln!("bench hard: {took:?} ({} marks)", a.buf.marks.len());
        assert!(took < Duration::from_millis(80), "gave up too late: {took:?}");
    }
}

#[test]
fn disk_2_a_merge_ends_the_visual_selection_instead_of_leaving_it_in_the_old_text() {
    let mut a = named("a\nb\nc\nd\n");
    feed(&mut a, "jjVj");
    disk(&mut a, "a\n");
    assert_eq!(msg(&mut a), "f  1:1  NORMAL  1 change on disk");
    assert_eq!(text(&a), "a");
}
