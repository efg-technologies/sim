//! FILT-1, FILT-2 on App: `:{range}!cmd` and Visual `!` start a filter that runs without anyone
//! waiting for it, the status line counts its seconds, Ctrl-c cancels it, and its result comes
//! back as `FilterDone`: after a clean exit it takes the place of the lines it was given, as one
//! undo step on top of what the buffer has become meanwhile, after a failure nothing but the first
//! line of stderr.
mod common;
use common::*;
use sim_core::core::app::{App, Effect, Event};
use sim_core::core::vi::Mode;
use std::time::Instant;

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

/// Three lines to be upper-cased, with unchanged lines between them.
const THREE: &str = "one\n1\ntwo\n2\nthree\n";
const UPPER: &str = "ONE\n1\nTWO\n2\nTHREE\n";

// ---------------------------------------------------------------------------------------------
// FILT-1: running
// ---------------------------------------------------------------------------------------------

#[test]
fn filt_1_a_range_is_sent_to_the_shell_as_lines() {
    let mut a = named(THREE);
    let fx = feed(&mut a, ":2,4!sort -r\n");
    assert_eq!(spawn_of(&fx), Some(("sort -r", &b"1\ntwo\n2\n"[..])));
    assert_eq!(st(&mut a), format!("f  1:1  NORMAL  {RUNNING}"));
    assert_eq!(text(&a), THREE.trim_end(), "the text is not touched until the result comes");
    assert!(!a.buf.dirty());
}

#[test]
fn filt_1_visual_bang_filters_the_selection() {
    let mut a = named(THREE);
    let fx = feed(&mut a, "jVj!tr a-z A-Z\n");
    assert_eq!(spawn_of(&fx), Some(("tr a-z A-Z", &b"1\ntwo\n"[..])));
    let mut a = named(THREE);
    let fx = feed(&mut a, "!jcat\n");
    assert_eq!(spawn_of(&fx), Some(("cat", &b"one\n1\n"[..])), "`!{{motion}}`");
}

#[test]
fn filt_1_the_last_line_without_a_final_newline_is_sent_with_one() {
    let mut a = named("a\nb");
    let fx = feed(&mut a, ":$!cat\n");
    assert_eq!(spawn_of(&fx), Some(("cat", &b"b\n"[..])));
}

#[test]
fn filt_1_editing_and_saving_go_on_while_it_runs() {
    let mut a = named(THREE);
    feed(&mut a, ":%!sleep 30\n");
    feed(&mut a, "ggxjdd");
    assert_eq!(text(&a), "ne\ntwo\n2\nthree");
    assert!(st(&mut a).contains(&format!("modified · {RUNNING}")), "{}", st(&mut a));
    let fx = feed(&mut a, ":w\n");
    assert!(fx.iter().any(|e| matches!(e, Effect::Write { .. })), "a save is not held up");
}

#[test]
fn filt_1_the_status_line_counts_the_seconds_by_the_ticks() {
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
    ok(&mut a, "one\n");
    assert!(a.update(Event::Tick(71_000)).is_empty(), "the filter is over: a tick draws nothing");
}

