//! PROP-5, PROP-6 on App: `:{range}!cmd` and Visual `!` start a filter that runs without anyone
//! waiting for it, the status line counts its seconds, Ctrl-c cancels it, and its result comes
//! back as `FilterDone`: after a clean exit every change is a proposal on top of what the buffer
//! has become meanwhile, after a failure nothing but the first line of stderr.
mod common;
use common::*;
use si::core::app::{App, Effect, Event};
use si::core::vi::Mode;
use std::time::Instant;

const PROPOSALS: &str = "unresolved proposals: ]c to review, :q! discards";
const RUNNING: &str = "filter 0s · Ctrl-c cancel";

fn named(text: &str) -> App {
    app_with(Some("f"), text.as_bytes(), (70, 10))
}

fn st(a: &mut App) -> String {
    status(&screen(a)).to_string()
}

fn spawn_of(fx: &[Effect]) -> Option<(&str, &[u8])> {
    fx.iter().find_map(|e| match e {
        Effect::Spawn { cmd, input } => Some((cmd.as_str(), input.as_slice())),
        _ => None,
    })
}

fn count(fx: &[Effect], what: fn(&Effect) -> bool) -> usize {
    fx.iter().filter(|e| what(e)).count()
}

fn is_kill(e: &Effect) -> bool {
    matches!(e, Effect::Kill)
}

/// The filter ends: its exit code, stdout and stderr.
fn done(a: &mut App, status: Option<i32>, out: &str, err: &str) -> Vec<Effect> {
    let (stdout, stderr) = (out.as_bytes().to_vec(), err.as_bytes().to_vec());
    a.update(Event::FilterDone(status, stdout, stderr, far()))
}

fn ok(a: &mut App, out: &str) -> Vec<Effect> {
    done(a, Some(0), out, "")
}

/// The lines of mine of every hunk, and what each would put in their place.
fn mines(a: &App) -> Vec<(usize, usize)> {
    a.buf.hunks.iter().map(|h| (h.mine.start, h.mine.end)).collect()
}

fn theirs(a: &App) -> Vec<String> {
    a.buf.hunks.iter().map(|h| h.theirs.to_string()).collect()
}

/// Three lines to be upper-cased, with unchanged lines between them: three hunks.
const THREE: &str = "one\n1\ntwo\n2\nthree\n";
const UPPER: &str = "ONE\n1\nTWO\n2\nTHREE\n";

fn three() -> App {
    let mut a = named(THREE);
    feed(&mut a, ":%!tr a-z A-Z\n");
    ok(&mut a, UPPER);
    a
}

// ---------------------------------------------------------------------------------------------
// PROP-5: running
// ---------------------------------------------------------------------------------------------

#[test]
fn prop_5_a_range_is_sent_to_the_shell_as_lines() {
    let mut a = named(THREE);
    let fx = feed(&mut a, ":2,4!sort -r\n");
    assert_eq!(spawn_of(&fx), Some(("sort -r", &b"1\ntwo\n2\n"[..])));
    assert_eq!(st(&mut a), format!("f  1:1  NORMAL  {RUNNING}"));
    assert_eq!(text(&a), THREE.trim_end(), "the text is not touched until the result is reviewed");
    assert!(!a.buf.dirty());
}

#[test]
fn prop_5_visual_bang_filters_the_selection() {
    let mut a = named(THREE);
    let fx = feed(&mut a, "jVj!tr a-z A-Z\n");
    assert_eq!(spawn_of(&fx), Some(("tr a-z A-Z", &b"1\ntwo\n"[..])));
    let mut a = named(THREE);
    let fx = feed(&mut a, "!jcat\n");
    assert_eq!(spawn_of(&fx), Some(("cat", &b"one\n1\n"[..])), "`!{{motion}}`");
}

#[test]
fn prop_5_the_last_line_without_a_final_newline_is_sent_with_one() {
    let mut a = named("a\nb");
    let fx = feed(&mut a, ":$!cat\n");
    assert_eq!(spawn_of(&fx), Some(("cat", &b"b\n"[..])));
}

#[test]
fn prop_5_editing_and_saving_go_on_while_it_runs() {
    let mut a = named(THREE);
    feed(&mut a, ":%!sleep 30\n");
    feed(&mut a, "ggxjdd");
    assert_eq!(text(&a), "ne\ntwo\n2\nthree");
    assert!(st(&mut a).contains(&format!("modified · {RUNNING}")), "{}", st(&mut a));
    let fx = feed(&mut a, ":w\n");
    assert!(fx.iter().any(|e| matches!(e, Effect::Write { .. })), "a save is not held up");
}

