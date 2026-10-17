//! Races between the shell and the App, and the other ways a late or unexpected event can bite:
//! a watcher read that cannot be ordered against a save's answer (DISK-1), a merge across undo
//! and across a Visual selection (DISK-2), `Opened` answers (VI-7), the snapshot of a file that is
//! gone (SAFE-6), and the marks of joined lines (DISK-6). Each test reproduces one defect found by
//! review; the random versions are tests/model.rs and tests/shift.rs.
#![allow(clippy::unwrap_used, clippy::single_range_in_vec_init)]
mod common;
use common::*;
use sim_core::core::app::{App, Effect, Event};
use sim_core::core::buffer::{ABSENT, Buffer, hash};

fn named(text: &str) -> App {
    app_with(Some("f"), text.as_bytes(), (100, 12))
}

fn write(fx: &[Effect]) -> Option<(Vec<u8>, Option<u64>)> {
    fx.iter().find_map(|e| match e {
        Effect::Write { bytes, expect_hash, .. } => Some((bytes.clone(), *expect_hash)),
        _ => None,
    })
}

fn rewatch(fx: &[Effect]) -> bool {
    fx.iter().any(|e| matches!(e, Effect::Watch(_)))
}

fn quits(fx: &[Effect]) -> bool {
    fx.iter().any(|e| matches!(e, Effect::Quit(_)))
}

fn disk(a: &mut App, text: &str) {
    a.update(Event::Disk {
        bytes: Some(text.as_bytes().to_vec()),
        hash: None,
        deadline: far(),
        seq: 0,
    });
}

// ---------------------------------------------------------------------------------------------
// DISK-1: what the watcher says while a save is in flight cannot be ordered against the save
// ---------------------------------------------------------------------------------------------

/// The file is deleted right after our rename. The watcher's read (`None`) reaches the App before
/// the save thread's answer, and the answer then says the file is saved. The App asks for a fresh
/// read, which says `None` again: the text is in the buffer only, and `:q` must not lose it.
#[test]
fn disk_1_a_deletion_seen_during_a_save_is_not_forgotten_by_the_answer() {
    let mut a = named("one\ntwo\n");
    feed(&mut a, "x");
    let (bytes, _) = write(&feed(&mut a, ":w\n")).unwrap();
    a.update(Event::Disk { bytes: None, hash: None, deadline: far(), seq: 0 }); // read after the rename
    let fx = a.update(Event::Saved(Ok((hash(&bytes), None)), 0));
    assert!(rewatch(&fx), "the answer of a save is followed by a fresh read of the file");
    a.update(Event::Disk { bytes: None, hash: None, deadline: far(), seq: 0 }); // what the fresh read says
    assert!(a.gone && a.buf.dirty());
    assert!(!quits(&feed(&mut a, ":q\n")), "quit with the text only in the buffer");
}

/// Somebody reverts the file to the content we knew before the save. The event hashes like the
/// known disk content and is dropped; the answer of the save then replaces what we know.
#[test]
fn disk_1_a_revert_seen_during_a_save_is_not_swallowed_by_the_hash_check() {
    let mut a = named("one\ntwo\n");
    feed(&mut a, "x");
    let (bytes, _) = write(&feed(&mut a, ":w\n")).unwrap();
    disk(&mut a, "one\ntwo\n"); // the old content comes back
    let fx = a.update(Event::Saved(Ok((hash(&bytes), None)), 0));
    assert!(
        rewatch(&fx),
        "clean buffer \"{}\" but the last read of the file was the old text",
        text(&a)
    );
    // The fresh read says so, and the buffer takes it in like any change.
    disk(&mut a, "one\ntwo\n");
    assert_eq!(text(&a), "one\ntwo");
    assert!(!a.buf.dirty());
}

