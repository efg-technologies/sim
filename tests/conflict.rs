//! SAFE-5, SAFE-7, DISK-2 on App: a save that finds the file changed on disk writes nothing, and
//! when what the disk holds conflicts with mine both versions are kept: the two explicit ways out
//! (`:w!`, `:e!`) copy the version they give up first, and a crash between the copy and the write
//! loses nothing.
#![allow(clippy::unwrap_used, clippy::single_range_in_vec_init)]
mod common;
use common::*;
use si::core::app::{App, Effect, Event, Key};
use si::core::buffer::{ABSENT, hash};
use si::core::vi::Mode;

const BASE: &str = "one\ntwo\nthree\n";
/// What the disk comes to hold: the line mine changes (`jrX`) is changed by the other side too.
const THEIRS: &str = "one\nTWO\nthree\n";
/// Mine: the same line changed differently.
const MINE: &str = "one\nXwo\nthree";

fn named(text: &str) -> App {
    app_with(Some("f"), text.as_bytes(), (130, 12))
}

fn disk(a: &mut App, text: &str) -> Vec<Effect> {
    a.update(Event::Disk {
        bytes: Some(text.as_bytes().to_vec()),
        hash: None,
        deadline: far(),
        seq: 0,
    })
}

/// A buffer that has changed line 2 while the disk has changed it another way: diverged.
fn diverged() -> App {
    let mut a = named(BASE);
    feed(&mut a, "jrX");
    disk(&mut a, THEIRS);
    assert!(a.diverged() && text(&a) == MINE);
    a
}

fn write(fx: &[Effect]) -> Option<(&[u8], Option<u64>)> {
    fx.iter().find_map(|e| match e {
        Effect::Write { bytes, expect_hash, .. } => Some((bytes.as_slice(), *expect_hash)),
        _ => None,
    })
}

/// What a `Keep` asks for: the file it is named after, the bytes, and the ending of the copy's name.
fn kept(fx: &[Effect]) -> Option<(&str, &[u8], &str)> {
    fx.iter().find_map(|e| match e {
        Effect::Keep { path, bytes, ext } => Some((path.as_str(), bytes.as_slice(), *ext)),
        _ => None,
    })
}

fn opens(fx: &[Effect]) -> bool {
    fx.iter().any(|e| matches!(e, Effect::Open(_)))
}

fn quits(fx: &[Effect]) -> bool {
    fx.iter().any(|e| matches!(e, Effect::Quit(_)))
}

fn msg(a: &mut App) -> String {
    status(&screen(a)).to_string()
}

/// (line, symbol) of every mark.
fn marks(a: &App) -> Vec<(usize, char)> {
    a.buf.marks.iter().map(|m| (m.line, m.kind)).collect()
}

const OUT: &str = ":w! keeps yours · :e! takes the disk's · the other is copied first";

#[test]
fn safe_5_a_save_that_finds_a_conflict_writes_nothing_names_both_ways_out_and_keeps_mine() {
    let mut a = named(BASE);
    feed(&mut a, "jrX");
    let fx = feed(&mut a, ":w\n");
    assert!(write(&fx).is_some());
    let fx = a.update(Event::SaveConflict(Some(THEIRS.as_bytes().to_vec()), far(), 0));
    assert!(write(&fx).is_none() && !quits(&fx) && kept(&fx).is_none());
    assert_eq!(text(&a), MINE, "mine stays whole in the buffer");
    assert_eq!(marks(&a), [(1, '~')], "where the disk differs is marked");
    assert!(a.diverged() && a.buf.dirty());
    assert_eq!(
        status(last_frame(&fx)),
        format!("not saved: the disk changed and conflicts · {OUT}")
    );
}

#[test]
fn safe_5_what_the_merge_can_take_in_is_taken_in_and_only_the_conflict_is_left() {
    let mut a = named("a\nb\nc\nd\n");
    feed(&mut a, "rX"); // mine: line 1
    let fx = feed(&mut a, ":w\n");
    assert!(write(&fx).is_some());
    // Theirs: the same line changed another way, and line 4 changed alone.
    let fx = a.update(Event::SaveConflict(Some(b"A\nb\nc\nD\n".to_vec()), far(), 0));
    assert_eq!(text(&a), "X\nb\nc\nD");
    assert_eq!(marks(&a), [(0, '~'), (3, '~')]);
    assert!(a.diverged());
    assert!(status(last_frame(&fx)).starts_with("not saved: the disk changed and conflicts"));
}

