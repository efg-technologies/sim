//! MARK-2: the likelihood of opening a file from the history of opening files, and the history
//! file in sim's state directory.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use proptest::prelude::*;
use sim::{io, rank::History};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

const MIN: u64 = 60;
const HOUR: u64 = 3600;
const DAY: u64 = 24 * HOUR;
const NOW: u64 = 1_800_000_000;

fn opened(list: &[(&str, &[u64])]) -> History {
    let mut h = History::default();
    for (path, ages) in list {
        for age in *ages {
            h.record(path, NOW - age);
        }
    }
    h
}

#[test]
fn mark_2_one_recent_open_beats_many_old_ones() {
    let h = opened(&[
        ("/p/recent", &[2 * MIN]),
        ("/p/old", &[7 * DAY, 8 * DAY, 9 * DAY, 10 * DAY, 11 * DAY, 12 * DAY]),
    ]);
    assert!(h.likelihood("/p/recent", NOW) > h.likelihood("/p/old", NOW));
}

#[test]
fn mark_2_opens_add_up_so_frequency_counts_too() {
    let h = opened(&[
        ("/p/once", &[HOUR]),
        ("/p/often", &[HOUR, 2 * HOUR, 5 * HOUR, DAY]),
        ("/p/often_but_ago", &[30 * DAY, 31 * DAY, 32 * DAY, 33 * DAY]),
    ]);
    let l = |p: &str| h.likelihood(p, NOW);
    assert!(l("/p/often") > l("/p/once"), "the same last open, but more of them");
    assert!(l("/p/once") > l("/p/often_but_ago"), "a month ago, even four times, fades");
    assert_eq!(l("/p/never"), 0.0);
}

#[test]
fn mark_2_the_likelihood_falls_with_time_and_never_blows_up() {
    let h = opened(&[("/p/a", &[0])]);
    let at = |t: u64| h.likelihood("/p/a", NOW + t);
    assert!(at(0).is_finite() && at(0) == at(MIN), "within a minute counts alike");
    assert!(
        at(MIN) > at(HOUR) && at(HOUR) > at(DAY) && at(DAY) > at(365 * DAY) && at(365 * DAY) > 0.0
    );
    // A clock that went backwards after the open is not a negative age.
    assert!(h.likelihood("/p/a", NOW - DAY).is_finite());
}

#[test]
fn mark_2_top_lists_the_likeliest_files_under_a_root_relative_to_it() {
    let h = opened(&[
        ("/home/me/proj/a.rs", &[DAY]),
        ("/home/me/proj/src/b.rs", &[MIN]),
        ("/home/me/proj/src/c.rs", &[HOUR, 2 * HOUR]),
        ("/home/me/other/z.rs", &[0]),
        ("/home/me/project2/y.rs", &[0]),
    ]);
    let top = h.top("/home/me/proj", 5, NOW);
    assert_eq!(
        top.iter().map(|t| t.0.as_str()).collect::<Vec<_>>(),
        ["src/b.rs", "src/c.rs", "a.rs"]
    );
    assert!(top.windows(2).all(|w| w[0].1 >= w[1].1));
    assert_eq!(h.top("/home/me/proj/", 2, NOW).len(), 2, "a trailing slash is fine");
    assert_eq!(h.top("/home/me/proj", 0, NOW), []);
    assert_eq!(h.top("/nowhere", 5, NOW), []);
}

#[test]
fn mark_2_the_history_is_bounded() {
    let mut h = History::default();
    for i in 0..100 {
        h.record("/p/busy", NOW - 100 * DAY + i * HOUR);
    }
    let line = h.serialize();
    assert_eq!(line.lines().count(), 1);
    assert_eq!(line.split('\t').next().unwrap().split(',').count(), 16, "16 opens a path at most");
    // 1,000 paths at most: the least likely go.
    for i in 0..1500 {
        h.record(&format!("/p/file{i}"), NOW - 1500 * MIN + i * MIN);
    }
    let kept = h.serialize().lines().count();
    assert_eq!(kept, 1000);
    // The least likely go: the oldest single opens, not the newest ones.
    assert!(h.likelihood("/p/file1499", NOW) > 0.0 && h.likelihood("/p/file0", NOW) == 0.0);
}

#[test]
fn mark_2_paths_with_line_breaks_are_not_remembered() {
    let mut h = History::default();
    h.record("/p/a\nb", NOW);
    h.record("/p/a\rb", NOW);
    h.record("/p/tab\there", NOW);
    assert_eq!(h.likelihood("/p/a\nb", NOW), 0.0);
    // A tab is fine: the times come first, so the first tab ends them.
    assert_eq!(History::parse(&h.serialize()), h);
    assert!(h.likelihood("/p/tab\there", NOW) > 0.0);
}

#[test]
fn mark_2_a_damaged_history_file_loses_only_the_damaged_lines() {
    let good = "5\t/p/a\n";
    let text = format!(
        "{good}no tab on this line\nzz,1\t/p/b\n\t/p/empty-times\n1,\t/p/trailing\n1\t\n10000000000000000\t/p/overflow\n+5\t/p/c\n\u{0}\t/p/nul\n"
    );
    let h = History::parse(&text);
    assert!(h.likelihood("/p/a", 100) > 0.0);
    for gone in ["/p/b", "/p/empty-times", "/p/trailing", "/p/overflow", "/p/nul", ""] {
        assert_eq!(h.likelihood(gone, 100), 0.0, "{gone:?}");
    }
    assert_eq!(History::parse("").serialize(), "");
    assert_eq!(History::parse("\u{0}\u{1}\n\n\n").serialize(), "");
}