#[test]
fn disk_1_every_answer_of_a_save_is_followed_by_a_fresh_read() {
    let mut a = named("one\n");
    feed(&mut a, "x");
    write(&feed(&mut a, ":w\n")).unwrap();
    let fx = a.update(Event::SaveConflict(Some(b"one\nmore\n".to_vec()), far(), 0));
    assert!(rewatch(&fx), "a conflict is followed by a read too");
    // A first save of an unnamed buffer starts watching once, and reads once.
    let mut a = app("");
    feed(&mut a, "ihi\x1b:w new.txt\n");
    let fx = a.update(Event::Saved(Ok((hash(b"hi\n"), None)), 0));
    assert_eq!(fx.iter().filter(|e| matches!(e, Effect::Watch(p) if p == "new.txt")).count(), 1);
}

// ---------------------------------------------------------------------------------------------
// DISK-1: every read of the disk is numbered as it starts; a read older than what App took in is
// dropped
// ---------------------------------------------------------------------------------------------

fn disk_n(a: &mut App, text: &str, seq: u64) {
    let bytes = Some(text.as_bytes().to_vec());
    a.update(Event::Disk { bytes, hash: None, deadline: far(), seq });
}

/// The race of the model's replay: a watcher read taken before an `:e!` loaded the newer file
/// arrives after the load. Without numbers it was taken for a change and put the old text back.
#[test]
fn disk_1_a_read_older_than_a_load_is_dropped() {
    let mut a = named("one\n");
    assert!(feed(&mut a, ":e!\n").iter().any(|e| matches!(e, Effect::Open(_))));
    a.update(Event::Opened("f".into(), Ok(Some(b"two\n".to_vec())), 5));
    disk_n(&mut a, "one\nold\n", 3); // taken before the load, sent after it
    assert_eq!((text(&a), a.buf.marks.len()), ("two".into(), 0));
    disk_n(&mut a, "two\nnew\n", 6); // a read that started after the load is news
    assert_eq!((text(&a), a.buf.marks.len()), ("two\nnew".into(), 1));
}

#[test]
fn disk_1_a_read_older_than_a_saves_answer_is_dropped() {
    let mut a = named("one\n");
    feed(&mut a, "x");
    let (bytes, _) = write(&feed(&mut a, ":w\n")).unwrap();
    a.update(Event::Saved(Ok((hash(&bytes), None)), 10));
    disk_n(&mut a, "one\n", 7); // read before the rename: the file as it was
    assert_eq!((text(&a), a.buf.dirty()), ("ne".into(), false), "{:?}", a.msg);
    disk_n(&mut a, "ne\nmore\n", 11); // read after it
    assert_eq!(text(&a), "ne\nmore");
}

#[test]
fn disk_1_a_read_older_than_a_newer_read_is_dropped_and_an_equal_one_is_not() {
    let mut a = named("one\n");
    disk_n(&mut a, "one\ntwo\n", 9);
    disk_n(&mut a, "one\nTWO\n", 8);
    assert_eq!(text(&a), "one\ntwo", "the older read is not applied after the newer one");
    // The shell never gives one number twice; tests that do not care pass 0 for everything.
    disk_n(&mut a, "one\nTWO\n", 9);
    assert_eq!(text(&a), "one\nTWO");
}

/// A save's answer is never dropped: the save happened. But what was read before it is older.
#[test]
fn disk_1_a_save_answer_is_taken_in_even_when_a_newer_read_came_first() {
    let mut a = named("one\ntwo\n");
    feed(&mut a, "x");
    let (bytes, _) = write(&feed(&mut a, ":w\n")).unwrap();
    disk_n(&mut a, "one\ntwo\nout\n", 12); // an outside append, read after the rename
    assert_eq!(text(&a), "ne\ntwo\nout");
    let fx = a.update(Event::Saved(Ok((hash(&bytes), None)), 10));
    assert!(rewatch(&fx) && a.msg.as_deref() == Some("saved f"), "{:?}", a.msg);
    // What was read before the answer is older, whoever sent it last.
    disk_n(&mut a, "ne\ntwo\n", 11);
    assert_eq!(text(&a), "ne\ntwo\nout");
}