#[test]
fn safe_5_a_save_that_finds_no_conflict_merges_and_the_next_save_goes_through() {
    let mut a = named(BASE);
    feed(&mut a, "rO");
    feed(&mut a, ":w\n");
    let fx = a.update(Event::SaveConflict(Some(b"one\ntwo\nTHREE\n".to_vec()), far(), 0));
    assert_eq!(
        status(last_frame(&fx)),
        "not saved: the file changed on disk; merged its changes (:w to save)"
    );
    assert!(!a.diverged(), "nothing is left on the disk that the buffer lacks");
    let fx = feed(&mut a, ":w\n");
    assert!(write(&fx).is_some());
}

#[test]
fn safe_7_every_way_to_save_is_refused_while_diverged_and_none_writes() {
    for keys in [":w\n", ":wq\n", ":x\n", "ZZ", "<C-s>"] {
        let mut a = diverged();
        let fx = feed(&mut a, keys);
        assert!(write(&fx).is_none() && kept(&fx).is_none() && !quits(&fx), "{keys:?}");
        assert_eq!(msg(&mut a), format!("not saved: the disk conflicts · {OUT}"), "{keys:?}");
        assert_eq!(text(&a), MINE);
    }
    // In Insert mode Ctrl-s is refused and Insert mode stays.
    let mut a = diverged();
    feed(&mut a, "i");
    let fx = a.update(Event::Key(Key::Ctrl('s')));
    assert!(write(&fx).is_none() && a.vi.mode == Mode::Insert);
}

#[test]
fn safe_7_saving_under_another_name_is_allowed_it_is_how_to_keep_mine() {
    let mut a = diverged();
    let fx = feed(&mut a, ":w mine.txt\n");
    let (bytes, expect) = write(&fx).unwrap();
    assert_eq!((bytes, expect), (&b"one\nXwo\nthree\n"[..], Some(ABSENT)));
}

#[test]
fn safe_7_quitting_is_refused_as_vi_refuses_it_and_q_bang_discards() {
    let mut a = diverged();
    let fx = feed(&mut a, ":q\n");
    assert!(!quits(&fx));
    assert_eq!(msg(&mut a), "no write since last change (:q! to override)");
    assert!(quits(&feed(&mut a, ":q!\n")));
    assert!(quits(&feed(&mut diverged(), "ZQ")));
}

#[test]
fn safe_7_w_bang_copies_the_disks_version_first_and_writes_only_after_the_copy() {
    let mut a = diverged();
    let fx = feed(&mut a, ":w!\n");
    assert_eq!(kept(&fx), Some(("f", THEIRS.as_bytes(), "disk")), "the disk's version is copied");
    assert!(write(&fx).is_none(), "nothing is written before the copy is done");
    assert!(a.diverged() && a.buf.dirty());
    let fx = a.update(Event::Kept(Ok("/state/f.1.disk".into())));
    let (bytes, expect) = write(&fx).unwrap();
    assert_eq!((bytes, expect), (&b"one\nXwo\nthree\n"[..], None), "mine goes over it, forced");
    assert!(a.diverged(), "the conflict ends when the save is done");
    let fx = a.update(Event::Saved(Ok((hash(bytes), None)), 0));
    assert!(!a.diverged() && !a.buf.dirty());
    assert_eq!(status(last_frame(&fx)), "saved f; the disk's version is in /state/f.1.disk");
}

#[test]
fn safe_7_a_crash_between_the_copy_and_the_write_loses_nothing() {
    let mut a = diverged();
    let fx = feed(&mut a, ":w!\n");
    // The copy is asked for and the write has not started: the disk still holds the disk's version
    // (the core writes nothing itself), the copy is made by the shell, and mine is in the snapshot
    // that the next tick asks for. A process that dies here has lost neither.
    let theirs = kept(&fx).unwrap().1.to_vec();
    assert_eq!(theirs, THEIRS.as_bytes());
    assert!(write(&fx).is_none());
    let fx = a.update(Event::Tick(5000));
    let snap = fx.iter().find_map(|e| match e {
        Effect::Snapshot { text, .. } => Some(text.to_string()),
        _ => None,
    });
    assert_eq!(snap.as_deref(), Some(MINE), "mine is in the journal");
}

#[test]
fn safe_7_a_copy_that_cannot_be_made_writes_nothing_and_ends_nothing() {
    let mut a = diverged();
    feed(&mut a, ":w!\n");
    let fx = a.update(Event::Kept(Err("no space left on device".into())));
    assert!(write(&fx).is_none());
    assert_eq!(
        status(last_frame(&fx)),
        "not saved: cannot keep the disk's version: no space left on device"
    );
    assert!(a.diverged() && a.buf.dirty() && text(&a) == MINE);
    // It can be tried again.
    let fx = feed(&mut a, ":w!\n");
    assert!(kept(&fx).is_some());
}