#[test]
fn prop_5_the_status_line_counts_the_seconds_by_the_ticks() {
    let mut a = named(THREE);
    assert!(a.update(Event::Tick(5000)).is_empty(), "no filter: a tick draws nothing");
    feed(&mut a, ":1!sleep 30\n");
    for (ms, want) in [(6000, "1s"), (7000, "2s"), (9000, "4s"), (70_000, "65s")] {
        let fx = a.update(Event::Tick(ms));
        assert_eq!(
            status(last_frame(&fx)),
            format!("f  1:1  NORMAL  filter {want} · Ctrl-c cancel")
        );
    }
    ok(&mut a, THREE);
    assert!(a.update(Event::Tick(71_000)).is_empty(), "the filter is over: a tick draws nothing");
}

#[test]
fn prop_5_a_second_filter_is_refused_while_one_runs() {
    let mut a = named(THREE);
    assert!(spawn_of(&feed(&mut a, ":1!sleep 30\n")).is_some());
    let fx = feed(&mut a, ":2!cat\n");
    assert!(spawn_of(&fx).is_none(), "one at a time");
    assert_eq!(st(&mut a), "a filter is already running (Ctrl-c cancels it)");
    let fx = feed(&mut a, "!jcat\n");
    assert!(spawn_of(&fx).is_none());
    ok(&mut a, THREE);
    assert!(spawn_of(&feed(&mut a, ":2!cat\n")).is_some(), "after the first ends");
}

#[test]
fn prop_5_ctrl_c_cancels_the_filter_and_nothing_comes_of_it() {
    let mut a = named(THREE);
    feed(&mut a, ":%!sleep 30\n");
    let fx = feed(&mut a, "<C-c>");
    assert_eq!(count(&fx, is_kill), 1);
    assert_eq!(st(&mut a), "cancelling the filter");
    // The process dies and the answer comes: killed by a signal.
    done(&mut a, None, "", "");
    assert_eq!(st(&mut a), "filter cancelled");
    assert!(a.buf.hunks.is_empty() && !a.buf.dirty());
    // With nothing to cancel Ctrl-c says what it always said (VI-13).
    let fx = feed(&mut a, "<C-c>");
    assert_eq!(count(&fx, is_kill), 0);
    assert_eq!(st(&mut a), "q to quit");
    // A new filter may start; a result that comes anyway after Ctrl-c is not taken.
    feed(&mut a, ":%!cat\n<C-c>");
    ok(&mut a, UPPER);
    assert_eq!((st(&mut a).as_str(), a.buf.hunks.len()), ("filter cancelled", 0));
}

#[test]
fn prop_5_ctrl_c_in_other_places_keeps_its_old_job() {
    let mut a = named(THREE);
    feed(&mut a, ":%!sleep 30\n");
    for keys in ["i<C-c>", "v<C-c>", "d<C-c>", "2<C-c>", ":<C-c>", "/x<C-c>", "Z<C-c>"] {
        let fx = feed(&mut a, keys);
        assert_eq!(count(&fx, is_kill), 0, "{keys}");
        assert_eq!(a.vi.mode, Mode::Normal, "{keys}");
    }
    assert!(st(&mut a).ends_with(RUNNING) || st(&mut a).contains("Ctrl-c cancel"));
    assert_eq!(count(&feed(&mut a, "<C-c>"), is_kill), 1, "in Normal mode it cancels");
}

#[test]
fn prop_5_it_is_refused_where_it_could_not_be_used() {
    let mut a = app_with(Some("f"), &[b'a', 0xff], (70, 6));
    assert!(spawn_of(&feed(&mut a, ":1!cat\n")).is_none());
    assert!(st(&mut a).starts_with("read-only: "), "{}", st(&mut a));
    let mut a = named(THREE);
    for keys in [":1!\n", ":1!   \n", ":%!<CR>"] {
        assert!(spawn_of(&feed(&mut a, keys)).is_none(), "{keys:?}");
        assert_eq!(st(&mut a), "usage: :{range}!cmd");
    }
    // Another file does not replace the buffer under a running filter.
    feed(&mut a, ":1!sleep 30\n");
    let fx = feed(&mut a, ":e g\n");
    assert!(fx.iter().any(|e| matches!(e, Effect::Open(_))));
    a.update(Event::Opened("g".into(), Ok(Some(b"other\n".to_vec())), 0));
    assert_eq!(
        (st(&mut a).as_str(), text(&a).as_str()),
        ("not opened: a filter is running", THREE.trim_end())
    );
    ok(&mut a, "ONE\n");
    assert_eq!(a.buf.hunks.len(), 1, "the filter was not lost");
}