#[test]
fn disk_1_a_conflict_older_than_a_read_taken_in_merges_nothing() {
    let mut a = named("one\ntwo\n");
    feed(&mut a, "x");
    write(&feed(&mut a, ":w\n")).unwrap();
    disk_n(&mut a, "one\ntwo\nthree\n", 20); // already merged, the buffer is the newer file plus mine
    let merged = text(&a);
    let fx = a.update(Event::SaveConflict(Some(b"one\nOLDER\n".to_vec()), far(), 15));
    assert_eq!(text(&a), merged, "the older content of the compare is not merged over the newer");
    assert!(a.msg.as_deref().is_some_and(|m| m.contains("changed on disk meanwhile")));
    assert!(rewatch(&fx), "a fresh read follows the answer all the same");
    // The save is over: :w can be asked again.
    assert!(write(&feed(&mut a, ":w\n")).is_some());
}

#[test]
fn disk_1_a_load_older_than_a_newer_read_of_the_same_file_is_refused_and_another_file_loads() {
    let mut a = named("one\n");
    feed(&mut a, ":e!\n");
    // The file was read for the load, then written back to what the buffer has, then read again:
    // that newer read changes nothing, but the load would bring the older text back.
    disk_n(&mut a, "one\n", 9);
    a.update(Event::Opened("f".into(), Ok(Some(b"older\n".to_vec())), 8));
    assert_eq!(text(&a), "one");
    assert_eq!(a.msg.as_deref(), Some("not opened: the file changed meanwhile (:e! again)"));
    feed(&mut a, ":e!\n"); // asking again works: the next read is newer
    let fx = a.update(Event::Opened("f".into(), Ok(Some(b"third\n".to_vec())), 10));
    assert!(rewatch(&fx) && text(&a) == "third");
    // A load of another file is not ordered against reads of this one.
    feed(&mut a, ":e g\n");
    disk_n(&mut a, "third\n", 14);
    a.update(Event::Opened("g".into(), Ok(Some(b"g\n".to_vec())), 13));
    assert_eq!((a.path.as_deref(), text(&a)), (Some("g"), "g".into()));
}

// ---------------------------------------------------------------------------------------------
// DISK-2: a merge, undo and the Visual selection
// ---------------------------------------------------------------------------------------------

#[test]
fn disk_2_undo_across_a_dirty_merge_does_not_make_the_buffer_clean() {
    let mut a = named("b0\n");
    feed(&mut a, "A m1\x1b");
    disk(&mut a, "e2\nb0\n");
    assert_eq!(text(&a), "e2\nb0 m1");
    feed(&mut a, "uu");
    assert_eq!(text(&a), "b0", "everything undone: the text before both");
    assert!(a.buf.dirty(), "the file holds \"e2\\nb0\" now; \"b0\" is not the file's text");
    assert!(!quits(&feed(&mut a, ":q\n")));
    // Redo goes forward again, and the buffer is still modified.
    feed(&mut a, "<C-r><C-r>");
    assert_eq!(text(&a), "e2\nb0 m1");
    assert!(a.buf.dirty());
}

/// The file changes only in its format (it loses its final newline). The merge takes the format
/// in, and undo goes back to the text that was saved: but that text is not what the file holds
/// any more, in the format a save would write, so the buffer is not clean.
#[test]
fn disk_2_undo_across_a_merge_that_changed_only_the_format_does_not_make_the_buffer_clean() {
    let mut a = named("b0\n");
    feed(&mut a, "yyp");
    a.update(Event::Disk { bytes: Some(b"b0".to_vec()), hash: None, deadline: far(), seq: 0 });
    assert_eq!(text(&a), "b0\nb0");
    feed(&mut a, "uu");
    assert_eq!(text(&a), "b0");
    assert!(
        a.buf.dirty(),
        "clean, but a save would write {:?} over the file \"b0\"",
        a.buf.encode()
    );
}