#[test]
fn safe_7_a_save_is_not_started_twice_while_the_copy_is_made() {
    let mut a = diverged();
    feed(&mut a, ":w!\n");
    let fx = feed(&mut a, ":w!\n");
    assert!(kept(&fx).is_none() && write(&fx).is_none());
    assert_eq!(msg(&mut a), "a save is already running");
}

#[test]
fn safe_7_w_bang_with_no_conflict_copies_nothing_as_before() {
    let mut a = named(BASE);
    feed(&mut a, "x");
    let fx = feed(&mut a, ":w!\n");
    assert!(kept(&fx).is_none());
    assert_eq!(write(&fx).unwrap().1, None);
}

#[test]
fn safe_7_e_bang_copies_mine_first_and_reloads_only_after_the_copy() {
    let mut a = diverged();
    let fx = feed(&mut a, ":e!\n");
    assert_eq!(kept(&fx), Some(("f", &b"one\nXwo\nthree\n"[..], "yours")));
    assert!(!opens(&fx), "nothing is reloaded before the copy is done");
    assert_eq!(text(&a), MINE);
    let fx = a.update(Event::Kept(Ok("/state/f.2.yours".into())));
    assert!(opens(&fx));
    let fx = a.update(Event::Opened("f".into(), Ok(Some(THEIRS.as_bytes().to_vec())), 9));
    assert_eq!(text(&a), "one\nTWO\nthree");
    assert!(!a.diverged() && !a.buf.dirty());
    assert_eq!(status(last_frame(&fx)), "loaded f; yours is in /state/f.2.yours");
}

#[test]
fn safe_7_a_copy_of_mine_that_cannot_be_made_leaves_the_buffer_as_it_was() {
    let mut a = diverged();
    feed(&mut a, ":e!\n");
    let fx = a.update(Event::Kept(Err("no state directory".into())));
    assert!(!opens(&fx));
    assert_eq!(status(last_frame(&fx)), "not reloaded: cannot keep yours: no state directory");
    assert!(a.diverged() && a.buf.dirty() && text(&a) == MINE);
}

#[test]
fn safe_7_an_edit_made_while_mine_is_copied_is_not_thrown_away_by_the_reload() {
    let mut a = diverged();
    feed(&mut a, ":e!\n");
    feed(&mut a, "x"); // typed after the copy was asked for: the copy does not hold it
    let fx = a.update(Event::Kept(Ok("/state/f.2.yours".into())));
    assert!(!opens(&fx));
    assert_eq!(msg(&mut a), "not reloaded: edited meanwhile; yours is in /state/f.2.yours");
}

#[test]
fn safe_7_e_bang_without_a_conflict_discards_the_changes_as_vi_does() {
    let mut a = named(BASE);
    feed(&mut a, "x");
    let fx = feed(&mut a, ":e!\n");
    assert!(kept(&fx).is_none() && opens(&fx));
}

#[test]
fn safe_7_the_conflict_outlives_an_undo_of_the_merge_and_a_newer_read_replaces_what_is_kept() {
    let mut a = diverged();
    feed(&mut a, "u"); // the merge was one undo step (there is nothing of theirs in it to take back)
    assert!(a.diverged(), "the disk still holds what it held");
    let fx = feed(&mut a, ":w\n");
    assert!(write(&fx).is_none());
    // The other side writes again: the version kept is the newest.
    disk(&mut a, "one\nTWO\nthree\nfour\n");
    let fx = feed(&mut a, ":w!\n");
    assert_eq!(kept(&fx).unwrap().1, b"one\nTWO\nthree\nfour\n");
}

#[test]
fn safe_7_a_file_deleted_after_a_conflict_is_still_kept() {
    let mut a = diverged();
    a.update(Event::Disk { bytes: None, hash: None, deadline: far(), seq: 1 });
    assert!(a.gone && a.diverged());
    let fx = feed(&mut a, ":w\n");
    assert!(
        write(&fx).is_none(),
        "even a file that is gone has a version that nobody has saved over"
    );
    let fx = feed(&mut a, ":w!\n");
    assert_eq!(kept(&fx).unwrap().1, THEIRS.as_bytes());
}

#[test]
fn safe_7_a_file_that_is_no_longer_text_is_kept_as_it_is_before_it_is_overwritten() {
    let mut a = named(BASE);
    feed(&mut a, "jrX");
    let bin = b"\x00\x01\x02 not text\n".to_vec();
    a.update(Event::Disk { bytes: Some(bin.clone()), hash: None, deadline: far(), seq: 0 });
    assert!(a.diverged() && text(&a) == MINE);
    let fx = feed(&mut a, ":w!\n");
    assert_eq!(kept(&fx).unwrap().1, bin.as_slice());
}