#[test]
fn prop_5_quitting_is_not_held_up_by_a_filter() {
    let mut a = named(THREE);
    feed(&mut a, ":1!sleep 30\n");
    let fx = feed(&mut a, ":q\n");
    assert!(fx.iter().any(|e| matches!(e, Effect::Quit(0))), "the shell stops the filter then");
}

// ---------------------------------------------------------------------------------------------
// PROP-6: the result
// ---------------------------------------------------------------------------------------------

#[test]
fn prop_6_every_change_becomes_a_proposal_and_the_text_stays() {
    let mut a = named(THREE);
    feed(&mut a, ":%!tr a-z A-Z\n");
    ok(&mut a, UPPER);
    assert_eq!(mines(&a), [(0, 1), (2, 3), (4, 5)]);
    assert_eq!(theirs(&a), ["ONE\n", "TWO\n", "THREE\n"]);
    assert!(
        a.buf.hunks.iter().all(|h| !h.disk && h.words.is_some()),
        "a filter's, with word diffs"
    );
    assert_eq!(text(&a), THREE.trim_end());
    assert!(a.buf.marks.is_empty() && !a.buf.dirty());
    assert_eq!(st(&mut a), "3 proposals from the filter · ]c to review");
    feed(&mut a, "j");
    assert_eq!(st(&mut a), "f  2:1  NORMAL  3 proposals");
}

#[test]
fn prop_6_the_proposals_are_reviewed_with_the_review_keys() {
    let mut a = three();
    feed(&mut a, "]c");
    assert_eq!(a.vi.mode, Mode::Review);
    assert_eq!(
        st(&mut a),
        format!("f  1:1  REVIEW 1/3  {}", "y accept · n reject · Y all · N none · Esc back")
    );
    feed(&mut a, "y");
    assert_eq!((text(&a), a.buf.dirty()), ("ONE\n1\ntwo\n2\nthree".to_string(), true));
    feed(&mut a, "n");
    assert_eq!(text(&a), "ONE\n1\ntwo\n2\nthree");
    feed(&mut a, "y");
    assert_eq!((text(&a), a.buf.hunks.len()), ("ONE\n1\ntwo\n2\nTHREE".to_string(), 0));
    assert_eq!((a.vi.mode, st(&mut a).as_str()), (Mode::Normal, "1 proposal accepted"));
    // Y and N.
    let mut a = three();
    feed(&mut a, "]cY");
    assert_eq!((text(&a), a.buf.hunks.len()), (UPPER.trim_end().to_string(), 0));
    let mut a = three();
    feed(&mut a, "]cN");
    assert_eq!(
        (text(&a), a.buf.hunks.len(), a.buf.dirty()),
        (THREE.trim_end().to_string(), 0, false)
    );
}

#[test]
fn prop_6_edits_made_meanwhile_are_kept_and_the_proposals_follow_their_lines() {
    let mut a = named(THREE);
    feed(&mut a, ":%!tr a-z A-Z\n");
    // While it runs: a line at the top, and a change in a line the filter leaves alone.
    feed(&mut a, "ggOnew<Esc>");
    feed(&mut a, "4j$a!<Esc>");
    assert_eq!(text(&a), "new\none\n1\ntwo\n2!\nthree");
    ok(&mut a, UPPER);
    assert_eq!(mines(&a), [(1, 2), (3, 4), (5, 6)], "the hunks sit on the lines they are about");
    assert_eq!(text(&a), "new\none\n1\ntwo\n2!\nthree", "the edits stay");
    feed(&mut a, "gg]cY");
    assert_eq!(
        text(&a),
        "new\nONE\n1\nTWO\n2!\nTHREE",
        "and the accepted words come in around them"
    );
}