/// A save is running with the format the file had (no BOM). Meanwhile a change with a BOM is
/// merged (format adopted: BOM). The file now holds what the save wrote, without BOM, so the
/// buffer must not write a BOM from then on.
#[test]
fn disk_2_the_format_of_a_finished_save_is_the_format_of_the_file() {
    let mut a = named("b0\n");
    feed(&mut a, "A x\x1b");
    let (bytes, _) = write(&feed(&mut a, ":w\n")).unwrap();
    a.update(Event::Disk {
        bytes: Some("\u{feff}b0 y\n".as_bytes().to_vec()),
        hash: None,
        deadline: far(),
        seq: 0,
    });
    a.update(Event::Saved(Ok((hash(&bytes), None)), 0));
    let bom = |b: &[u8]| b.starts_with(b"\xEF\xBB\xBF");
    assert_eq!(
        bom(&a.buf.encode()),
        bom(&bytes),
        "the file has no BOM, the buffer would write one"
    );
    // The same for CRLF and the final newline.
    let mut a = app_with(Some("f"), b"a\nb", (80, 12));
    feed(&mut a, "x");
    let (bytes, _) = write(&feed(&mut a, ":w\n")).unwrap();
    assert_eq!(bytes, b"\nb");
    a.update(Event::Disk {
        bytes: Some(b"a\r\nb\r\nc\r\n".to_vec()),
        hash: None,
        deadline: far(),
        seq: 0,
    });
    a.update(Event::Saved(Ok((hash(&bytes), None)), 0));
    assert_eq!(Buffer::decode(&bytes).encode(), bytes);
    let after = a.buf.encode();
    assert!(!after.contains(&b'\r') && !after.ends_with(b"\n"), "{after:?}");
}

/// The text is deleted while a save is running: the file ends as it was written, but the buffer
/// is empty, and saving that writes an empty file, not a line break (SAFE-3).
#[test]
fn disk_2_a_finished_save_leaves_an_emptied_buffer_empty() {
    let mut a = named("abc\n");
    feed(&mut a, "x");
    let (bytes, _) = write(&feed(&mut a, ":w\n")).unwrap();
    feed(&mut a, "ggdG");
    assert_eq!(text(&a), "");
    a.update(Event::Saved(Ok((hash(&bytes), None)), 0));
    assert!(a.buf.dirty());
    assert_eq!(write(&feed(&mut a, ":w\n")).unwrap().0, b"");
}

/// The selection ends when a merge comes in (DISK-2) and its lines are kept for `:'<,'>`. They are
/// lines of the new text: after three lines come in above, `:'<,'>s/./X/` changes the lines that
/// were selected, not the ones that are now at those numbers.
#[test]
fn disk_2_the_lines_of_the_last_selection_follow_a_merge() {
    let mut a = named("a1\nb2\nc3\nd4\ne5\nf6\n");
    feed(&mut a, "jVj:"); // b2 and c3 are selected; the command line starts with `:'<,'>`
    disk(&mut a, "x\ny\nz\na1\nb2\nc3\nd4\ne5\nf6\n");
    feed(&mut a, "s/./X/\n");
    assert_eq!(text(&a), "x\ny\nz\na1\nX2\nX3\nd4\ne5\nf6", "only the selected lines");
    // The same after the selection was left by Esc.
    let mut a = named("a1\nb2\nc3\nd4\n");
    feed(&mut a, "jVj\x1b");
    disk(&mut a, "x\na1\nb2\nc3\nd4\n");
    feed(&mut a, ":'<,'>s/./X/\n");
    assert_eq!(text(&a), "x\na1\nX2\nX3\nd4");
}

// ---------------------------------------------------------------------------------------------
// DISK-6: marks
// ---------------------------------------------------------------------------------------------