#[test]
fn filt_1_a_second_filter_is_refused_while_one_runs() {
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
fn filt_1_ctrl_c_cancels_the_filter_and_nothing_comes_of_it() {
    let mut a = named(THREE);
    feed(&mut a, ":%!sleep 30\n");
    let fx = feed(&mut a, "<C-c>");
    assert_eq!(count(&fx, is_kill), 1);
    assert_eq!(st(&mut a), "cancelling the filter");
    // The process dies and the answer comes: killed by a signal.
    done(&mut a, None, "", "");
    assert_eq!(st(&mut a), "filter cancelled");
    assert!(!a.buf.dirty());
    // With nothing to cancel Ctrl-c says what it always said (VI-13).
    let fx = feed(&mut a, "<C-c>");
    assert_eq!(count(&fx, is_kill), 0);
    assert_eq!(st(&mut a), "q to quit");
    // A new filter may start; a result that comes anyway after Ctrl-c is not taken.
    feed(&mut a, ":%!cat\n<C-c>");
    ok(&mut a, UPPER);
    assert_eq!((st(&mut a).as_str(), a.buf.dirty()), ("filter cancelled", false));
}

#[test]
fn filt_1_ctrl_c_in_other_places_keeps_its_old_job() {
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
fn filt_1_it_is_refused_where_it_could_not_be_used() {
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
    assert_eq!(text(&a), "ONE\n1\ntwo\n2\nthree", "the filter was not lost");
}

#[test]
fn filt_1_quitting_is_not_held_up_by_a_filter() {
    let mut a = named(THREE);
    feed(&mut a, ":1!sleep 30\n");
    let fx = feed(&mut a, ":q\n");
    assert!(fx.iter().any(|e| matches!(e, Effect::Quit(0))), "the shell stops the filter then");
}

// ---------------------------------------------------------------------------------------------
// FILT-2: the result
// ---------------------------------------------------------------------------------------------

#[test]
fn filt_2_the_output_takes_the_place_of_its_lines_as_one_undo_step() {
    let mut a = named(THREE);
    feed(&mut a, ":%!tr a-z A-Z\n");
    ok(&mut a, UPPER);
    assert_eq!(text(&a), UPPER.trim_end());
    assert!(a.buf.dirty() && a.buf.marks.is_empty());
    assert_eq!(st(&mut a), "5 lines from the filter · u to undo");
    feed(&mut a, "u");
    assert_eq!(
        (text(&a), a.buf.dirty()),
        (THREE.trim_end().to_string(), false),
        "one u takes it all back"
    );
    feed(&mut a, "<C-r>");
    assert_eq!(text(&a), UPPER.trim_end());
}

#[test]
fn filt_2_only_the_lines_that_differ_are_replaced_so_the_marks_elsewhere_stay() {
    let mut a = named("a\nb\nc\nd\n");
    a.update(Event::Disk {
        bytes: Some(b"a\nb\nc\nD\n".to_vec()),
        hash: None,
        deadline: far(),
        seq: 0,
    });
    let marks: Vec<_> = a.buf.marks.iter().map(|m| (m.line, m.kind)).collect();
    assert_eq!(marks, [(3, '~')]);
    feed(&mut a, ":1!x\n");
    ok(&mut a, "A\n");
    assert_eq!(text(&a), "A\nb\nc\nD");
    let marks: Vec<_> = a.buf.marks.iter().map(|m| (m.line, m.kind)).collect();
    assert_eq!(marks, [(3, '~')], "the mark of a change from the disk is where it was");
}

#[test]
fn filt_2_edits_made_meanwhile_are_kept() {
    let mut a = named(THREE);
    feed(&mut a, ":%!tr a-z A-Z\n");
    // While it runs: a line at the top, and a change in a line the filter leaves alone.
    feed(&mut a, "ggOnew<Esc>");
    feed(&mut a, "4j$a!<Esc>");
    assert_eq!(text(&a), "new\none\n1\ntwo\n2!\nthree");
    ok(&mut a, UPPER);
    assert_eq!(
        text(&a),
        "new\nONE\n1\nTWO\n2!\nTHREE",
        "the edits stay, and the output comes in around them"
    );
    feed(&mut a, "u");
    assert_eq!(text(&a), "new\none\n1\ntwo\n2!\nthree", "and u takes only the output back");
}

#[test]
fn filt_2_an_edit_on_a_line_the_filter_changed_conflicts_and_nothing_is_applied() {
    let mut a = named("one\ntwo\nthree\n");
    feed(&mut a, ":%!tr a-z A-Z\n");
    feed(&mut a, "jA!<Esc>");
    ok(&mut a, "ONE\nTWO\nTHREE\n");
    assert_eq!(text(&a), "one\ntwo!\nthree", "mine is as it was");
    assert_eq!(st(&mut a), "filter result discarded: the lines changed meanwhile (run it again)");
    // Run again, now that nothing has changed meanwhile.
    feed(&mut a, ":%!tr a-z A-Z\n");
    ok(&mut a, "ONE\nTWO!\nTHREE\n");
    assert_eq!(text(&a), "ONE\nTWO!\nTHREE");
}

#[test]
fn filt_2_a_failed_filter_is_discarded_and_its_stderr_is_shown() {
    let mut a = named(THREE);
    feed(&mut a, ":%!tr\n");
    done(&mut a, Some(2), "partial\n", "tr: missing operand\nTry 'tr --help'\n");
    assert_eq!(st(&mut a), "tr: missing operand");
    assert!(!a.buf.dirty() && text(&a) == THREE.trim_end());
    // Blank first lines are skipped; no stderr says what happened instead.
    for (status, err, said) in [
        (Some(1), "\n  \nsh: 1: x: not found\n", "sh: 1: x: not found"),
        (Some(3), "", "filter failed (exit 3)"),
        (None, "", "filter failed (killed)"),
    ] {
        feed(&mut a, ":1!x\n");
        done(&mut a, status, "ONE\n", err);
        assert_eq!((st(&mut a).as_str(), a.buf.dirty()), (said, false));
    }
    assert!(spawn_of(&feed(&mut a, ":1!x\n")).is_some(), "a failure ends the filter");
}

#[test]
fn filt_2_an_empty_output_is_a_valid_result_and_deletes_the_lines() {
    let mut a = named("a\nb\nc\n");
    feed(&mut a, ":2!true\n");
    ok(&mut a, "");
    assert_eq!(text(&a), "a\nc");
    // The whole buffer.
    let mut a = named("a\nb\n");
    feed(&mut a, ":%!true\n");
    ok(&mut a, "");
    assert_eq!(text(&a), "");
    assert!(a.buf.dirty());
}

#[test]
fn filt_2_an_output_that_changes_nothing_says_so() {
    let mut a = named(THREE);
    feed(&mut a, ":%!cat\n");
    ok(&mut a, THREE);
    assert_eq!((st(&mut a).as_str(), a.buf.dirty()), ("no changes from the filter", false));
}

#[test]
fn filt_2_the_output_may_be_longer_or_shorter_and_lacks_no_final_newline() {
    let mut a = named("a\nb\nc\n");
    feed(&mut a, ":2!x\n");
    ok(&mut a, "b1\nb2\nb3");
    assert_eq!(text(&a), "a\nb1\nb2\nb3\nc", "a final newline is added");
    let mut a = named("a\nb\nc\n");
    feed(&mut a, ":1,2!x\n");
    ok(&mut a, "ab\n");
    assert_eq!(text(&a), "ab\nc");
}

#[test]
fn filt_2_the_last_line_keeps_the_end_of_the_file_as_it_was() {
    let mut a = named("a\nb");
    feed(&mut a, ":2!tr a-z A-Z\n");
    ok(&mut a, "B\n");
    assert_eq!(text(&a), "a\nB");
    assert_eq!(a.buf.encode(), b"a\nB", "a file without a final newline has none after");
    let mut a = named("a\nb\n");
    feed(&mut a, ":2!tr a-z A-Z\n");
    ok(&mut a, "B");
    assert_eq!(a.buf.encode(), b"a\nB\n");
}

#[test]
fn filt_2_an_output_that_is_not_text_is_discarded() {
    for bad in [&b"\xff\xfe\n"[..], b"a\0b\n"] {
        let mut a = named("a\nb\n");
        feed(&mut a, ":1!x\n");
        a.update(Event::FilterDone(Some(0), bad.to_vec(), vec![], far()));
        assert_eq!(
            (st(&mut a).as_str(), a.buf.dirty()),
            ("filter output is not text: discarded", false)
        );
    }
}

#[test]
fn filt_2_a_later_result_comes_in_on_top_of_the_earlier_one() {
    let mut a = named(THREE);
    feed(&mut a, ":1!x\n");
    ok(&mut a, "ONE\n");
    feed(&mut a, ":5!x\n");
    ok(&mut a, "THREE\n");
    feed(&mut a, ":3!x\n");
    ok(&mut a, "TWO\n");
    assert_eq!(text(&a), UPPER.trim_end());
    feed(&mut a, "uu");
    assert_eq!(text(&a), "ONE\n1\ntwo\n2\nthree");
}

#[test]
fn filt_2_the_result_comes_in_any_mode_without_changing_it() {
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
        assert_eq!(a.vi.mode, mode, "{keys}");
        assert_eq!(text(&a), UPPER.trim_end(), "{keys}");
    }
    // Typing goes on in Insert mode after the result; the line it typed on conflicts with it.
    let mut a = named("one\n");
    feed(&mut a, ":1!x\nA ab");
    ok(&mut a, "ONE\n");
    feed(&mut a, "c<Esc>");
    assert_eq!(text(&a), "one abc");
}

#[test]
fn filt_2_an_answer_nobody_waits_for_is_ignored() {
    let mut a = named(THREE);
    let before = st(&mut a);
    ok(&mut a, UPPER);
    done(&mut a, Some(1), "", "boom");
    assert_eq!((st(&mut a), a.buf.dirty()), (before, false));
}

#[test]
fn filt_2_a_diff_that_gave_up_is_coarse_but_the_output_is_what_comes_in() {
    let mut a = named("a b c\nd e f\n");
    feed(&mut a, ":%!x\n");
    let late = Instant::now() - std::time::Duration::from_secs(1);
    let out = "a B c\nd E f\n";
    a.update(Event::FilterDone(Some(0), out.into(), vec![], late));
    assert_eq!(text(&a), out.trim_end());
}
