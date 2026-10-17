//! KEY-6: the first time a second file is open the message line says what moves between files, once
//! for the run, until Tab and Shift-Tab have been used three times; the count is kept in the state
//! directory.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use sim::io::{load_tab_uses, save_tab_uses};
use sim::sim::SimEffect;

const HINT: &str = "Tab / S-Tab: next / previous file";

fn rig_tab(name: &str, uses: u8) -> Rig {
    rig_env(name, &["a.txt=one\n", "b.txt=two\n", "c.txt=three\n"], &["a.txt"], (100, 24), |e| {
        e.tab_uses = uses
    })
}

fn uses(r: &Rig) -> Vec<u8> {
    r.log
        .iter()
        .filter_map(|e| if let SimEffect::TabUses(n) = e { Some(*n) } else { None })
        .collect()
}

#[test]
fn key_6_the_first_second_file_of_a_run_says_how_to_move_between_files_once() {
    let mut r = rig_tab("k6-hint", 0);
    assert!(!r.status().contains("Tab"), "one file: nothing to say: {}", r.status());
    r.keys(":e b.txt\n");
    assert!(r.status().starts_with(HINT), "the message line: {}", r.status());
    r.keys("j");
    assert!(
        !r.status().contains(HINT),
        "the next key puts it out, as for any message: {}",
        r.status()
    );
    r.keys(":e c.txt\n");
    assert!(!r.status().contains(HINT), "not again in this run: {}", r.status());
}

#[test]
fn key_6_three_uses_of_tab_or_shift_tab_end_it_for_good_and_each_is_kept() {
    let mut r = rig_tab("k6-count", 0);
    r.keys(":e b.txt\n");
    assert_eq!(uses(&r), Vec::<u8>::new());
    r.keys("<Tab>");
    r.keys("<S-Tab>");
    assert_eq!(uses(&r), [1, 2], "Tab and Shift-Tab, each that showed another file");
    r.keys("<Tab>");
    r.keys("<Tab><Tab>");
    assert_eq!(uses(&r), [1, 2, 3], "never more than three");
    assert_eq!(r.sim.tab_uses, 3);
    // a run that begins with three uses never says it
    let mut learned = rig_tab("k6-learned", 3);
    learned.keys(":e b.txt\n");
    assert!(!learned.status().contains(HINT), "{}", learned.status());
    // two uses are not enough: it is said again in the next run
    let mut two = rig_tab("k6-two", 2);
    two.keys(":e b.txt\n");
    assert!(two.status().starts_with(HINT), "{}", two.status());
}

#[test]
fn key_6_a_tab_that_shows_no_other_file_and_the_other_ways_are_not_uses() {
    let mut r = rig_tab("k6-notuse", 0);
    r.keys("<Tab><S-Tab>"); // one file: nothing is shown
    assert_eq!(uses(&r), Vec::<u8>::new());
    r.keys(":e b.txt\n");
    r.keys(" bpa"); // the letters
    r.keys(":e a.txt\n:e b.txt\n");
    assert_eq!(uses(&r), Vec::<u8>::new(), "<Space>bp is another way, and the hint teaches Tab");
}

#[test]
fn key_6_the_count_is_kept_as_a_number_in_a_file_of_the_state_directory() {
    let dir = temp_dir("k6-file");
    assert_eq!(load_tab_uses(&dir.0), 0, "none yet");
    save_tab_uses(&dir.0, 2).unwrap();
    assert_eq!(std::fs::read_to_string(dir.0.join("tab-uses")).unwrap(), "2\n");
    assert_eq!(load_tab_uses(&dir.0), 2);
    for (junk, want) in [("", 0), ("many\n", 0), ("-1\n", 0), ("9\n", 3), ("300\n", 0)] {
        std::fs::write(dir.0.join("tab-uses"), junk).unwrap();
        assert_eq!(load_tab_uses(&dir.0), want, "{junk:?}");
    }
}