/// `J` on an empty line followed by an empty line: the edit reads as "the first line stays" and as
/// "the second line stays" at once, and one line is left. DISK-6 has one mark per line, and the
/// mark of the cursor's line is gone after a key.
#[test]
fn disk_6_joining_two_empty_lines_leaves_one_mark_not_two() {
    let mut a = named("a\n\n\nb\n");
    a.buf.marks = (1..3).map(|line| mark(line, '+', vec![0..0])).collect::<Vec<_>>().into();
    a.buf.cursor = a.buf.start(1);
    feed(&mut a, "J");
    assert_eq!(text(&a), "a\n\nb");
    let lines: Vec<usize> = a.buf.marks.iter().map(|m| m.line).collect();
    assert!(lines.is_empty(), "the cursor is on line 1, so its mark is gone: {lines:?}");
    // Without the cursor there, the mark that is left is one, and sorted.
    let mut b = Buffer::decode(b"a\n\n\nb\n");
    b.marks = (1..3).map(|line| mark(line, '+', vec![0..0])).collect::<Vec<_>>().into();
    b.replace(b.start(1)..b.start(2), "");
    let lines: Vec<usize> = b.marks.iter().map(|m| m.line).collect();
    assert_eq!(lines, [1], "{lines:?}");
}

/// DISK-6 Decided: undo brings back the marks as they were before the undone step, including
/// ones the cursor had already entered. Showing a change again errs on the safe side.
#[test]
fn disk_6_undo_brings_back_a_mark_that_was_seen() {
    let mut a = named("a\nb\nc\nd\n");
    disk(&mut a, "a\nb\nC\nd\n");
    assert_eq!(a.buf.marks.len(), 1);
    feed(&mut a, "x"); // an undo snapshot that holds the mark of line 2
    feed(&mut a, "jj"); // the cursor enters line 2: the mark is gone
    assert!(a.buf.marks.is_empty());
    feed(&mut a, "u");
    assert_eq!(a.buf.marks.len(), 1, "undo shows the change again");
}

// ---------------------------------------------------------------------------------------------
// VI-7: `Opened` answers and `.`
// ---------------------------------------------------------------------------------------------

/// `:e!` throws my edit away, and `:w!` right behind it (a script, `send-keys`) must not save the
/// edit that is being thrown away while the new buffer has not arrived yet.
#[test]
fn vi_7_a_write_right_behind_a_reload_is_refused() {
    let mut a = named("one\ntwo\n");
    feed(&mut a, "dd");
    let mut fx = feed(&mut a, ":e!\n");
    assert!(fx.iter().any(|e| matches!(e, Effect::Open(_))));
    fx = feed(&mut a, ":w!\n");
    assert!(write(&fx).is_none(), "`:w!` after `:e!` wrote the edit that `:e!` discards");
    assert_eq!(status(&screen(&mut a)), "a reload is already running");
    fx = feed(&mut a, "<C-s>:w\n");
    assert!(write(&fx).is_none());
    // When the file has arrived, writing works again.
    a.update(Event::Opened("f".into(), Ok(Some(b"one\ntwo\n".to_vec())), 0));
    assert!(write(&feed(&mut a, ":w!\n")).is_some());
}

/// `:w` and, while it is running, `:e!`: refused, so the new buffer can never take the answer of a
/// save it knows nothing about.
#[test]
fn vi_7_a_reload_during_a_save_is_refused() {
    let mut a = named("one\ntwo\n");
    feed(&mut a, "x");
    write(&feed(&mut a, ":w\n")).unwrap();
    let fx = feed(&mut a, ":e!\n");
    assert!(!fx.iter().any(|e| matches!(e, Effect::Open(_))));
    assert_eq!(status(&screen(&mut a)), "a save is already running");
}

/// `Opened` loads the file whatever the buffer holds when no `:e` is outstanding.
#[test]
fn vi_7_an_opened_nobody_asked_for_does_not_replace_the_buffer() {
    let mut a = named("one\n");
    feed(&mut a, "x");
    a.update(Event::Opened("f".into(), Ok(Some(b"zzz\n".to_vec())), 0));
    assert_eq!(text(&a), "ne", "my edit was thrown away by a file nobody asked for");
}