#[test]
fn prop_6_an_edit_on_a_line_the_filter_changed_conflicts_and_nothing_is_lost() {
    let mut a = named("one\ntwo\nthree\n");
    feed(&mut a, ":%!tr a-z A-Z\n");
    feed(&mut a, "jA!<Esc>");
    ok(&mut a, "ONE\nTWO\nTHREE\n");
    // The change is one block of three lines; mine ("two!") is inside it.
    assert_eq!(theirs(&a), ["ONE\nTWO\nTHREE\n"]);
    assert_eq!(text(&a), "one\ntwo!\nthree");
    feed(&mut a, "gg]cn");
    assert_eq!(text(&a), "one\ntwo!\nthree", "n keeps mine");
    let mut b = named("one\ntwo\nthree\n");
    feed(&mut b, ":%!tr a-z A-Z\njA!<Esc>");
    ok(&mut b, "ONE\nTWO\nTHREE\n");
    feed(&mut b, "gg]cy");
    assert_eq!(
        text(&b),
        "ONE\nTWO\nTHREE",
        "y takes the filter's version, by the reviewer's decision"
    );
    feed(&mut b, "u");
    assert_eq!(text(&b), "one\ntwo!\nthree", "and u gives mine back");
}

#[test]
fn prop_6_a_failed_filter_is_discarded_and_its_stderr_is_shown() {
    let mut a = named(THREE);
    feed(&mut a, ":%!tr\n");
    done(&mut a, Some(2), "partial\n", "tr: missing operand\nTry 'tr --help'\n");
    assert_eq!(st(&mut a), "tr: missing operand");
    assert!(a.buf.hunks.is_empty() && !a.buf.dirty() && text(&a) == THREE.trim_end());
    // Blank first lines are skipped; no stderr says what happened instead.
    for (status, err, said) in [
        (Some(1), "\n  \nsh: 1: x: not found\n", "sh: 1: x: not found"),
        (Some(3), "", "filter failed (exit 3)"),
        (None, "", "filter failed (killed)"),
    ] {
        feed(&mut a, ":1!x\n");
        done(&mut a, status, "ONE\n", err);
        assert_eq!((st(&mut a).as_str(), a.buf.hunks.len()), (said, 0));
    }
    assert!(spawn_of(&feed(&mut a, ":1!x\n")).is_some(), "a failure ends the filter");
}

#[test]
fn prop_6_an_empty_output_is_a_valid_result_and_proposes_deleting_the_lines() {
    let mut a = named("a\nb\nc\n");
    feed(&mut a, ":2!true\n");
    ok(&mut a, "");
    assert_eq!((mines(&a), theirs(&a)), (vec![(1, 2)], vec!["".to_string()]));
    feed(&mut a, "]cy");
    assert_eq!(text(&a), "a\nc");
    // The whole buffer.
    let mut a = named("a\nb\n");
    feed(&mut a, ":%!true\n");
    ok(&mut a, "");
    feed(&mut a, "]cy");
    assert_eq!(text(&a), "");
    assert!(a.buf.dirty());
}

#[test]
fn prop_6_an_output_that_changes_nothing_says_so() {
    let mut a = named(THREE);
    feed(&mut a, ":%!cat\n");
    ok(&mut a, THREE);
    assert_eq!((st(&mut a).as_str(), a.buf.hunks.len()), ("no changes from the filter", 0));
}

#[test]
fn prop_6_the_output_may_be_longer_or_shorter_and_lacks_no_final_newline() {
    let mut a = named("a\nb\nc\n");
    feed(&mut a, ":2!x\n");
    ok(&mut a, "b1\nb2\nb3");
    assert_eq!(theirs(&a), ["b1\nb2\nb3\n"], "a final newline is added");
    feed(&mut a, "]cy");
    assert_eq!(text(&a), "a\nb1\nb2\nb3\nc");
    let mut a = named("a\nb\nc\n");
    feed(&mut a, ":1,2!x\n");
    ok(&mut a, "ab\n");
    feed(&mut a, "]cy");
    assert_eq!(text(&a), "ab\nc");
}

#[test]
fn prop_6_the_last_line_keeps_the_end_of_the_file_as_it_was() {
    let mut a = named("a\nb");
    feed(&mut a, ":2!tr a-z A-Z\n");
    ok(&mut a, "B\n");
    feed(&mut a, "]cy");
    assert_eq!(text(&a), "a\nB");
    assert_eq!(a.buf.encode(), b"a\nB", "a file without a final newline has none after");
    let mut a = named("a\nb\n");
    feed(&mut a, ":2!tr a-z A-Z\n");
    ok(&mut a, "B");
    feed(&mut a, "]cy");
    assert_eq!(a.buf.encode(), b"a\nB\n");
}