#[test]
fn mark_2_a_history_file_with_too_much_in_it_is_cut_down_when_read() {
    let mut text = String::new();
    for i in 0..3000 {
        let times: Vec<String> =
            (0..40).map(|j| format!("{:x}", if j == 0 { NOW - i * MIN } else { 1 })).collect();
        text += &format!("{}\t/p/f{i}\n", times.join(","));
    }
    let h = History::parse(&text);
    let out = h.serialize();
    assert_eq!(out.lines().count(), 1000);
    assert!(out.lines().all(|l| l.split('\t').next().unwrap().split(',').count() <= 16));
}

proptest! {
    #[test]
    fn mark_2_the_serialized_history_reads_back_the_same(
        list in proptest::collection::vec(("(/[a-z日 .]{1,6}){1,3}", proptest::collection::vec(1..2_000_000_000u64, 1..20)), 0..30)
    ) {
        let mut h = History::default();
        for (path, times) in &list {
            for t in times { h.record(path, *t); }
        }
        let text = h.serialize();
        prop_assert_eq!(History::parse(&text), h.clone());
        prop_assert_eq!(History::parse(&text).serialize(), text);
    }
}

#[test]
fn mark_2_the_state_directory_is_never_a_shared_one() {
    let os = |s: &str| Some(std::ffi::OsString::from(s));
    assert_eq!(io::state_dir(os("/x/state"), os("/home/me")), Some("/x/state/sim".into()));
    assert_eq!(io::state_dir(None, os("/home/me")), Some("/home/me/.local/state/sim".into()));
    assert_eq!(io::state_dir(os(""), os("/home/me")), Some("/home/me/.local/state/sim".into()));
    assert_eq!(
        io::state_dir(os("relative/dir"), os("/home/me")),
        Some("/home/me/.local/state/sim".into()),
        "XDG says to ignore a relative path"
    );
    // Nothing to build on: no directory at all, not /tmp.
    assert_eq!(io::state_dir(None, None), None);
    assert_eq!(io::state_dir(os("rel"), os("rel")), None);
}

#[test]
fn mark_2_the_history_is_saved_whole_in_a_private_directory() {
    let t = temp_dir("mark2-save");
    let dir = t.0.join("state/sim"); // neither level exists yet
    assert_eq!(io::load_history(&dir), History::default(), "no file yet");
    let h = opened(&[("/p/a", &[MIN, HOUR]), ("/p/b", &[DAY])]);
    io::save_history(&dir, &h).unwrap();
    assert_eq!(io::load_history(&dir), h);
    let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&dir), 0o700);
    assert_eq!(mode(&dir.join("history")), 0o600);
    // A second save replaces the first; no temporary file is left.
    let h2 = opened(&[("/p/c", &[MIN])]);
    io::save_history(&dir, &h2).unwrap();
    assert_eq!(io::load_history(&dir), h2);
    let names: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(names, ["history"]);
}

#[test]
fn mark_2_a_save_that_fails_says_why_and_leaves_no_litter() {
    let t = temp_dir("mark2-fail");
    let h = opened(&[("/p/a", &[MIN])]);
    // The state directory cannot be made: something that is not a directory is in the way.
    std::fs::write(t.0.join("file"), "x").unwrap();
    let err = io::save_history(&t.0.join("file/sim"), &h).unwrap_err();
    assert!(err.starts_with("history: cannot create"), "{err}");
    // The final rename cannot happen: a directory is where the history belongs.
    let dir = t.0.join("sim");
    std::fs::create_dir_all(dir.join("history")).unwrap();
    let err = io::save_history(&dir, &h).unwrap_err();
    assert!(err.starts_with("history: cannot write"), "{err}");
    let names: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(names, ["history"], "the temporary file was removed");
}

#[test]
fn mark_2_a_history_directory_that_is_a_link_is_not_written_through() {
    // The state directory can be somewhere others can make links (an `XDG_STATE_HOME` in a shared
    // directory): writing through a link would put the file, and the mode, in any directory.
    let t = temp_dir("mark2-link");
    let elsewhere = t.0.join("elsewhere");
    std::fs::create_dir(&elsewhere).unwrap();
    std::fs::set_permissions(&elsewhere, std::fs::Permissions::from_mode(0o755)).unwrap();
    let dir = t.0.join("sim");
    std::os::unix::fs::symlink(&elsewhere, &dir).unwrap();
    let err = io::save_history(&dir, &opened(&[("/p/a", &[MIN])])).unwrap_err();
    assert!(err.starts_with("history: cannot use") && err.contains("link"), "{err}");
    assert_eq!(std::fs::read_dir(&elsewhere).unwrap().count(), 0, "nothing was written there");
    let mode = std::fs::metadata(&elsewhere).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o755, "and its mode was not changed");
}

#[test]
fn mark_2_an_unreadable_or_foreign_history_file_loads_as_empty() {
    let t = temp_dir("mark2-bad");
    std::fs::write(t.0.join("history"), [0xff, 0xfe, 0x00, 0x80]).unwrap();
    assert_eq!(io::load_history(&t.0), History::default());
    std::fs::remove_file(t.0.join("history")).unwrap();
    std::fs::create_dir(t.0.join("history")).unwrap();
    assert_eq!(io::load_history(&t.0), History::default());
}