#[test]
fn disk_2_a_read_that_makes_the_buffer_equal_to_the_file_ends_the_conflict() {
    let mut a = diverged();
    disk(&mut a, "one\nXwo\nthree\n");
    assert!(!a.diverged());
    let fx = feed(&mut a, ":w\n");
    assert!(write(&fx).is_some() || !a.buf.dirty());
}

#[test]
fn disk_2_a_reload_by_the_shell_ends_the_conflict_and_the_status_line_says_it_while_it_lasts() {
    let mut a = diverged();
    assert!(
        msg(&mut a).contains("modified · 1 change on disk · conflicts with the disk"),
        "{}",
        msg(&mut a)
    );
    // `:e!` of a loaded file: the loaded buffer is the disk's, whatever came before.
    feed(&mut a, ":e!\n");
    a.update(Event::Kept(Ok("/s/f.1.yours".into())));
    a.update(Event::Opened("f".into(), Ok(Some(THEIRS.as_bytes().to_vec())), 3));
    assert!(!msg(&mut a).contains("conflicts"));
}

#[test]
fn disk_6_the_lines_of_a_conflict_are_marked_and_bracket_c_walks_them() {
    let at = |a: &App| a.buf.line_of(a.buf.cursor);
    // Both sides changed lines 2 and 5; the cursor is elsewhere when the disk's version comes.
    let build = |keys: &str| {
        let mut a = named("a\nb\nc\nd\ne\nf\n");
        feed(&mut a, "jrXjjjrY");
        feed(&mut a, keys);
        disk(&mut a, "a\nB\nc\nd\nE\nf\n");
        assert_eq!(marks(&a), [(1, '~'), (4, '~')]);
        assert!(a.diverged());
        a
    };
    let mut a = build("gg");
    feed(&mut a, "]c");
    assert_eq!(at(&a), 1);
    feed(&mut a, "]c");
    assert_eq!(at(&a), 4);
    feed(&mut a, "]c");
    assert_eq!(msg(&mut a), "no next change", "a mark goes when the cursor has entered its line");
    let mut b = build("G");
    feed(&mut b, "[c");
    assert_eq!(at(&b), 4);
    feed(&mut b, "[c");
    assert_eq!(at(&b), 1);
    feed(&mut b, "[c");
    assert_eq!(msg(&mut b), "no previous change");
    assert!(b.vi.mode == Mode::Normal, "]c and [c start no mode of their own");
    assert!(b.diverged(), "walking the marks does not settle the conflict");
}

#[test]
fn disk_2_a_conflict_where_mine_deleted_lines_marks_the_deletion_point() {
    let mut a = named("a\nb\nc\n");
    feed(&mut a, "jdd"); // mine drops line 2
    disk(&mut a, "a\nB\nc\n");
    assert_eq!(text(&a), "a\nc");
    assert_eq!(marks(&a), [(1, '-')]);
    assert!(a.diverged());
}

#[test]
fn disk_2_a_clean_buffer_is_never_diverged() {
    let mut a = named(BASE);
    disk(&mut a, THEIRS);
    assert!(!a.diverged() && text(&a) == "one\nTWO\nthree");
}

#[test]
fn safe_6_a_recovered_snapshot_that_conflicts_with_the_disk_is_diverged() {
    let mut a = named(THEIRS);
    a.update(Event::Recover(BASE.into(), MINE_WITH_NL.into(), 1000, "/s/x.snap".into(), far()));
    assert_eq!(text(&a), MINE);
    assert!(a.diverged());
}

const MINE_WITH_NL: &str = "one\nXwo\nthree\n";

#[test]
fn safe_7_a_reload_asked_before_the_conflict_came_is_not_done_without_the_copy() {
    // `:e!` is typed while the buffer and the file agree on nothing to keep; the disk then comes to
    // conflict with mine, and only then is the file read: the load would throw mine away uncopied.
    let mut a = named(BASE);
    feed(&mut a, "jrX");
    let fx = feed(&mut a, ":e!\n");
    assert!(kept(&fx).is_none() && opens(&fx));
    disk(&mut a, THEIRS);
    feed(&mut a, "u"); // (the merge took the version of the buffer on; this brings it back)
    assert!(a.diverged());
    let fx = a.update(Event::Opened("f".into(), Ok(Some(THEIRS.as_bytes().to_vec())), 9));
    assert_eq!(text(&a), MINE, "mine is still here");
    assert_eq!(
        status(last_frame(&fx)),
        "not opened: the disk conflicts with yours meanwhile (:e! again)"
    );
    // Typed again, it copies mine first.
    assert!(kept(&feed(&mut a, ":e!\n")).is_some());
}
