//! SAFE-6, CLI-3 on App: when snapshots are asked for and dropped, how a snapshot is recovered.
//! The journal's files are tests/journal.rs, and the thread that writes them is in scripts/e2e.sh.
mod common;
use common::*;
use si::core::app::{App, Effect, Event};

fn named(text: &str) -> App {
    app_with(Some("f"), text.as_bytes(), (80, 8))
}

/// The effects the journal cares about, in order: (path, base, text) of a Snapshot.
#[derive(Debug, PartialEq)]
enum J {
    Snap(Option<&'static str>, &'static str, &'static str),
    Unsnap,
    Quit(i32),
}

/// What a Snapshot effect holds, leaked to be comparable with a `J` written in a test.
fn journal(fx: &[Effect]) -> Vec<J> {
    let leak = |s: String| -> &'static str { Box::leak(s.into_boxed_str()) };
    fx.iter()
        .filter_map(|e| match e {
            Effect::Snapshot { path, base, text } => Some(J::Snap(
                path.clone().map(leak),
                leak(base.to_string()),
                leak(text.to_string()),
            )),
            Effect::Unsnapshot => Some(J::Unsnap),
            Effect::Quit(c) => Some(J::Quit(*c)),
            _ => None,
        })
        .collect()
}

fn tick(a: &mut App, ms: u64) -> Vec<J> {
    journal(&a.update(Event::Tick(ms)))
}

fn typed_keys(a: &mut App, keys: &str) -> Vec<J> {
    journal(&feed(a, keys))
}

fn msg(a: &mut App) -> String {
    status(&screen(a)).to_string()
}

fn recover(a: &mut App, base: &str, text: &str, ms: u64) -> Vec<Effect> {
    a.update(Event::Recover(base.into(), text.into(), ms, "/s/f.snap".into()))
}

#[test]
fn safe_6_a_dirty_buffer_is_snapshotted_at_most_once_a_second() {
    let mut a = named("a\nb\n");
    assert_eq!(tick(&mut a, 1000), [], "a clean buffer needs none");
    feed(&mut a, "x");
    assert_eq!(tick(&mut a, 2000), [J::Snap(Some("f"), "a\nb", "\nb")]);
    feed(&mut a, "jx");
    assert_eq!((tick(&mut a, 2001), tick(&mut a, 2999)), (vec![], vec![]), "less than a second");
    assert_eq!(tick(&mut a, 3000), [J::Snap(Some("f"), "a\nb", "\n")]);
    assert_eq!(tick(&mut a, 4000), [], "nothing changed since");
    assert_eq!(tick(&mut a, 90_000), []);
    feed(&mut a, "ix\x1b");
    assert_eq!(tick(&mut a, 90_000), [J::Snap(Some("f"), "a\nb", "\nx")]);
    feed(&mut a, "ix\x1b");
    assert_eq!(tick(&mut a, 90_999), [], "the new edit waits for the second to pass");
    assert_eq!(tick(&mut a, 91_000), [J::Snap(Some("f"), "a\nb", "\nxx")]);
}

#[test]
fn safe_6_ticks_snapshot_without_a_render() {
    let mut a = named("a\n");
    feed(&mut a, "x");
    let fx = a.update(Event::Tick(1000));
    assert!(matches!(&fx[..], [Effect::Snapshot { .. }]), "{fx:?}");
}

#[test]
fn safe_6_the_snapshot_holds_the_base_the_text_and_the_name() {
    let mut a = named("one\ntwo\n");
    typed_keys(&mut a, "jddix");
    assert_eq!(tick(&mut a, 5000), [J::Snap(Some("f"), "one\ntwo", "xone")]);
}

#[test]
fn safe_6_an_unnamed_buffer_is_protected_too() {
    let mut a = app("");
    typed_keys(&mut a, "ihello");
    assert_eq!(tick(&mut a, 1000), [J::Snap(None, "", "hello")]);
    assert_eq!(typed_keys(&mut a, "\x1b:q!\n"), [J::Unsnap, J::Quit(0)]);
}

#[test]
fn safe_6_becoming_clean_forgets_the_snapshot() {
    let mut a = named("a\n");
    feed(&mut a, "x");
    assert_eq!(tick(&mut a, 1000).len(), 1);
    assert_eq!(typed_keys(&mut a, "u"), [J::Unsnap], "undone back to the saved text");
    assert_eq!((tick(&mut a, 2000), tick(&mut a, 3000)), (vec![], vec![]));
    assert_eq!(typed_keys(&mut a, "<C-r>"), [], "dirty again: nothing until the next tick");
    assert_eq!(tick(&mut a, 4000), [J::Snap(Some("f"), "a", "")], "redo is snapshotted again");
}

#[test]
fn safe_6_nothing_to_forget_when_no_snapshot_was_taken() {
    let mut a = named("a\n");
    assert_eq!(typed_keys(&mut a, "xu"), [], "undone before the first tick");
}

#[test]
fn safe_6_a_successful_save_forgets_the_snapshot() {
    let mut a = named("a\n");
    feed(&mut a, "x");
    tick(&mut a, 1000);
    feed(&mut a, ":w\n");
    assert_eq!(journal(&a.update(Event::Saved(Err("disk full".into())))), [], "a failed save");
    feed(&mut a, ":w\n");
    assert_eq!(journal(&a.update(Event::SaveConflict(b"other\n".to_vec()))), []);
    feed(&mut a, ":w!\n");
    assert_eq!(journal(&a.update(Event::Saved(Ok((1, None))))), [J::Unsnap]);
    assert_eq!(tick(&mut a, 2000), [], "clean");
}

#[test]
fn safe_6_a_save_with_newer_edits_keeps_a_snapshot_of_them() {
    let mut a = named("a\n");
    feed(&mut a, "x:w\n");
    feed(&mut a, "iq\x1b");
    assert_eq!(journal(&a.update(Event::Saved(Ok((1, None))))), [], "edited while saving");
    assert_eq!(tick(&mut a, 1000), [J::Snap(Some("f"), "", "q")], "the saved text is the base");
}

#[test]
fn safe_6_every_way_of_quitting_forgets_the_snapshots() {
    for (keys, code) in [(":q\n", 0), ("q", 0), (":q!\n", 0), ("ZQ", 0), (":x\n", 0), (":cq\n", 1)]
    {
        let mut a = named("a\n");
        assert_eq!(typed_keys(&mut a, keys), [J::Unsnap, J::Quit(code)], "{keys:?} when clean");
    }
    // Giving up on a modified buffer: its snapshot goes too.
    for (keys, code) in [(":q!\n", 0), ("ZQ", 0), (":cq\n", 1)] {
        let mut a = named("a\n");
        feed(&mut a, "x");
        tick(&mut a, 1000);
        assert_eq!(typed_keys(&mut a, keys), [J::Unsnap, J::Quit(code)], "{keys:?} when modified");
    }
}

#[test]
fn safe_6_a_successful_save_and_quit_forgets_before_it_quits() {
    for keys in [":wq\n", ":x\n", "ZZ", ":wq!\n"] {
        let mut a = named("a\n");
        feed(&mut a, "x");
        tick(&mut a, 1000);
        assert_eq!(typed_keys(&mut a, keys), [], "{keys:?}: the save comes first");
        assert_eq!(journal(&a.update(Event::Saved(Ok((1, None))))), [J::Unsnap, J::Quit(0)]);
    }
    let mut a = named("a\n");
    feed(&mut a, "x:wq\n");
    let fx = a.update(Event::Saved(Err("disk full".into())));
    assert_eq!(journal(&fx), [], "a failed save keeps the snapshots and does not quit");
    feed(&mut a, ":wq\n");
    feed(&mut a, "iy\x1b");
    let fx = a.update(Event::Saved(Ok((2, None))));
    assert_eq!(journal(&fx), [], "edited since the save: not quitting, nothing to forget");
}

#[test]
fn safe_6_a_signal_snapshots_the_text_and_then_quits() {
    let mut a = named("a\n");
    feed(&mut a, "x");
    let fx = a.update(Event::Signal(15));
    assert_eq!(journal(&fx), [J::Snap(Some("f"), "a", ""), J::Quit(143)]);
    assert!(!fx.iter().any(|e| matches!(e, Effect::Render(_))));
    let mut a = named("a\n");
    feed(&mut a, "x");
    tick(&mut a, 1000);
    feed(&mut a, "ib\x1b");
    let want = [J::Snap(Some("f"), "a", "b"), J::Quit(129)];
    assert_eq!(journal(&a.update(Event::Signal(1))), want, "the throttle does not apply");
}

#[test]
fn safe_6_a_signal_keeps_the_snapshots_and_a_clean_buffer_has_none_to_take() {
    let mut a = named("a\n");
    assert_eq!(journal(&a.update(Event::Signal(15))), [J::Quit(143)]);
    let mut a = named("a\n");
    feed(&mut a, "x");
    tick(&mut a, 1000);
    assert!(!journal(&a.update(Event::Signal(15))).contains(&J::Unsnap));
}

#[test]
fn safe_6_a_signal_while_the_unnamed_buffer_is_dirty_snapshots_it() {
    let mut a = app("");
    feed(&mut a, "ihi");
    assert_eq!(journal(&a.update(Event::Signal(15))), [J::Snap(None, "", "hi"), J::Quit(143)]);
}

#[test]
fn safe_6_recovery_applies_as_one_undo_step() {
    let mut a = named("one\ntwo\n");
    let fx = recover(&mut a, "one\ntwo", "one\nTWO\nthree", 180_000);
    assert_eq!(status(last_frame(&fx)), "recovered unsaved changes from 3 minutes ago · u to undo");
    assert_eq!((text(&a).as_str(), a.buf.dirty()), ("one\nTWO\nthree", true));
    assert_eq!(a.buf.encode(), b"one\nTWO\nthree\n", "saved as the file was: final newline");
    assert_eq!(typed_keys(&mut a, "u"), [J::Unsnap], "one `u` is back to the disk text");
    assert_eq!((text(&a).as_str(), a.buf.dirty()), ("one\ntwo", false));
    feed(&mut a, "u");
    assert_eq!(msg(&mut a), "nothing to undo", "it was a single step");
    feed(&mut a, "<C-r>");
    assert_eq!((text(&a).as_str(), a.buf.dirty()), ("one\nTWO\nthree", true));
}

#[test]
fn safe_6_recovered_text_is_snapshotted_again_after_the_next_edit() {
    let mut a = named("one\n");
    recover(&mut a, "one", "two", 1000);
    assert_eq!(tick(&mut a, 1000), [], "the journal already holds this text");
    feed(&mut a, "x");
    assert_eq!(tick(&mut a, 2000), [J::Snap(Some("f"), "one", "wo")]);
}

#[test]
fn safe_6_a_snapshot_of_another_disk_state_is_kept_and_not_applied() {
    let mut a = named("changed on disk\n");
    let fx = recover(&mut a, "old", "mine", 5000);
    assert_eq!(
        status(last_frame(&fx)),
        "kept unsaved changes from 5 seconds ago, not applied: /s/f.snap"
    );
    assert_eq!((text(&a).as_str(), a.buf.dirty()), ("changed on disk", false));
    assert_eq!(journal(&fx), [], "nothing is asked of the journal: the file stays");
    assert_eq!((tick(&mut a, 1000), tick(&mut a, 2000)), (vec![], vec![]));
    assert_eq!(typed_keys(&mut a, ":q\n"), [J::Unsnap, J::Quit(0)]);
}

#[test]
fn safe_6_a_snapshot_is_not_applied_to_a_read_only_buffer() {
    let mut a = app_with(Some("f"), b"a\xff\n", (80, 8));
    let base = a.buf.base.to_string();
    let fx = recover(&mut a, &base, "mine", 1000);
    assert!(status(last_frame(&fx)).starts_with("kept unsaved changes"));
    assert!(!a.buf.dirty() && a.buf.readonly.is_some());
}

#[test]
fn safe_6_the_age_is_told_in_the_largest_unit() {
    let table = [
        (0, "0 seconds"),
        (999, "0 seconds"),
        (1000, "1 second"),
        (2000, "2 seconds"),
        (59_999, "59 seconds"),
        (60_000, "1 minute"),
        (119_999, "1 minute"),
        (180_000, "3 minutes"),
        (3_599_000, "59 minutes"),
        (3_600_000, "1 hour"),
        (7_200_000, "2 hours"),
        (86_399_000, "23 hours"),
        (86_400_000, "1 day"),
        (3 * 86_400_000 + 5000, "3 days"),
        (400 * 86_400_000, "400 days"),
    ];
    for (ms, want) in table {
        let mut a = named("a\n");
        let fx = recover(&mut a, "a", "b", ms);
        let want = format!("recovered unsaved changes from {want} ago · u to undo");
        assert_eq!(status(last_frame(&fx)), want, "{ms} ms");
    }
}

#[test]
fn safe_6_recovering_into_an_empty_new_file_works() {
    let mut a = named("");
    recover(&mut a, "", "kept text", 2000);
    assert_eq!((text(&a).as_str(), a.buf.dirty()), ("kept text", true));
    let mut a = app("");
    recover(&mut a, "", "unnamed text", 2000);
    assert_eq!(text(&a), "unnamed text");
}