#[test]
fn prop_6_an_output_that_is_not_text_is_discarded() {
    for bad in [&b"\xff\xfe\n"[..], b"a\0b\n"] {
        let mut a = named("a\nb\n");
        feed(&mut a, ":1!x\n");
        a.update(Event::FilterDone(Some(0), bad.to_vec(), vec![], far()));
        assert_eq!(
            (st(&mut a).as_str(), a.buf.hunks.len()),
            ("filter output is not text: discarded", 0)
        );
    }
}

#[test]
fn prop_6_proposals_do_not_stop_a_save_but_stop_quitting() {
    let mut a = three();
    let fx = feed(&mut a, ":w\n");
    assert!(fx.iter().any(|e| matches!(e, Effect::Write { .. })), "SAFE-7: filter hunks allow :w");
    a.update(Event::Saved(Ok((hash_of(&a), None)), 0));
    for keys in [":q\n", "q", ":x\n", ":wq\n", "ZZ"] {
        let fx = feed(&mut a, keys);
        assert!(!fx.iter().any(|e| matches!(e, Effect::Quit(_))), "{keys:?}");
    }
    assert!(st(&mut a).contains(PROPOSALS) || st(&mut a).contains("proposals"));
    let fx = feed(&mut a, ":q!\n");
    assert!(fx.iter().any(|e| matches!(e, Effect::Quit(0))));
}

fn hash_of(a: &App) -> u64 {
    si::core::buffer::hash(&a.buf.encode())
}

#[test]
fn prop_6_undo_takes_the_proposals_away_and_redo_brings_them_back() {
    let mut a = three();
    feed(&mut a, "u");
    assert_eq!((a.buf.hunks.len(), a.buf.dirty()), (0, false));
    feed(&mut a, "<C-r>");
    assert_eq!((a.buf.hunks.len(), a.buf.dirty()), (3, false));
    // An edit on top of them and an undo of it leave them alone.
    feed(&mut a, "jxu");
    assert_eq!(a.buf.hunks.len(), 3);
}

#[test]
fn prop_6_a_later_result_adds_to_the_earlier_one() {
    let mut a = named(THREE);
    feed(&mut a, ":1!x\n");
    ok(&mut a, "ONE\n");
    feed(&mut a, ":5!x\n");
    ok(&mut a, "THREE\n");
    assert_eq!(
        (mines(&a), theirs(&a)),
        (vec![(0, 1), (4, 5)], vec!["ONE\n".to_string(), "THREE\n".to_string()])
    );
    feed(&mut a, ":3!x\n");
    ok(&mut a, "TWO\n");
    assert_eq!(mines(&a), [(0, 1), (2, 3), (4, 5)], "in the order of the lines");
}

#[test]
fn prop_6_the_result_comes_in_any_mode_without_changing_it() {
    for (keys, mode) in [
        ("i", Mode::Insert),
        ("v", Mode::Visual),
        ("V", Mode::VLine),
        (":", Mode::Cmd),
        ("]c", Mode::Normal),
    ] {
        let mut a = named(THREE);
        feed(&mut a, ":%!x\n");
        feed(&mut a, keys);
        ok(&mut a, UPPER);
        assert_eq!(a.vi.mode, if keys == "]c" { Mode::Normal } else { mode }, "{keys}");
        assert_eq!(a.buf.hunks.len(), 3, "{keys}");
    }
    // Typing goes on in Insert mode after the result.
    let mut a = named("one\n");
    feed(&mut a, ":1!x\nA ab");
    ok(&mut a, "ONE\n");
    feed(&mut a, "c<Esc>");
    assert_eq!((text(&a), a.buf.hunks.len()), ("one abc".to_string(), 1));
}

#[test]
fn prop_6_an_answer_nobody_waits_for_is_ignored() {
    let mut a = named(THREE);
    let before = st(&mut a);
    ok(&mut a, UPPER);
    done(&mut a, Some(1), "", "boom");
    assert_eq!((st(&mut a), a.buf.hunks.len()), (before, 0));
}

#[test]
fn prop_6_a_diff_that_gave_up_is_coarse_but_accepting_it_all_gives_the_output() {
    let mut a = named("a b c\nd e f\n");
    feed(&mut a, ":%!x\n");
    let late = Instant::now() - std::time::Duration::from_secs(1);
    let out = "a B c\nd E f\n";
    a.update(Event::FilterDone(Some(0), out.into(), vec![], late));
    feed(&mut a, "]cY");
    assert_eq!(text(&a), out.trim_end());
}