#[test]
fn vi_7_two_reloads_typed_together_do_not_throw_away_the_edit_made_after_them() {
    let mut a = named("one\ntwo\n");
    // Both commands are processed before either answer (the shell runs effects after the keys).
    feed(&mut a, ":e!\n:e!\n");
    feed(&mut a, "x");
    a.update(Event::Opened("f".into(), Ok(Some(b"one\ntwo\n".to_vec())), 0));
    a.update(Event::Opened("f".into(), Ok(Some(b"one\ntwo\n".to_vec())), 0));
    assert_eq!(text(&a), "ne\ntwo", "the edit made after `:e!` is kept, both answers refused");
}

/// Two `:e` typed together are answered one after the other: the first loads, and the second one
/// still counts if the new buffer is as it was loaded. Versions start again at 0 in a new buffer,
/// so the second must not be compared with the numbers of the old one.
#[test]
fn vi_7_an_answer_after_a_load_is_for_the_buffer_that_was_loaded() {
    let mut a = named("one\ntwo\n");
    feed(&mut a, "xx:e! g\n:e! h\n"); // both remember a version of 2 changes
    a.update(Event::Opened("g".into(), Ok(Some(b"g\n".to_vec())), 0));
    a.update(Event::Opened("h".into(), Ok(Some(b"h\n".to_vec())), 0));
    assert_eq!((a.path.as_deref(), text(&a).as_str()), (Some("h"), "h"), "in order, the last wins");
    // The new buffer is edited before the second answer: that edit is kept.
    let mut a = named("one\ntwo\n");
    feed(&mut a, "xx:e! g\n:e! h\n");
    a.update(Event::Opened("g".into(), Ok(Some(b"gg\n".to_vec())), 0));
    feed(&mut a, "xx"); // the new buffer reaches the same version of 2 changes
    a.update(Event::Opened("h".into(), Ok(Some(b"h\n".to_vec())), 0));
    assert_eq!((a.path.as_deref(), text(&a).as_str()), (Some("g"), ""));
    // And one nobody asked for is ignored.
    a.update(Event::Opened("h".into(), Ok(Some(b"h\n".to_vec())), 0));
    assert_eq!(a.path.as_deref(), Some("g"));
}

// ---------------------------------------------------------------------------------------------
// SAFE-6: recovery
// ---------------------------------------------------------------------------------------------

/// The file was deleted from outside while si held its text (DISK-1: the text stays, the buffer
/// counts as modified, so it is snapshotted). si dies; the file is still gone. Opening it again
/// offers the snapshot, whose text equals its base: a merge would take the missing file for a
/// deletion of every line, apply it, make the buffer clean, and `Unsnapshot` would delete the only
/// copy of the text.
#[test]
fn safe_6_the_snapshot_of_a_deleted_file_is_restored_whole_and_modified() {
    let mut a = App::new(Some("f".into()), Ok(None), (80, 12));
    let fx = a.update(Event::Recover(
        b"one\ntwo".to_vec(),
        b"one\ntwo".to_vec(),
        5000,
        "snap".into(),
        far(),
    ));
    assert!(
        !fx.iter().any(|e| matches!(e, Effect::Unsnapshot)),
        "it held the only copy of the text"
    );
    assert_eq!(text(&a), "one\ntwo");
    assert!(a.buf.dirty() && a.gone, "the text is not on disk");
    feed(&mut a, "\x1b"); // the message of the recovery goes with the first key
    assert!(status(&screen(&mut a)).contains("modified · deleted on disk"));
    // `:w` makes the file again.
    let fx = feed(&mut a, ":w\n");
    assert_eq!(write(&fx).unwrap(), (b"one\ntwo\n".to_vec(), Some(ABSENT)));
    // An empty snapshot of a missing file counts as modified too.
    let mut a = App::new(Some("f".into()), Ok(None), (80, 12));
    a.update(Event::Recover(b"one\n".to_vec(), b"".to_vec(), 5000, "snap".into(), far()));
    assert!(a.buf.dirty(), "nothing on disk, so nothing is saved");
    // Undo takes the recovery back, and the buffer is still not the (missing) file.
    feed(&mut a, "u");
    assert!(a.buf.dirty());
}
